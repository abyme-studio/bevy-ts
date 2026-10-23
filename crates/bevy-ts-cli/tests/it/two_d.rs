//! 2D in TSX (change script-2d): sprites, `Camera2d` and sprite picking, through Bevy's own
//! plugins in a headless app that renders to an offscreen image. macOS (Metal).
#![cfg(target_os = "macos")]

use crate::common;

use bevy::camera::{ImageRenderTarget, NormalizedRenderTarget};
use bevy::picking::pointer::{Location, PointerAction, PointerButton, PointerId, PointerInput};
use bevy::prelude::*;
use bevy::render::view::screenshot::{Screenshot, ScreenshotCaptured};
use bevy::sprite::{SpritePickingMode, SpritePickingSettings};
use bevy::window::ExitCondition;
use bevy::winit::WinitPlugin;
use bevy_ts::BevyTsPlugin;
use bevy_ts::generation::{DevSource, ReloadStatus};
use bevy_ts_cli::compile::Compiler;
use bevy_ts_cli::headless::{Offscreen, SIZE};
use bevy_ts_tsx::TsxNode;
use common::*;
use std::sync::{Arc, Mutex};

fn scene(body: &str) -> String {
    format!("import {{ app }} from \"bevy-ts\";\nimport {{ Camera2d, Sprite }} from \"bevy-ts/bevy\";\n\napp.scene(\"s\", () => (\n  <>\n{body}  </>\n));\n")
}

/// Bevy's default plugins with no window, every camera drawing into [`Offscreen`].
fn setup(name: &str, body: &str) -> App {
    let dir = project(name, &[("game.tsx", &scene(body))]);
    let mut app = App::new();
    app.add_plugins(
        DefaultPlugins
            .build()
            .disable::<WinitPlugin>()
            .set(WindowPlugin { primary_window: None, exit_condition: ExitCondition::DontExit, ..default() }),
    )
    .add_plugins(bevy_ts_cli::headless::HeadlessPlugin)
    .add_plugins(BevyTsPlugin::new(bevy_ts_v8::factory(Default::default())).dev(true))
    .add_plugins(bevy_ts_tsx::TsxPlugin);
    app.insert_resource(DevSource(Arc::new(Compiler::new(&dir.join("game.tsx"), true).unwrap())));
    app.finish();
    app.cleanup();
    let r = reload(&mut app, false);
    assert_eq!(r.status, ReloadStatus::Applied, "{:?}", r.errors);
    app.update();
    app
}

fn node(app: &mut App, key: &str) -> Entity {
    let world = app.world_mut();
    world.query::<(Entity, &TsxNode)>().iter(world).find(|(_, n)| n.key_path == [key]).map(|(e, _)| e).unwrap_or_else(|| panic!("no node {key}"))
}

#[test]
fn a_sprite_takes_an_image_path_and_flips() {
    let mut app = setup("2d-sprite-image", "    <entity key=\"hero\"><Sprite image=\"hero.png\" flip_x /></entity>\n");
    let e = node(&mut app, "hero");
    let sprite = app.world().get::<Sprite>(e).expect("a Sprite");
    assert!(sprite.flip_x && !sprite.flip_y);
    let path = sprite.image.path().expect("a handle loaded from a path");
    assert_eq!(path.path().to_str(), Some("hero.png"));
}

#[test]
fn a_camera2d_is_orthographic_and_draws_a_sprite() {
    // Sprite's default image is white, and the default clear colour is grey: a white centre
    // pixel is the sprite drawn through the Camera2d.
    let body = "    <entity key=\"cam\"><Camera2d /></entity>\n    <entity key=\"s\"><Sprite custom_size={[2000, 2000]} /></entity>\n";
    let mut app = setup("2d-camera", body);
    let cam = node(&mut app, "cam");
    assert!(app.world().get::<Camera>(cam).is_some(), "Camera2d requires Camera");
    assert!(matches!(app.world().get::<Projection>(cam), Some(Projection::Orthographic(_))), "an orthographic Projection");

    let target = app.world().resource::<Offscreen>().0.clone();
    let shot: Arc<Mutex<Option<Image>>> = Arc::default();
    let mut last = None;
    for frame in 0..600 {
        app.update();
        if frame % 20 == 10 {
            let s = shot.clone();
            app.world_mut().spawn(Screenshot::image(target.clone())).observe(move |e: On<ScreenshotCaptured>| {
                *s.lock().unwrap() = Some(e.image.clone());
            });
        }
        if let Some(img) = shot.lock().unwrap().take() {
            let i = ((SIZE.1 / 2 * SIZE.0 + SIZE.0 / 2) * 4) as usize;
            let d = img.data.as_ref().unwrap();
            last = Some([d[i], d[i + 1], d[i + 2]]);
            if last == Some([255, 255, 255]) {
                return;
            }
        }
    }
    panic!("the centre pixel never became the white sprite: {last:?}");
}

/// A mouse at `at` on the offscreen image, then `action`, then a frame.
fn pointer(app: &mut App, at: Vec2, action: PointerAction) {
    let target = app.world().resource::<Offscreen>().0.clone();
    let location = Location { target: NormalizedRenderTarget::Image(ImageRenderTarget::from(target)), position: at };
    app.world_mut().write_message(PointerInput::new(PointerId::Mouse, location, action));
    app.update();
}

#[test]
fn clicking_a_sprite_runs_its_handler_once_with_its_entity() {
    let body = "    <entity key=\"cam\"><Camera2d /></entity>\n    <entity key=\"s\" onPointerClick={({ entity }) => console.log(`clicked ${entity}`)}><Sprite custom_size={[100, 100]} /></entity>\n";
    let mut app = setup("2d-pick-sprite", body);
    // No image loads: pick the sprite's rectangle, not its texels (design 6).
    app.world_mut().resource_mut::<SpritePickingSettings>().picking_mode = SpritePickingMode::BoundingBox;
    // Bevy casts a pointer's ray only when some window is primary, even into an image target: a
    // game has its window; this headless app gets one with no surface.
    app.world_mut().spawn((Window::default(), bevy::window::PrimaryWindow));
    let e = node(&mut app, "s");
    updates(&mut app, 3);
    let centre = Vec2::new(SIZE.0 as f32 / 2.0, SIZE.1 as f32 / 2.0);
    pointer(&mut app, centre, PointerAction::Move { delta: Vec2::ZERO });
    pointer(&mut app, centre, PointerAction::Press(PointerButton::Primary));
    pointer(&mut app, centre, PointerAction::Release(PointerButton::Primary));
    updates(&mut app, 2);
    let clicks: Vec<String> = messages(&app).into_iter().filter(|m| m.starts_with("clicked")).collect();
    assert_eq!(clicks, vec![format!("clicked {}", e.to_bits())]);

    // A click off the sprite picks nothing.
    let corner = Vec2::new(10.0, 10.0);
    pointer(&mut app, corner, PointerAction::Move { delta: Vec2::ZERO });
    pointer(&mut app, corner, PointerAction::Press(PointerButton::Primary));
    pointer(&mut app, corner, PointerAction::Release(PointerButton::Primary));
    updates(&mut app, 2);
    assert_eq!(messages(&app).iter().filter(|m| m.starts_with("clicked")).count(), 1);
}
