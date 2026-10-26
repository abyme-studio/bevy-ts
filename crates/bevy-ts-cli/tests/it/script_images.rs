//! script-image-bytes: images made, rewritten, read and removed from TS bytes through
//! `ResMut<Assets<Image>>` (the engine's `Images`), checked against `Assets<Image>` and its
//! `AssetEvent`s. Headless, no GPU.

use crate::common;

use bevy::asset::AssetEvent;
use bevy::ecs::message::{MessageCursor, Messages};
use bevy::image::{ImageFilterMode, ImageSampler};
use bevy::prelude::*;
use bevy::render::render_resource::TextureFormat;
use bevy_ts::BevyTsPlugin;
use bevy_ts::generation::DevSource;
use bevy_ts::reflect_asset::ScriptAssets;
use bevy_ts_cli::compile::Compiler;
use bevy_ts_v8::V8Options;
use common::*;
use std::sync::Arc;

const IMPORTS: &str = r#"import { app, assets, Startup, Update } from "bevy-ts";
import type { Assets, AssetRef, ResMut } from "bevy-ts";
import { Image, TextureFormat } from "bevy-ts/bevy";
"#;

/// A headless app with images and `Images`, its script `body` after [`IMPORTS`], loaded.
fn image_app(name: &str, body: &str) -> App {
    let dir = project(&format!("images-{name}"), &[("game.ts", &format!("{IMPORTS}{body}"))]);
    let mut app = App::new();
    app.add_plugins((MinimalPlugins, AssetPlugin::default(), bevy::image::ImagePlugin::default()))
        .add_plugins(BevyTsPlugin::new(bevy_ts_v8::factory(V8Options::default())).dev(true))
        .add_plugins(bevy_ts_cli::assets::ScriptAssetsPlugin);
    app.insert_resource(DevSource(Arc::new(Compiler::new(dir.join("game.ts"), true).unwrap())));
    app.finish();
    app.cleanup();
    let r = reload(&mut app, false);
    assert!(r.errors.is_empty(), "load failed: {:?}", r.errors);
    app
}

/// What the script logged after `prefix`.
fn logged(app: &App, prefix: &str) -> String {
    let m = messages(app);
    m.iter().find_map(|l| l.strip_prefix(prefix)).unwrap_or_else(|| panic!("no {prefix:?} in {m:?}")).to_string()
}

fn handle(app: &App, prefix: &str) -> Handle<Image> {
    let n: u64 = logged(app, prefix).trim().parse().unwrap();
    app.world().resource::<ScriptAssets>().handle(n).unwrap().typed::<Image>()
}

fn image(app: &App, h: &Handle<Image>) -> Image {
    app.world().resource::<Assets<Image>>().get(h).expect("the image is in Assets<Image>").clone()
}

fn modified(app: &App, cursor: &mut MessageCursor<AssetEvent<Image>>, h: &Handle<Image>) -> usize {
    cursor.read(app.world().resource::<Messages<AssetEvent<Image>>>()).filter(|e| e.is_modified(h)).count()
}

/// The 32 bytes of a 4×2 RGBA image: 1, 2, 3, ...
const BYTES: &str = "const bytes = new Uint8Array(32).map((_, i) => i + 1);\n";

#[test]
fn an_image_from_bytes() {
    let mut app = image_app(
        "new",
        &format!(
            r#"{BYTES}
export function make(images: ResMut<Assets<Image>>) {{
  const r: AssetRef<Image> = images.add(Image.new([4, 2], TextureFormat.Rgba8UnormSrgb, bytes));
  console.log(`made ${{r.$asset}}`);
}}
app.addSystems(Startup, make);
"#
        ),
    );
    updates(&mut app, 2);
    let h = handle(&app, "made ");
    let img = image(&app, &h);
    assert_eq!(img.size(), UVec2::new(4, 2));
    assert_eq!(img.texture_descriptor.format, TextureFormat::Rgba8UnormSrgb);
    assert_eq!(img.data.as_deref().unwrap(), &(1..=32).collect::<Vec<u8>>()[..]);
    assert!(img.texture_descriptor.usage.contains(bevy::render::render_resource::TextureUsages::COPY_DST));
}

