//! Headless UI: with no window, game UI draws into the offscreen image `BEVY_TS_SCREENSHOT` saves,
//! laid out at the image's size. The arena's Hud, on the GPU. macOS (Metal).
#![cfg(target_os = "macos")]

use crate::common;

use bevy::prelude::*;
use bevy::render::view::screenshot::{Screenshot, ScreenshotCaptured};
use bevy::ui::{ComputedUiRenderTargetInfo, ComputedUiTargetCamera, IsDefaultUiCamera, UiGlobalTransform};
use bevy_ts::generation::{DevSource, ReloadStatus};
use bevy_ts_cli::compile::Compiler;
use bevy_ts_cli::headless::{self, Offscreen, SIZE};
use common::*;
use std::path::Path;
use std::sync::{Arc, Mutex};

const BUTTON: Color = Color::srgb(0.2, 0.4, 0.8);

/// The arena, built as `bevy-ts dev --headless` builds it.
fn arena(name: &str) -> App {
    let src = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/arena");
    let mut files: Vec<(String, String)> = Vec::new();
    for f in ["src/enemy.tsx", "src/player.tsx", "src/hud.tsx", "src/game.tsx", "assets/levels/arena.bsn", "assets/levels/room.bsn"] {
        files.push((f.to_string(), std::fs::read_to_string(src.join(f)).unwrap()));
    }
    files.push(("package.json".into(), "{}".into()));
    let list: Vec<(&str, &str)> = files.iter().map(|(a, b)| (a.as_str(), b.as_str())).collect();
    let dir = project(name, &list);
    let mut app = App::new();
    app.add_plugins(headless::default_plugins(true).set(headless::asset_plugin_unwatched(&dir.join("assets"))))
        .add_plugins(headless::HeadlessPlugin)
        .add_plugins(bevy_ts::BevyTsPlugin::new(bevy_ts_v8::factory(Default::default())).dev(true))
        .add_plugins(bevy_ts_tsx::TsxPlugin)
        .add_plugins(bevy_ts_levels::engine::LevelsPlugin);
    app.insert_resource(DevSource(Arc::new(Compiler::new(dir.join("src/game.tsx"), true).unwrap())));
    app.finish();
    app.cleanup();
    let r = reload(&mut app, false);
    assert_eq!(r.status, ReloadStatus::Applied, "{:?}", r.errors);
    app
}

/// The UI node whose background is `color`.
fn node_with(app: &mut App, color: Color) -> Entity {
    let mut q = app.world_mut().query::<(Entity, &BackgroundColor)>();
    let found: Vec<Entity> = q.iter(app.world()).filter(|(_, b)| (b.0.to_srgba().to_vec4() - color.to_srgba().to_vec4()).length() < 0.01).map(|(e, _)| e).collect();
    assert_eq!(found.len(), 1, "one node with background {color:?}");
    found[0]
}

/// A node's top-left corner and size, in the image's pixels.
fn rect(app: &App, e: Entity) -> (Vec2, Vec2) {
    let size = app.world().get::<ComputedNode>(e).unwrap().size;
    let center = app.world().get::<UiGlobalTransform>(e).unwrap().translation;
    (center - size / 2.0, size)
}

fn ui_camera(app: &App, e: Entity) -> Option<Entity> {
    app.world().get::<ComputedUiTargetCamera>(e).unwrap().get()
}

fn screenshot(app: &mut App) -> Image {
    let shot = Arc::new(Mutex::new(None));
    let slot = shot.clone();
    let image = app.world().resource::<Offscreen>().0.clone();
    app.world_mut().spawn(Screenshot::image(image)).observe(move |c: On<ScreenshotCaptured>| {
        *slot.lock().unwrap() = Some(c.image.clone());
    });
    for _ in 0..60 {
        app.update();
        if let Some(i) = shot.lock().unwrap().take() {
            return i;
        }
    }
    panic!("no screenshot within 60 frames");
}

#[test]
fn the_arena_hud_draws_into_the_headless_image() {
    let mut app = arena("headless-ui-hud");
    let full = app.world_mut().spawn(Node { width: percent(100), height: percent(100), ..default() }).id();
    updates(&mut app, 10);

    let camera = app.world_mut().query_filtered::<Entity, With<Camera3d>>().single(app.world()).unwrap();
    assert!(app.world().entity(camera).contains::<IsDefaultUiCamera>(), "the game camera is the default UI camera");
    let hud = node_with(&mut app, Color::srgba(0.0, 0.0, 0.0, 0x88 as f32 / 255.0));
    assert_eq!(ui_camera(&app, hud), Some(camera));
    let target = app.world().get::<ComputedUiRenderTargetInfo>(hud).unwrap();
    assert_eq!(target.logical_size(), Vec2::new(SIZE.0 as f32, SIZE.1 as f32));

    // A full-size root fills the image; the Hud sits 12 px in from its top-left.
    assert_eq!(rect(&app, full), (Vec2::ZERO, Vec2::new(SIZE.0 as f32, SIZE.1 as f32)));
    let (at, size) = rect(&app, hud);
    assert_eq!(at, Vec2::new(12.0, 12.0));
    assert!(size.x > 0.0 && size.y > 0.0, "{size}");

    // The Spawn button's blue is in the image, just inside its corner.
    app.world_mut().entity_mut(full).despawn();
    let button = node_with(&mut app, BUTTON);
    let (corner, _) = rect(&app, button);
    let image = screenshot(&mut app);
    assert_eq!((image.width(), image.height()), SIZE);
    let pixel = image.get_color_at(corner.x as u32 + 2, corner.y as u32 + 2).unwrap().to_srgba();
    assert!((pixel.to_vec4() - BUTTON.to_srgba().to_vec4()).length() < 0.05, "the button's corner is {pixel:?}, not its blue");
}

#[test]
fn a_games_own_default_ui_camera_wins() {
    let mut app = arena("headless-ui-own");
    let own = app.world_mut().spawn((Camera2d, Camera { order: -1, ..default() }, IsDefaultUiCamera)).id();
    updates(&mut app, 5);
    let hud = node_with(&mut app, Color::srgba(0.0, 0.0, 0.0, 0x88 as f32 / 255.0));
    assert_eq!(ui_camera(&app, hud), Some(own));
    let marked = app.world_mut().query_filtered::<Entity, With<IsDefaultUiCamera>>().iter(app.world()).collect::<Vec<_>>();
    assert_eq!(marked, vec![own], "headless takes its marker back");

    // Without the game's, headless gives the marker back to the highest-order camera.
    app.world_mut().entity_mut(own).remove::<IsDefaultUiCamera>();
    updates(&mut app, 2);
    let camera = app.world_mut().query_filtered::<Entity, With<Camera3d>>().single(app.world()).unwrap();
    assert_eq!(ui_camera(&app, hud), Some(camera));
}
