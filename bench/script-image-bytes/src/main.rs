//! What a 1024² RGBA8 image update costs from TS beside the same copy in Rust (script-image-bytes
//! research, docs/research/script-image-bytes.md). The image is 4 MiB; TS passes one `Uint8Array`
//! to an exported method, viewed in place, and Rust copies it once into the image's own `Vec`.
//!
//! - (a) Rust: `Assets::get_mut` and one `copy_from_slice` of the whole image, the floor.
//! - (b) TS: one call with the whole image, its bytes made once (one byte changed per frame).
//! - (c) TS: as (b), every pixel written by JS first (a `Uint32Array` loop over 1M pixels).
//! - (d) TS: one call with a 256×256 sub-rect, copied row by row.
//! - (e) Rust: `Image::clone`, what Bevy's extraction adds per `Modified` when the image stays in
//!   the main world (`RenderAssetUsages::default()`).
//! - (f) TS: `images.getMut(ref).setData(bytes)` through the engine's `Images`
//!   (`ResMut<Assets<Image>>`), what a game writes; (b) is a bench-only export.
//! - (0) TS: the same system with nothing to do, subtracted from the TS arms.
//!
//! Each arm runs 3 times (8 when arms are within noise, `RUNS=8`), 200 frames after 20 to warm up,
//! in a fresh app. A TS arm is timed by the system's own run time (`TsSystems`' `last_run_us`), a
//! Rust arm inside its system. The GPU upload (Bevy's `write_texture`) is not here: it is the same
//! for Rust and TS. Run by the `image-bytes-bench` process (`run.sh`), outside the gate's build
//! (docs/testing.md rule 8).

use bevy::asset::RenderAssetUsages;
use bevy::ecs::system::SystemParam;
use bevy::prelude::*;
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};
use bevy_ts::BevyTsPlugin;
use bevy_ts::generation::{DevSource, ReloadReport};
use bevy_ts_cli::compile::Compiler;
use bevy_ts_native::prelude::*;
use bevy_ts_v8::V8Options;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Instant;

const SIDE: u32 = 1024;
const FRAMES: usize = 200;
const WARM: usize = 20;

/// Runs per arm: `RUNS`, 3 unless given; 8 when arms come out within noise of each other.
fn runs() -> usize {
    std::env::var("RUNS").ok().and_then(|n| n.parse().ok()).unwrap_or(3)
}

#[derive(Resource)]
struct Target(Handle<Image>);

#[derive(SystemParam)]
pub struct Pixels<'w> {
    target: Res<'w, Target>,
    images: ResMut<'w, Assets<Image>>,
}

#[export(ts = "Pixels", module = "bevy-ts/bench")]
impl Pixels<'_> {
    /// Overwrites the whole image.
    pub fn write(&mut self, data: &[u8]) -> Result<(), String> {
        let mut image = self.images.get_mut(&self.target.0).ok_or("no image")?;
        let dst = image.data.as_mut().ok_or("no data")?;
        if dst.len() != data.len() {
            return Err(format!("{} bytes for an image of {}", data.len(), dst.len()));
        }
        dst.copy_from_slice(data);
        Ok(())
    }

    /// Overwrites the `cols`×`rows` rect at `x`, `y` from tightly packed rows.
    pub fn write_rect(&mut self, x: u32, y: u32, cols: u32, rows: u32, data: &[u8]) -> Result<(), String> {
        let mut image = self.images.get_mut(&self.target.0).ok_or("no image")?;
        let width = image.width() as usize;
        let dst = image.data.as_mut().ok_or("no data")?;
        let (x, y, w, h) = (x as usize, y as usize, cols as usize, rows as usize);
        if data.len() != w * h * 4 || (y + h) * width * 4 > dst.len() || x + w > width {
            return Err("rect out of range".into());
        }
        for (row, src) in data.chunks_exact(w * 4).enumerate() {
            let at = ((y + row) * width + x) * 4;
            dst[at..at + w * 4].copy_from_slice(src);
        }
        Ok(())
    }
}

const TS: &str = r#"import { app, Update } from "bevy-ts";
import type { AssetRef, Assets, ResMut } from "bevy-ts";
import type { Pixels } from "bevy-ts/bench";
import { Image, TextureFormat } from "bevy-ts/bevy";

const ARM = "__ARM__";
const side = 1024;
const bytes = new Uint8Array(side * side * 4);
const pixels32 = new Uint32Array(bytes.buffer);
const rect = new Uint8Array(256 * 256 * 4);
let frame = 0;
export function paint(p: Pixels) {
  frame++;
  if (ARM === "empty") return;
  if (ARM === "full") {
    bytes[0] = frame & 255;
    p.write(bytes);
  } else if (ARM === "fill") {
    const owner = frame & 7;
    for (let i = 0; i < pixels32.length; i++) pixels32[i] = 0xff000000 | ((i + owner) & 7) * 0x202020;
    p.write(bytes);
  } else if (ARM === "rect") {
    rect[0] = frame & 255;
    p.writeRect(384, 384, 256, 256, rect);
  }
}
let image: AssetRef<Image> | undefined;
export function set(images: ResMut<Assets<Image>>) {
  frame++;
  image ??= images.add(Image.new([side, side], TextureFormat.Rgba8UnormSrgb, bytes));
  bytes[0] = frame & 255;
  images.getMut(image)!.setData(bytes);
}
app.addSystems(Update, ARM === "set" ? set : paint);
"#;

