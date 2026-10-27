//! Change pixel-art-2d, stage 4: `bevy-ts.json`'s `imageSampler`, the `ImagePlugin` default sampler
//! `base_app` sets. Two red-and-blue sprites drawn through `base_app`: one on the default sampler,
//! one given a linear sampler of its own. macOS (Metal).
#![cfg(target_os = "macos")]

use crate::common::*;
use bevy::prelude::*;
use bevy::render::view::screenshot::{Screenshot, ScreenshotCaptured};
use bevy_ts::generation::{DevSource, ReloadStatus};
use bevy_ts_cli::cli::{Display, Engine, Prebuilt, base_app};
use bevy_ts_cli::compile::Compiler;
use bevy_ts_cli::headless::{Offscreen, SIZE};
use bevy_ts_cli::project::{DefaultSampler, Switches};
use std::sync::{Arc, Mutex};

/// A 2 x 1 PNG: a red texel, then a blue one.
fn red_blue() -> Vec<u8> {
    let mut out = Vec::new();
    let mut enc = png::Encoder::new(&mut out, 2, 1);
    enc.set_color(png::ColorType::Rgba);
    enc.set_depth(png::BitDepth::Eight);
    enc.write_header().unwrap().write_image_data(&[255, 0, 0, 255, 0, 0, 255, 255]).unwrap();
    out
}

/// Both sprites are 400 x 100, centred in x: the texel boundary is the view's middle column. The
/// default one sits 100 px above the middle, the linear one 100 px below.
const GAME: &str = r#"import { app } from "bevy-ts";
import { Camera2d, Image, Sprite, Transform } from "bevy-ts/bevy";

const linear = Image.load("linear.png", { sampler: { Descriptor: { mag_filter: "Linear", min_filter: "Linear" } } });

app.scene("s", () => (
  <>
    <entity><Camera2d /></entity>
    <entity><Sprite image="plain.png" custom_size={[400, 100]} /><Transform translation={[0, 100, 0]} /></entity>
    <entity><Sprite image={linear} custom_size={[400, 100]} /><Transform translation={[0, -100, 0]} /></entity>
  </>
));
"#;

/// What the view shows 40 px left of the middle (0.3 of a texel past the red one's centre, so
/// linear sampling mixes in blue): on the default sprite's row, then the linear one's.
fn left_of_middle(name: &str, image_sampler: DefaultSampler) -> ([u8; 3], [u8; 3]) {
    let _one = renderer();
    let dir = project(name, &[("game.tsx", GAME), ("package.json", "{}")]);
    for file in ["plain.png", "linear.png"] {
        std::fs::create_dir_all(dir.join("assets")).unwrap();
        std::fs::write(dir.join("assets").join(file), red_blue()).unwrap();
    }
    let engine = Engine { identity: bevy_ts_cli::EngineIdentity::prebuilt(env!("CARGO_PKG_VERSION"), "test") };
    let switches = Switches { image_sampler, ..Default::default() };
    let mut app = base_app(&engine, Prebuilt, Display::Headless, Default::default(), Some(&dir.join("assets")), true, switches, &[], false);
    app.insert_resource(DevSource(Arc::new(Compiler::new(dir.join("game.tsx"), true).unwrap())));
    app.finish();
    app.cleanup();
    let r = reload(&mut app, false);
    assert_eq!(r.status, ReloadStatus::Applied, "{:?}", r.errors);
    let target = app.world().resource::<Offscreen>().0.clone();
    let shot: Arc<Mutex<Option<Image>>> = Arc::default();
    let px = |img: &Image, x: u32, y: u32| {
        let i = ((y * SIZE.0 + x) * 4) as usize;
        let d = img.data.as_ref().unwrap();
        [d[i], d[i + 1], d[i + 2]]
    };
    let (mid, top, bottom) = (SIZE.0 / 2, SIZE.1 / 2 - 100, SIZE.1 / 2 + 100);
    let mut last = None;
    for frame in 0..600 {
        app.update();
        if frame % 20 == 10 {
            let s = shot.clone();
            app.world_mut().spawn(Screenshot::image(target.clone())).observe(move |e: On<ScreenshotCaptured>| {
                *s.lock().unwrap() = Some(e.image.clone());
            });
        }
        let Some(img) = shot.lock().unwrap().take() else { continue };
        // Both images are drawn once each sprite's far left is its red texel.
        let drawn = |y| px(&img, mid - 180, y) == [255, 0, 0];
        if drawn(top) && drawn(bottom) {
            return (px(&img, mid - 40, top), px(&img, mid - 40, bottom));
        }
        last = Some((px(&img, mid - 180, top), px(&img, mid - 180, bottom)));
    }
    panic!("{name}: the sprites never drew red at their left edge: {last:?}");
}

fn mixed(c: [u8; 3]) -> bool {
    c[0] > 100 && c[2] > 60
}

#[test]
fn nearest_samples_a_default_image_nearest_and_an_own_linear_sampler_stays_linear() {
    let (plain, linear) = left_of_middle("sampler-nearest", DefaultSampler::Nearest);
    assert_eq!(plain, [255, 0, 0], "the default image is sampled nearest");
    assert!(mixed(linear), "the image with its own linear sampler mixes red and blue: {linear:?}");
}

#[test]
fn without_the_key_a_default_image_samples_linear() {
    let (plain, _) = left_of_middle("sampler-linear", DefaultSampler::Linear);
    assert!(mixed(plain), "Bevy's default sampler is linear: {plain:?}");
}