#[test]
fn the_wrong_length_throws_and_adds_nothing() {
    let mut app = image_app(
        "length",
        r#"
export function make(images: ResMut<Assets<Image>>) {
  try {
    images.add(Image.new([4, 2], TextureFormat.Rgba8Unorm, new Uint8Array(31)));
    console.log("added");
  } catch (e) { console.log(`threw ${(e as Error).message}`); }
  try {
    images.add(Image.new([0, 2], TextureFormat.Rgba8Unorm, new Uint8Array(0)));
    console.log("added");
  } catch (e) { console.log(`zero ${(e as Error).message}`); }
}
app.addSystems(Startup, make);
"#,
    );
    // ImagePlugin's own defaults are there before the script runs.
    let before = app.world().resource::<Assets<Image>>().len();
    updates(&mut app, 2);
    let m = logged(&app, "threw ");
    assert!(m.contains("4×2") && m.contains("Rgba8Unorm") && m.contains("32 bytes"), "{m}");
    assert!(logged(&app, "zero ").contains("0×2 is not a size"), "{}", logged(&app, "zero "));
    assert_eq!(app.world().resource::<Assets<Image>>().len(), before);
}

#[test]
fn a_sampler_at_creation() {
    let mut app = image_app(
        "sampler",
        &format!(
            r#"{BYTES}
export function make(images: ResMut<Assets<Image>>) {{
  const r = images.add(Image.new([4, 2], TextureFormat.Rgba8Unorm, bytes, {{ sampler: {{ Descriptor: {{ mag_filter: "Nearest", min_filter: "Nearest" }} }} }}));
  console.log(`made ${{r.$asset}}`);
  console.log(`read ${{JSON.stringify(assets.get(r)!.sampler)}}`);
}}
app.addSystems(Startup, make);
"#
        ),
    );
    updates(&mut app, 2);
    let ImageSampler::Descriptor(d) = image(&app, &handle(&app, "made ")).sampler else { panic!("a descriptor") };
    assert_eq!((d.mag_filter, d.min_filter), (ImageFilterMode::Nearest, ImageFilterMode::Nearest));
    let read: serde_json::Value = serde_json::from_str(&logged(&app, "read ")).unwrap();
    assert_eq!(read["Descriptor"]["mag_filter"], "Nearest");
    assert_eq!(read["Descriptor"]["min_filter"], "Nearest");
}

/// Adds a 4×2 RGBA image of zeros at startup; each update runs `body` (with `img`, the live
/// image, and `frame`) once the image is in.
fn rewriting(name: &str, body: &str) -> App {
    image_app(
        name,
        &format!(
            r#"
let r: AssetRef<Image> | undefined;
let frame = 0;
export function make(images: ResMut<Assets<Image>>) {{
  r = images.add(Image.new([4, 2], TextureFormat.Rgba8Unorm, new Uint8Array(32)));
  console.log(`made ${{r.$asset}}`);
}}
export function write(images: ResMut<Assets<Image>>) {{
  frame++;
  const img = images.getMut(r!)!;
  {body}
}}
app.addSystems(Startup, make);
app.addSystems(Update, write);
"#
        ),
    )
}

#[test]
fn the_whole_image_is_rewritten_with_one_event() {
    let mut app = rewriting("whole", "if (frame === 2) img.setData(new Uint8Array(32).fill(9));");
    updates(&mut app, 1);
    let h = handle(&app, "made ");
    let ptr = |app: &App| app.world().resource::<Assets<Image>>().get(&h).unwrap().data.as_ref().unwrap().as_ptr() as usize;
    let before = ptr(&app);
    let mut cursor = app.world().resource::<Messages<AssetEvent<Image>>>().get_cursor_current();
    app.update();
    assert_eq!(image(&app, &h).data.unwrap(), vec![9; 32]);
    assert_eq!(modified(&app, &mut cursor, &h), 1);
    assert_eq!(ptr(&app), before, "the data is copied into the image's own Vec");
}

#[test]
fn a_rect_is_rewritten() {
    let mut app = rewriting("rect", "if (frame === 2) img.writeRect(1, 0, 2, 2, new Uint8Array(16).map((_, i) => i + 1));");
    updates(&mut app, 2);
    let data = image(&app, &handle(&app, "made ")).data.unwrap();
    let px = |x: usize, y: usize| data[(y * 4 + x) * 4..(y * 4 + x) * 4 + 4].to_vec();
    assert_eq!(px(1, 0), [1, 2, 3, 4]);
    assert_eq!(px(2, 0), [5, 6, 7, 8]);
    assert_eq!(px(1, 1), [9, 10, 11, 12]);
    assert_eq!(px(2, 1), [13, 14, 15, 16]);
    for (x, y) in [(0, 0), (3, 0), (0, 1), (3, 1)] {
        assert_eq!(px(x, y), [0; 4], "({x}, {y})");
    }
}

