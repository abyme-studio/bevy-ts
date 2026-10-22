//! Change `mesh-picking`: `base_app` adds `MeshPickingPlugin` only when asked, and a mesh with a
//! pointer handler while nothing picks meshes is named once.

use bevy::picking::mesh_picking::{MeshPickingPlugin, MeshPickingSettings};
use bevy::prelude::*;
use bevy_ts::log::{LogLevel, ScriptLog};
use bevy_ts_cli::cli::{Display, Engine, base_app};
use bevy_ts_tsx::picking::TsxHandlers;

fn engine(extra: impl Plugin, mesh_picking: bool) -> App {
    let engine = Engine { identity: bevy_ts_cli::EngineIdentity::prebuilt(env!("CARGO_PKG_VERSION"), "test") };
    let mut app = base_app(&engine, extra, Display::Headless, Default::default(), None, true, mesh_picking);
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

#[derive(Resource, Default)]
struct Clicks(Vec<Entity>);

#[test]
fn the_headless_pointer_clicks_a_mesh() {
    use bevy_ts_remote::protocol::{PointerEvent, PointerEventButton, PointerEventKind, PointerEventType};
    use bevy_ts_viewport::offscreen::{OffscreenPointerPlugin, Pending};
    let mut app = {
        let engine = Engine { identity: bevy_ts_cli::EngineIdentity::prebuilt(env!("CARGO_PKG_VERSION"), "test") };
        let mut app = base_app(&engine, bevy_ts_cli::Prebuilt, Display::Headless, Default::default(), None, true, true);
        let image = app.world().resource::<bevy_ts_cli::headless::Offscreen>().0.clone();
        app.add_plugins(OffscreenPointerPlugin { image });
        app.finish();
        app.cleanup();
        app
    };
    app.init_resource::<Clicks>();
    app.world_mut().spawn((Camera3d::default(), Transform::from_xyz(0.0, 0.0, 4.0).looking_at(Vec3::ZERO, Vec3::Y)));
    let mesh = app.world_mut().resource_mut::<Assets<Mesh>>().add(Cuboid::from_size(Vec3::ONE));
    let cube = app
        .world_mut()
        .spawn((Mesh3d(mesh), Transform::default()))
        .observe(|on: On<bevy::picking::events::PointerClick>, mut clicks: ResMut<Clicks>| clicks.0.push(on.event().entity))
        .id();
    for _ in 0..10 {
        app.update();
    }
    let ev = |kind| PointerEvent {
        button: Some(PointerEventButton::Left),
        buttons: vec![],
        clicks: None,
        dx: None,
        dy: None,
        kind,
        mods: None,
        type_: PointerEventType::Pointer,
        x: 480.0,
        y: 270.0,
    };
    app.world_mut().resource_mut::<Pending>().0.extend([ev(PointerEventKind::Move), ev(PointerEventKind::Down), ev(PointerEventKind::Up)]);
    // One event a frame: the move, then the press, then the release.
    for _ in 0..6 {
        app.update();
    }
    assert_eq!(app.world().resource::<Clicks>().0, vec![cube]);
}
