//! `HeadlessSize` (change `headless-size`): the offscreen image is made at the size asked for, and
//! the UI and `bevy-ts/screenshot` follow the image. macOS (Metal).
#![cfg(target_os = "macos")]

use crate::common;

use bevy::asset::RenderAssetUsages;
use bevy::image::{CompressedImageFormats, ImageSampler, ImageType};
use bevy::prelude::*;
use bevy::remote::{BrpMessage, BrpResult, BrpSender, RemotePlugin};
use bevy::ui::{ComputedUiRenderTargetInfo, UiGlobalTransform};
use bevy_ts::generation::{DevSource, ReloadStatus};
use bevy_ts_cli::compile::Compiler;
use bevy_ts_cli::headless::{self, HeadlessSize, Offscreen};
use common::*;
use serde_json::{Value, json};
use std::sync::Arc;

const W: u32 = 640;
const H: u32 = 480;

/// A headless engine as `bevy-ts dev --headless` builds it, sized 640x480 as `BEVY_TS_SIZE=640x480`
/// would size it, with the core methods and a camera.
fn engine(name: &str) -> App {
    let dir = project(name, &[("package.json", "{}"), ("game.ts", "import { app } from \"bevy-ts\";\n")]);
    let mut app = App::new();
    app.add_plugins(headless::default_plugins(true))
        .insert_resource(HeadlessSize(UVec2::new(W, H)))
        .add_plugins(headless::HeadlessPlugin)
        .add_plugins(bevy_ts::BevyTsPlugin::new(bevy_ts_v8::factory(Default::default())).dev(true))
        .add_plugins(bevy_ts_tsx::TsxPlugin)
        .add_plugins(RemotePlugin::default())
        .add_systems(Startup, |mut commands: Commands| {
            commands.spawn(Camera3d::default());
        });
    bevy_ts_remote::add_methods(&mut app);
    app.insert_resource(DevSource(Arc::new(Compiler::new(dir.join("game.ts"), true).unwrap())));
    app.finish();
    app.cleanup();
    let r = reload(&mut app, false);
    assert_eq!(r.status, ReloadStatus::Applied, "{:?}", r.errors);
    // `RemotePlugin` makes its mailbox in `PreStartup`.
    app.update();
    app
}

/// Sends a request through Bevy's mailbox, as a transport does, and updates until it answers.
fn ask(app: &mut App, method: &str, params: Value) -> BrpResult {
    let (tx, rx) = async_channel::bounded(1);
    let message = BrpMessage { method: method.into(), params: Some(params), sender: tx };
    app.world().resource::<BrpSender>().try_send(message).unwrap();
    for _ in 0..600 {
        app.update();
        if let Ok(r) = rx.try_recv() {
            return r;
        }
    }
    panic!("{method} did not answer within 600 frames");
}

#[test]
fn the_image_and_a_full_ui_root_are_the_size_asked_for() {
    let mut app = engine("headless-size-ui");
    let image = app.world().resource::<Offscreen>().0.clone();
    let size = app.world().resource::<Assets<Image>>().get(&image).unwrap().size();
    assert_eq!(size, UVec2::new(W, H));

    let root = app.world_mut().spawn(Node { width: percent(100), height: percent(100), ..default() }).id();
    updates(&mut app, 10);
    let target = app.world().get::<ComputedUiRenderTargetInfo>(root).unwrap();
    assert_eq!(target.logical_size(), Vec2::new(W as f32, H as f32));
    let size = app.world().get::<ComputedNode>(root).unwrap().size;
    let center = app.world().get::<UiGlobalTransform>(root).unwrap().translation;
    assert_eq!((center - size / 2.0, size), (Vec2::ZERO, Vec2::new(W as f32, H as f32)), "covers (0,0)-({W},{H})");
}

#[test]
fn the_protocols_shot_is_the_size_asked_for() {
    let mut app = engine("headless-size-shot");
    let dir = std::env::temp_dir().join("bevy-ts-cli-tests").join(format!("headless-size-shots-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.canonicalize().unwrap().join("shot.png");
    let r = ask(&mut app, "bevy-ts/screenshot", json!({ "path": path })).unwrap();
    assert_eq!((r["width"].as_u64(), r["height"].as_u64()), (Some(W as u64), Some(H as u64)), "{r}");
    let bytes = std::fs::read(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
    let png = Image::from_buffer(&bytes, ImageType::Extension("png"), CompressedImageFormats::NONE, true, ImageSampler::Default, RenderAssetUsages::default()).unwrap();
    assert_eq!((png.width(), png.height()), (W, H));
}
