//! A level swap in one frame: a `<For>` slot unmounts rows whose `Sprite` Bevy's sprite mesh path
//! (`add_mesh`, `PostUpdate`) would see added. Its `Mesh2d` insert is queued on `Commands`, so
//! `host::frame` runs before it, or the despawn lands between the queue and the insert and the
//! insert panics on a despawned entity. The app has the engine's plugins, world text among them,
//! so an ordering that closes a cycle fails here.

use bevy::ecs::schedule::SingleThreadedExecutor;
use bevy::prelude::*;
use bevy::render::RenderPlugin;
use bevy::render::settings::{RenderCreation, WgpuSettings};
use bevy::window::ExitCondition;
use bevy::winit::WinitPlugin;
use bevy_ts::BevyTsPlugin;
use bevy_ts::backend::ModuleSet;
use bevy_ts::fake::{FakeHandle, FakeProgram};
use bevy_ts::generation::{self, ReloadStatus};
use bevy_ts::manifest::{EntryDecl, Manifest, Section};
use bevy_ts_tsx::{TsxNode, TsxPlugin, TsxTree};
use serde_json::{Value, json};
use std::collections::VecDeque;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

type Replies = Arc<Mutex<VecDeque<Value>>>;

fn app() -> (App, Replies) {
    let fake = FakeHandle::default();
    let h = fake.clone();
    let mut app = App::new();
    let render = RenderPlugin { render_creation: RenderCreation::Automatic(Box::new(WgpuSettings { backends: None, ..default() })), ..default() };
    app.add_plugins(
        DefaultPlugins
            .build()
            .disable::<WinitPlugin>()
            .disable::<bevy::log::LogPlugin>()
            .set(WindowPlugin { primary_window: None, exit_condition: ExitCondition::DontExit, ..default() })
            .set(render),
    )
    .add_plugins(BevyTsPlugin::new(move || Box::new(h.backend())))
    .add_plugins(TsxPlugin)
    .add_plugins(bevy_ts_material::WorldTextPlugin);
    app.finish();
    app.cleanup();
    // One thread, in the order the graph gives: the race is the graph's, not the scheduler's luck.
    app.edit_schedule(PostUpdate, |s| {
        s.set_executor(SingleThreadedExecutor::new());
    });
    let replies: Replies = Arc::default();
    let r = replies.clone();
    let section = json!({ "roots": [{ "name": "level", "nodes": [{ "key": "level", "type": "entity", "children": [{ "key": "for#0", "type": "For", "slot": 1 }] }] }] });
    let mut p = FakeProgram {
        manifest: Manifest::default(),
        sections: vec![Section { name: "tsx".into(), bytes: serde_json::to_vec(&section).unwrap(), location: None }],
        ..Default::default()
    };
    p.manifest.entries.push(EntryDecl { name: "tsx".into(), location: None });
    p.entries.insert(
        "tsx".into(),
        Box::new(move |call| {
            let reply = r.lock().unwrap().pop_front().unwrap_or_else(|| json!({}));
            call.output = serde_json::to_vec(&reply).unwrap();
            Ok(())
        }),
    );
    fake.push(p);
    let report = generation::apply(app.world_mut(), ModuleSet { entry: PathBuf::from("/game.tsx"), modules: vec![] }, false);
    assert_eq!(report.status, ReloadStatus::Applied, "{:?}", report.errors);
    app.update();
    (app, replies)
}

fn call(app: &mut App, replies: &Replies, reply: Value) {
    replies.lock().unwrap().push_back(reply);
    app.world_mut().resource_mut::<TsxTree>().reactive.due = true;
    app.update();
}

/// Mounted sprites, and those with their quad.
fn sprites(app: &mut App) -> (usize, usize) {
    let world = app.world_mut();
    let all = world.query_filtered::<&TsxNode, With<Sprite>>().iter(world).count();
    let meshed = world.query_filtered::<&TsxNode, (With<Sprite>, With<Mesh2d>)>().iter(world).count();
    (all, meshed)
}

/// `PostUpdate`'s systems in run order, by name.
fn order(app: &App) -> Vec<String> {
    let schedules = app.world().resource::<Schedules>();
    let schedule = schedules.get(PostUpdate).expect("PostUpdate");
    schedule.systems().expect("initialized").map(|(_, s)| s.name().to_string()).collect()
}

#[test]
fn the_frame_runs_before_the_sprite_mesh_systems() {
    let (mut app, replies) = app();
    let row = json!([{ "key": "row", "type": "entity", "components": [{ "type": "Sprite", "kind": "rust", "props": {} }] }]);
    call(&mut app, &replies, json!({ "mounts": [[1, "a", row]] }));
    assert_eq!(sprites(&mut app), (1, 1), "the quad the frame it mounts");
    // The swap: the old rows go and the new come in one call.
    call(&mut app, &replies, json!({ "unmounts": [[1, "a"]], "mounts": [[1, "b", row]] }));
    assert_eq!(sprites(&mut app), (1, 1), "the new level's quad the frame it swaps in");
    // The race is between systems the graph would leave unordered, so the order is the test.
    let names = order(&app);
    let at = |what: &str| names.iter().position(|n| n.contains(what)).unwrap_or_else(|| panic!("no {what} in {names:#?}"));
    assert!(at("host::frame") < at("add_mesh"), "{names:#?}");
}
