//! Change `mesh-picking`: `base_app` adds `MeshPickingPlugin` only when asked, and a mesh with a
//! pointer handler while nothing picks meshes is named once.

use bevy::picking::mesh_picking::{MeshPickingPlugin, MeshPickingSettings};
use bevy::prelude::*;
use bevy_ts::log::{LogLevel, ScriptLog};
use bevy_ts_cli::cli::{Display, Engine, base_app};
use bevy_ts_tsx::events::TsxHandlers;

fn engine(extra: impl Plugin, mesh_picking: bool) -> App {
    let engine = Engine { identity: bevy_ts_cli::EngineIdentity::prebuilt(env!("CARGO_PKG_VERSION"), "test") };
    let mut app = base_app(&engine, extra, Display::Headless, Default::default(), None, true, mesh_picking, &[], false);
    app.finish();
    app.cleanup();
    app
}

/// Two cubes with a click handler, a frame apart; the warnings logged.
fn warnings(mut app: App) -> Vec<String> {
    let mesh = app.world_mut().resource_mut::<Assets<Mesh>>().add(Cuboid::from_size(Vec3::ONE));
    app.world_mut().spawn((Name::new("first"), Mesh3d(mesh.clone()), TsxHandlers::default()));
    app.update();
    app.world_mut().spawn((Name::new("second"), Mesh3d(mesh), TsxHandlers::default()));
    app.update();
    app.update();
    app.world()
        .resource::<ScriptLog>()
        .since(0)
        .filter(|e| e.level == LogLevel::Warn && e.message.contains("meshPicking"))
        .map(|e| e.message.clone())
        .collect()
}

#[test]
fn off_by_default_on_when_asked() {
    assert!(!engine(bevy_ts_cli::Prebuilt, false).world().contains_resource::<MeshPickingSettings>());
    assert!(engine(bevy_ts_cli::Prebuilt, true).world().contains_resource::<MeshPickingSettings>());
}

#[test]
fn a_mesh_handler_with_picking_off_is_named_once() {
    let w = warnings(engine(bevy_ts_cli::Prebuilt, false));
    assert_eq!(w.len(), 1, "{w:#?}");
    assert!(w[0].contains("\"first\"") && w[0].contains("\"meshPicking\": true") && w[0].contains("bevy-ts.json"), "{}", w[0]);
}

#[test]
fn no_warning_with_the_switch() {
    assert_eq!(warnings(engine(bevy_ts_cli::Prebuilt, true)), Vec::<String>::new());
}

/// A native crate's plugin that adds mesh picking itself.
struct Native;

impl Plugin for Native {
    fn build(&self, app: &mut App) {
        app.add_plugins(MeshPickingPlugin);
    }
}

#[test]
fn no_warning_when_a_native_plugin_added_it() {
    assert_eq!(warnings(engine(Native, false)), Vec::<String>::new());
    // With the switch as well, it is added once.
    assert!(engine(Native, true).world().contains_resource::<MeshPickingSettings>());
}

/// Pointer events on the cube, in order.
#[derive(Resource, Default)]
struct Seen(Vec<&'static str>);

/// The headless engine with mesh picking and the offscreen pointer, a camera at z 4 and a unit
/// cube in the middle of the image that records its pointer events.
fn pointer_scene() -> App {
    use bevy::picking::events::{PointerClick, PointerOut, PointerOver};
    use bevy_ts_viewport::offscreen::OffscreenPointerPlugin;
    let engine = Engine { identity: bevy_ts_cli::EngineIdentity::prebuilt(env!("CARGO_PKG_VERSION"), "test") };
    let mut app = base_app(&engine, bevy_ts_cli::Prebuilt, Display::Headless, Default::default(), None, true, true, &[], false);
    let image = app.world().resource::<bevy_ts_cli::headless::Offscreen>().0.clone();
    app.add_plugins(OffscreenPointerPlugin { image });
    app.finish();
    app.cleanup();
    app.init_resource::<Seen>();
    app.world_mut().spawn((Camera3d::default(), Transform::from_xyz(0.0, 0.0, 4.0).looking_at(Vec3::ZERO, Vec3::Y)));
    let mesh = app.world_mut().resource_mut::<Assets<Mesh>>().add(Cuboid::from_size(Vec3::ONE));
    app.world_mut()
        .spawn((Mesh3d(mesh), Transform::default()))
        .observe(|_: On<PointerClick>, mut seen: ResMut<Seen>| seen.0.push("click"))
        .observe(|_: On<PointerOver>, mut seen: ResMut<Seen>| seen.0.push("over"))
        .observe(|_: On<PointerOut>, mut seen: ResMut<Seen>| seen.0.push("out"));
    for _ in 0..10 {
        app.update();
    }
    app
}

/// Queues pointer events at the cube's pixel and runs `frames` updates; what the cube saw.
fn send(app: &mut App, kinds: &[bevy_ts_remote::protocol::PointerEventKind], frames: usize) -> Vec<&'static str> {
    use bevy_ts_remote::protocol::{PointerEvent, PointerEventButton, PointerEventType};
    let events = kinds.iter().map(|kind| PointerEvent {
        button: Some(PointerEventButton::Left),
        buttons: vec![],
        clicks: None,
        dx: None,
        dy: None,
        kind: *kind,
        mods: None,
        type_: PointerEventType::Pointer,
        x: 480.0,
        y: 270.0,
    });
    app.world_mut().resource_mut::<bevy_ts_viewport::offscreen::Pending>().0.extend(events);
    for _ in 0..frames {
        app.update();
    }
    std::mem::take(&mut app.world_mut().resource_mut::<Seen>().0)
}

#[test]
fn the_headless_pointer_clicks_a_mesh() {
    use bevy_ts_remote::protocol::PointerEventKind::*;
    let mut app = pointer_scene();
    // One event a frame: the move, then the press, then the release.
    assert_eq!(send(&mut app, &[Move, Down, Up], 6), ["over", "click"]);
}

#[test]
fn a_press_without_a_move_moves_the_pointer_first() {
    use bevy_ts_remote::protocol::PointerEventKind::*;
    let mut app = pointer_scene();
    assert_eq!(send(&mut app, &[Down, Up], 6), ["over", "click"]);
}

#[test]
fn leave_takes_the_pointer_off_the_image() {
    use bevy_ts_remote::protocol::PointerEventKind::*;
    let mut app = pointer_scene();
    // Without clearing its location, Bevy casts again a frame after the cancel: over, out, over.
    assert_eq!(send(&mut app, &[Move, Leave], 8), ["over", "out"]);
}