#[test]
fn a_rect_outside_throws_and_changes_nothing() {
    let mut app = rewriting(
        "outside",
        r#"if (frame === 2) {
    try { img.writeRect(3, 0, 2, 1, new Uint8Array(8).fill(5)); console.log("wrote"); }
    catch (e) { console.log(`threw ${(e as Error).message}`); }
    try { img.setData(new Uint8Array(31)); console.log("wrote"); }
    catch (e) { console.log(`short ${(e as Error).message}`); }
  }"#,
    );
    updates(&mut app, 1);
    let h = handle(&app, "made ");
    let mut cursor = app.world().resource::<Messages<AssetEvent<Image>>>().get_cursor_current();
    app.update();
    app.update();
    let m = logged(&app, "threw ");
    assert!(m.contains("2×1 at (3, 0)") && m.contains("4×2"), "{m}");
    assert!(logged(&app, "short ").contains("32 bytes, not 31"), "{}", logged(&app, "short "));
    assert_eq!(image(&app, &h).data.unwrap(), vec![0; 32]);
    assert_eq!(modified(&app, &mut cursor, &h), 0, "a failed write marks nothing");
}

#[test]
fn two_writes_send_one_event() {
    let mut app = rewriting("two", "if (frame >= 2) { img.setData(new Uint8Array(32).fill(frame)); img.writeRect(0, 0, 1, 1, new Uint8Array(4)); }");
    updates(&mut app, 1);
    let h = handle(&app, "made ");
    let mut cursor = app.world().resource::<Messages<AssetEvent<Image>>>().get_cursor_current();
    app.update();
    assert_eq!(modified(&app, &mut cursor, &h), 1);
    app.update();
    assert_eq!(modified(&app, &mut cursor, &h), 1, "and one again the next frame");
}

#[test]
fn an_image_reads_back() {
    let mut app = image_app(
        "read",
        &format!(
            r#"{BYTES}
export function make(images: ResMut<Assets<Image>>) {{
  const r = images.add(Image.new([4, 2], TextureFormat.Rgba8UnormSrgb, bytes));
  const img = images.get(r)!;
  const data = img.readData();
  const out = img.readData(new Uint8Array(40));
  console.log(`size ${{img.size.join("x")}}`);
  console.log(`format ${{img.format === TextureFormat.Rgba8UnormSrgb}}`);
  console.log(`data ${{data.every((b, i) => b === bytes[i]) && data.length === 32}}`);
  console.log(`out ${{out.length === 40 && out[31] === 32}}`);
  try {{ img.setData(bytes); console.log("wrote"); }} catch (e) {{ console.log(`readonly ${{(e as Error).message}}`); }}
}}
app.addSystems(Startup, make);
"#
        ),
    );
    updates(&mut app, 2);
    assert_eq!(logged(&app, "size "), "4x2");
    assert_eq!(logged(&app, "format "), "true");
    assert_eq!(logged(&app, "data "), "true");
    assert_eq!(logged(&app, "out "), "true");
    assert!(logged(&app, "readonly ").contains("images.getMut"));
}

#[test]
fn remove_is_bevys_and_stales_the_ref() {
    let mut app = image_app(
        "remove",
        &format!(
            r#"{BYTES}
export function make(images: ResMut<Assets<Image>>) {{
  const r = images.add(Image.new([4, 2], TextureFormat.Rgba8Unorm, bytes));
  console.log(`made ${{r.$asset}}`);
  console.log(`removed ${{images.remove(r)}}`);
  try {{ images.get(r); console.log("read"); }} catch (e) {{ console.log(`stale ${{(e as Error).message}}`); }}
}}
app.addSystems(Startup, make);
"#
        ),
    );
    let before = app.world().resource::<Assets<Image>>().len();
    updates(&mut app, 2);
    assert_eq!(logged(&app, "removed "), "true");
    assert!(logged(&app, "stale ").contains("stale asset ref"), "{}", logged(&app, "stale "));
    assert_eq!(app.world().resource::<Assets<Image>>().len(), before);
}