/// A fresh directory holding `game.ts`, under the temp dir.
fn project(name: &str, src: &str) -> PathBuf {
    let dir = std::env::temp_dir().join("bevy-ts-bench").join(format!("{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("game.ts"), src).unwrap();
    dir
}

fn reload(app: &mut App) -> ReloadReport {
    bevy_ts::generation::reload_now(app.world_mut(), false).unwrap()
}

fn image() -> Image {
    let size = Extent3d { width: SIDE, height: SIDE, depth_or_array_layers: 1 };
    Image::new_fill(size, TextureDimension::D2, &[0, 0, 0, 255], TextureFormat::Rgba8UnormSrgb, RenderAssetUsages::default())
}

fn base_app() -> App {
    let mut app = App::new();
    app.add_plugins((MinimalPlugins, AssetPlugin::default())).init_asset::<Image>();
    let h = app.world_mut().resource_mut::<Assets<Image>>().add(image());
    app.insert_resource(Target(h));
    app
}

fn ts_app(arm: &str) -> App {
    let dir = project(&format!("image-bench-{arm}"), &TS.replace("__ARM__", arm));
    let mut app = base_app();
    app.add_plugins(BevyTsPlugin::new(bevy_ts_v8::factory(V8Options::default())).dev(true));
    app.add_plugins(bevy_ts_cli::assets::ScriptAssetsPlugin);
    app.ts_param::<Pixels<'static>>();
    app.insert_resource(DevSource(Arc::new(Compiler::new(&dir.join("game.ts"), true).unwrap())));
    app.finish();
    app.cleanup();
    let r = reload(&mut app);
    assert!(r.errors.is_empty(), "{:?}", r.errors);
    app
}

/// The Rust arms' source bytes and the last frame's time, µs.
#[derive(Resource)]
struct Rust {
    src: Vec<u8>,
    frame: u8,
    took: f64,
}

fn rust_copy(mut r: ResMut<Rust>, target: Res<Target>, mut images: ResMut<Assets<Image>>) {
    let t = Instant::now();
    r.frame = r.frame.wrapping_add(1);
    r.src[0] = r.frame;
    let mut image = images.get_mut(&target.0).unwrap();
    image.data.as_mut().unwrap().copy_from_slice(&r.src);
    drop(image);
    r.took = t.elapsed().as_secs_f64() * 1e6;
}

fn rust_clone(mut r: ResMut<Rust>, target: Res<Target>, images: Res<Assets<Image>>) {
    let t = Instant::now();
    let copy = images.get(&target.0).unwrap().clone();
    std::hint::black_box(&copy);
    drop(copy);
    r.took = t.elapsed().as_secs_f64() * 1e6;
}

fn rust_app(clone: bool) -> App {
    let mut app = base_app();
    app.insert_resource(Rust { src: vec![7; (SIDE * SIDE * 4) as usize], frame: 0, took: 0.0 });
    if clone {
        app.add_systems(Update, rust_clone);
    } else {
        app.add_systems(Update, rust_copy);
    }
    app
}

/// Update times in µs of one run: the TS system's run, or the Rust system's own timing.
fn run(mut app: App) -> Vec<f64> {
    for _ in 0..WARM {
        app.update();
    }
    (0..FRAMES)
        .map(|_| {
            app.update();
            match app.world().get_resource::<Rust>() {
                Some(r) => r.took,
                None => app.world().resource::<bevy_ts::systems::TsSystems>().iter().next().unwrap().last_run_us as f64,
            }
        })
        .collect()
}

fn pct(sorted: &[f64], p: f64) -> f64 {
    sorted[((sorted.len() - 1) as f64 * p).round() as usize]
}

/// Median and p95 µs over every run's frames.
fn arm(make: &dyn Fn() -> App) -> (f64, f64) {
    let mut all: Vec<f64> = (0..runs()).flat_map(|_| run(make())).collect();
    all.sort_by(f64::total_cmp);
    (pct(&all, 0.5), pct(&all, 0.95))
}

fn main() {
    let (empty, _) = arm(&|| ts_app("empty"));
    let mib = (SIDE * SIDE * 4) as f64 / (1024.0 * 1024.0);
    let arms: [(&str, &str, f64, f64, Box<dyn Fn() -> App>); 6] = [
        ("a", "Rust get_mut + copy_from_slice, whole image (floor)", 0.0, mib, Box::new(|| rust_app(false))),
        ("b", "TS one write(bytes), whole image", empty, mib, Box::new(|| ts_app("full"))),
        ("c", "TS every pixel written in JS, then write(bytes)", empty, mib, Box::new(|| ts_app("fill"))),
        ("d", "TS one writeRect, 256x256", empty, 0.25, Box::new(|| ts_app("rect"))),
        ("f", "TS images.getMut(ref).setData(bytes), whole image", empty, mib, Box::new(|| ts_app("set"))),
        ("e", "Rust Image::clone (extraction, main world kept)", 0.0, mib, Box::new(|| rust_app(true))),
    ];
    println!("{SIDE}x{SIDE} RGBA8 ({mib} MiB), {} runs x {FRAMES} frames; µs per update, a TS system's own run ({empty:.1} µs median) subtracted from TS arms", runs());
    println!("| arm | what | median µs | p95 µs | GB/s |");
    println!("|---|---|---|---|---|");
    for (k, what, base, mib, make) in arms {
        let (med, p95) = arm(&*make);
        let (med, p95) = (med - base, p95 - base);
        println!("| {k} | {what} | {med:.1} | {p95:.1} | {:.1} |", mib * 1.048_576e-3 / (med * 1e-6));
    }
}
