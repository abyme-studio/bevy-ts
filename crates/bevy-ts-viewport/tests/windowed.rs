//! Task 5.6: edit mode in `bevy-ts dev`'s own window. The window here is a primary `Window` with
//! winit off (nothing opens on screen); what differs from viewport mode is the plugin's wiring:
//! no virtual window, no ring, no viewer methods.
#![cfg(target_os = "macos")]

mod common;

use bevy::prelude::*;
use bevy::remote::{RemoteMethods, RemotePlugin};
use bevy::window::ExitCondition;
use bevy::winit::WinitPlugin;
use bevy_ts_viewport::edit::EditorCamera;
use bevy_ts_viewport::{ViewportPlugin, VirtualWindow};
use common::{TheCube, call, scene};
use serde_json::{Value, json};

#[test]
fn edit_mode_works_against_the_engines_own_window() {
    let mut app = App::new();
    app.add_plugins(
        DefaultPlugins
            .build()
            .disable::<WinitPlugin>()
            .set(WindowPlugin { primary_window: Some(Window::default()), exit_condition: ExitCondition::DontExit, ..default() }),
    )
    .add_plugins(RemotePlugin::default())
    .add_plugins(ViewportPlugin { windowed: true })
    .add_systems(Startup, scene);
    app.finish();
    app.cleanup();
    for _ in 0..3 {
        app.update();
    }
    assert_eq!(app.world_mut().query::<&VirtualWindow>().iter(app.world()).count(), 0, "no virtual window");
    let methods = app.world().resource::<RemoteMethods>();
    for m in ["bevy-ts/viewport_attach", "bevy-ts/viewport_detach", "bevy-ts/viewport_resize", "bevy-ts/viewport_input"] {
        assert!(methods.get(m).is_none(), "{m} is viewport mode's only");
    }
    let r = call(&mut app, "bevy-ts/viewport_mode", json!({ "mode": "edit" })).unwrap();
    assert_eq!(r["mode"], "edit");
    app.update();
    app.update();
    assert_eq!(app.world_mut().query_filtered::<(), With<EditorCamera>>().iter(app.world()).count(), 1);
    assert_eq!(app.world_mut().query::<&bevy_ts_viewport::orient::OrientCamera>().iter(app.world()).count(), 0, "no orientation gizmo in the engine's own window");
    // The cube is at the origin, in the middle of the editor camera's first view.
    let window = app.world_mut().query::<&Window>().single(app.world()).unwrap().resolution.size() / 2.0;
    let pick = call(&mut app, "bevy-ts/viewport_pick", json!({ "x": window.x, "y": window.y })).unwrap();
    let cube = app.world_mut().query_filtered::<Entity, With<TheCube>>().single(app.world()).unwrap();
    assert_eq!(pick["hits"][0]["entity"], cube.to_bits(), "{pick}");
    let stats = call(&mut app, "bevy-ts/viewport_stats", Value::Null).unwrap();
    assert_eq!(stats["mode"], "edit");
    assert_eq!(stats["attached"], false);
}
