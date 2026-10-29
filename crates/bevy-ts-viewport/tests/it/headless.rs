//! Headless input (`OffscreenInputPlugin`): wheel, gesture and pointer events become the messages
//! a window gets, on the window-less primary window, as under `--viewport`. No renderer: the
//! plugin needs only the image's size, and picking is not under test here.

use bevy::input::InputPlugin;
use bevy::input::gestures::{DoubleTapGesture, PinchGesture, RotationGesture};
use bevy::input::mouse::{MouseScrollUnit, MouseWheel};
use bevy::input::touch::TouchPhase;
use bevy::picking::pointer::{PointerAction, PointerInput, PointerMap};
use bevy::asset::RenderAssetUsages;
use bevy::prelude::*;
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};
use bevy::remote::{RemoteMethodSystemId, RemoteMethods, RemotePlugin};
use bevy::window::{CursorLeft, ExitCondition, PrimaryWindow};
use bevy_ts_viewport::offscreen::{OffscreenInputPlugin, POINTER};
use serde_json::{Value, json};

/// What the game saw, every frame's messages kept.
#[derive(Resource, Default)]
struct Seen {
    wheels: Vec<MouseWheel>,
    pinches: Vec<f32>,
    rotations: Vec<f32>,
    double_taps: usize,
    scrolls: Vec<String>,
    left: usize,
}

fn record(
    mut seen: ResMut<Seen>,
    mut wheels: MessageReader<MouseWheel>,
    mut pinches: MessageReader<PinchGesture>,
    mut rotations: MessageReader<RotationGesture>,
    mut double_taps: MessageReader<DoubleTapGesture>,
    mut pointer: MessageReader<PointerInput>,
    mut left: MessageReader<CursorLeft>,
) {
    seen.wheels.extend(wheels.read().cloned());
    seen.pinches.extend(pinches.read().map(|p| p.0));
    seen.rotations.extend(rotations.read().map(|r| r.0));
    seen.double_taps += double_taps.read().count();
    seen.scrolls.extend(pointer.read().filter(|p| p.pointer_id == POINTER && matches!(p.action, PointerAction::Scroll { .. })).map(|p| format!("{:?}", p.action)));
    seen.left += left.read().count();
}

fn app() -> App {
    let mut app = App::new();
    app.add_plugins((
        MinimalPlugins,
        AssetPlugin::default(),
        WindowPlugin { primary_window: None, exit_condition: ExitCondition::DontExit, ..default() },
        InputPlugin,
        RemotePlugin::default(),
    ))
    .init_asset::<Image>()
    .add_message::<PointerInput>()
    // The headless pointer's entity registers itself here, as `PickingPlugin` sets up.
    .init_resource::<PointerMap>()
    .init_resource::<Seen>()
    .add_systems(Update, record);
    // 100×100: a window reads a cursor outside it as none.
    let size = Extent3d { width: 100, height: 100, depth_or_array_layers: 1 };
    let pixels = Image::new_fill(size, TextureDimension::D2, &[0, 0, 0, 255], TextureFormat::Rgba8UnormSrgb, RenderAssetUsages::default());
    let image = app.world_mut().resource_mut::<Assets<Image>>().add(pixels);
    app.add_plugins(OffscreenInputPlugin { image });
    app.finish();
    app.cleanup();
    app.update();
    app
}

/// Posts `events` to `bevy-ts/viewport_input` and runs a frame per event, and one more.
fn post(app: &mut App, events: Value) {
    let n = events.as_array().unwrap().len();
    let Some(RemoteMethodSystemId::Instant(id)) = app.world().resource::<RemoteMethods>().get("bevy-ts/viewport_input").cloned() else {
        panic!("no bevy-ts/viewport_input");
    };
    let r = app.world_mut().run_system_with(id, Some(json!({ "events": events }))).unwrap().unwrap();
    assert_eq!(r["queued"], n, "{r}");
    for _ in 0..=2 * n {
        app.update();
    }
}

fn cursor(app: &mut App) -> Option<Vec2> {
    app.world_mut().query_filtered::<&Window, With<PrimaryWindow>>().single(app.world()).unwrap().cursor_position()
}

fn window(app: &mut App) -> Entity {
    app.world_mut().query_filtered::<Entity, With<PrimaryWindow>>().single(app.world()).unwrap()
}

#[test]
fn a_wheel_headless_is_mouse_wheel_where_it_points() {
    let mut app = app();
    post(&mut app, json!([
        { "type": "pointer", "kind": "move", "x": 10.0, "y": 20.0 },
        { "type": "wheel", "dx": 1.5, "dy": -3.0, "unit": "pixel", "phase": "started", "x": 30.0, "y": 40.0 },
        { "type": "wheel", "dx": 0.0, "dy": 2.0, "unit": "line" },
    ]));
    let window = window(&mut app);
    let seen = app.world().resource::<Seen>();
    assert_eq!(seen.wheels, [
        MouseWheel { unit: MouseScrollUnit::Pixel, x: 1.5, y: -3.0, window, phase: TouchPhase::Started },
        MouseWheel { unit: MouseScrollUnit::Line, x: 0.0, y: 2.0, window, phase: TouchPhase::Moved },
    ]);
    // The headless pointer scrolls too, as the mouse's does in a window.
    let scrolls = [
        PointerAction::Scroll { unit: MouseScrollUnit::Pixel, x: 1.5, y: -3.0, phase: TouchPhase::Started },
        PointerAction::Scroll { unit: MouseScrollUnit::Line, x: 0.0, y: 2.0, phase: TouchPhase::Moved },
    ];
    assert_eq!(seen.scrolls, scrolls.map(|s| format!("{s:?}")));
    // The placed wheel moved the cursor; the other left it.
    assert_eq!(cursor(&mut app), Some(Vec2::new(30.0, 40.0)));
}

#[test]
fn gestures_headless_are_bevy_gestures() {
    let mut app = app();
    post(&mut app, json!([
        { "type": "gesture", "kind": "pinch", "delta": 0.25 },
        { "type": "gesture", "kind": "rotate", "delta": -0.5 },
        { "type": "gesture", "kind": "double_tap" },
    ]));
    let seen = app.world().resource::<Seen>();
    assert_eq!(seen.pinches, [0.25]);
    assert_eq!(seen.rotations, [-0.5]);
    assert_eq!(seen.double_taps, 1);
}

#[test]
fn a_move_headless_sets_the_window_cursor_and_leave_clears_it() {
    let mut app = app();
    assert_eq!(cursor(&mut app), None);
    post(&mut app, json!([{ "type": "pointer", "kind": "move", "x": 12.0, "y": 34.0 }]));
    assert_eq!(cursor(&mut app), Some(Vec2::new(12.0, 34.0)));
    post(&mut app, json!([{ "type": "pointer", "kind": "leave", "x": 12.0, "y": 34.0 }]));
    assert_eq!(cursor(&mut app), None);
    assert_eq!(app.world().resource::<Seen>().left, 1);
}

#[test]
fn headless_refuses_touch() {
    let mut app = app();
    let Some(RemoteMethodSystemId::Instant(id)) = app.world().resource::<RemoteMethods>().get("bevy-ts/viewport_input").cloned() else {
        panic!("no bevy-ts/viewport_input");
    };
    let events = json!({ "events": [{ "type": "wheel", "dx": 0.0, "dy": 1.0, "unit": "line" }, { "type": "touch", "kind": "end", "touches": [] }] });
    let e = app.world_mut().run_system_with(id, Some(events)).unwrap().unwrap_err();
    assert!(e.message.contains("touch"), "{}", e.message);
}
