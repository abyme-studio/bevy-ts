//! A level swap in one frame: a `<For>` slot unmounts rows whose `Sprite` Bevy's sprite mesh path
//! (`add_mesh`, `PostUpdate`) saw added that same frame. Its `Mesh2d` insert is queued
//! on `Commands`, so `host::frame` must run after those commands apply, or the despawn lands first
//! and the insert panics on a despawned entity.

use bevy::ecs::schedule::SingleThreadedExecutor;
use bevy::prelude::*;
use bevy::sprite_render::SpriteMeshPlugin;
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
    app.add_plugins((MinimalPlugins, AssetPlugin::default()))
        .init_asset::<Mesh>()
        .init_asset::<Image>()
        .init_asset::<Shader>()
        .init_asset::<TextureAtlasLayout>()
        .init_asset::<bevy::sprite_render::SpriteMeshMaterial>()
        .add_plugins(BevyTsPlugin::new(move || Box::new(h.backend())))
        .add_plugins(TsxPlugin)
        .add_plugins(SpriteMeshPlugin);
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

fn sprites(app: &mut App) -> usize {
    let world = app.world_mut();
    world.query_filtered::<&TsxNode, With<Sprite>>().iter(world).count()
}

/// `PostUpdate`'s systems in run order, by name.
fn order(app: &App) -> Vec<String> {
    let schedules = app.world().resource::<Schedules>();
    let schedule = schedules.get(PostUpdate).expect("PostUpdate");
    schedule.systems().expect("initialized").map(|(_, s)| s.name().to_string()).collect()
}

#[test]
fn the_frame_runs_after_the_sprite_mesh_commands_apply() {
    let (mut app, replies) = app();
    let row = json!([{ "key": "row", "type": "entity", "components": [{ "type": "Sprite", "kind": "rust", "props": {} }] }]);
    call(&mut app, &replies, json!({ "mounts": [[1, "a", row]] }));
    assert_eq!(sprites(&mut app), 1);
    // The swap: the old rows go and the new come in one call.
    call(&mut app, &replies, json!({ "unmounts": [[1, "a"]], "mounts": [[1, "b", row]] }));
    assert_eq!(sprites(&mut app), 1);
    // The race is between systems the graph leaves unordered, so the order is the test: the
    // frame comes after `add_material` (the second of `add_mesh`'s chain) and a sync point.
    let names = order(&app);
    let at = |what: &str| names.iter().position(|n| n.contains(what)).unwrap_or_else(|| panic!("no {what} in {names:#?}"));
    let (material, frame) = (at("add_material"), at("host::frame"));
    assert!(material < frame, "{names:#?}");
    assert!(names[material..frame].iter().any(|n| n.contains("apply_deferred") || n.contains("ApplyDeferred")), "a sync point between: {names:#?}");
}
