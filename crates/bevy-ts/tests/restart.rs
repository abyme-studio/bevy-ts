//! A `restart` reload and a Stop's rebuild put the TS world back as at first load: resources at
//! their `insertResource` values, no pending message, and a failing re-apply leaves the empty
//! world. On the fake backend: no V8 in this build.

use bevy::prelude::*;
use bevy_ts::BevyTsPlugin;
use bevy_ts::backend::{ModuleSet, ScriptError};
use bevy_ts::commands::TsSpawned;
use bevy_ts::components::TsComponents;
use bevy_ts::fake::{FakeHandle, FakeProgram};
use bevy_ts::generation::{self, LiveSet, ReloadLog, ReloadStatus};
use bevy_ts::layout::FieldType;
use bevy_ts::manifest::{FieldDecl, Manifest, PayloadDecl, ResourceDecl};
use bevy_ts::messages::TsPayloads;
use bevy_ts::plugin_api;
use bevy_ts::systems::TsSystems;
use serde_json::json;
use std::path::PathBuf;

fn app() -> (App, FakeHandle) {
    let handle = FakeHandle::default();
    let h = handle.clone();
    let mut app = App::new();
    app.add_plugins(MinimalPlugins).add_plugins(BevyTsPlugin::new(move || Box::new(h.backend())));
    (app, handle)
}

fn set() -> ModuleSet {
    ModuleSet { entry: PathBuf::from("/game.ts"), modules: vec![] }
}

/// `Score { value: f32 }` inserted with `value: 0`, a `Hit` message, and an entry that spawns.
fn program() -> FakeProgram {
    let manifest = Manifest {
        resources: vec![ResourceDecl {
            name: "game::Score".into(),
            fields: vec![FieldDecl { name: "value".into(), ty: FieldType::F32, default: None, hints: Default::default() }],
            insert: Some(json!({ "value": 0 }).as_object().unwrap().clone()),
            location: None,
        }],
        messages: vec![PayloadDecl { name: "game::Hit".into(), fields: vec![], entity: false, propagate: false, added: true, location: None }],
        entries: vec![bevy_ts::manifest::EntryDecl { name: "test/spawn".into(), location: None }],
        ..Default::default()
    };
    let mut p = FakeProgram { manifest, ..Default::default() };
    p.entries.insert(
        "test/spawn".into(),
        Box::new(|call| {
            call.commands.spawn(&[])?;
            Ok(())
        }),
    );
    p
}

fn score(world: &mut World) -> Option<f32> {
    let id = world.resource::<TsComponents>().live("game::Score")?.id;
    let e = world.resource_entities().get(id)?;
    let ptr = world.get_by_id(e, id)?;
    // SAFETY: `Score` is one f32 at offset 0.
    Some(unsafe { ptr.as_ptr().cast::<f32>().read_unaligned() })
}

fn set_score(world: &mut World, v: f32) {
    let id = world.resource::<TsComponents>().live("game::Score").unwrap().id;
    let e = world.resource_entities().get(id).unwrap();
    let ptr = world.entity_mut(e).into_mut_by_id(id).unwrap();
    // SAFETY: as above.
    unsafe { ptr.into_inner().as_ptr().cast::<f32>().write_unaligned(v) };
}

#[test]
fn a_restart_resets_the_score_and_a_reload_keeps_it() {
    let (mut app, fake) = app();
    fake.push(program());
    assert_eq!(generation::apply(app.world_mut(), set(), false).status, ReloadStatus::Applied);
    assert_eq!(score(app.world_mut()), Some(0.0));
    set_score(app.world_mut(), 7.0);
    fake.push(program());
    generation::apply(app.world_mut(), set(), false);
    assert_eq!(score(app.world_mut()), Some(7.0), "a reload keeps a resource's value");
    fake.push(program());
    assert_eq!(generation::apply(app.world_mut(), set(), true).status, ReloadStatus::Applied);
    assert_eq!(score(app.world_mut()), Some(0.0), "a restart inserts it again with its insertResource value");
    // A Rust resource stays.
    assert!(app.world().contains_resource::<Time>());
}

#[test]
fn a_restart_drops_a_pending_message() {
    let (mut app, fake) = app();
    fake.push(program());
    generation::apply(app.world_mut(), set(), false);
    app.world_mut().resource_mut::<TsPayloads>().messages[0].push(&[]);
    assert!(app.world().resource::<TsPayloads>().messages[0].has_unread(0));
    fake.push(program());
    generation::apply(app.world_mut(), set(), true);
    assert!(!app.world().resource::<TsPayloads>().messages[0].has_unread(0), "no reader sees the Hit");
}

#[test]
fn a_rebuild_reapplies_the_live_set_without_compiling() {
    let (mut app, fake) = app();
    assert!(generation::rebuild(app.world_mut()).is_err(), "no generation yet");
    fake.push(program());
    generation::apply(app.world_mut(), set(), false);
    assert!(app.world().contains_resource::<LiveSet>());
    plugin_api::call_entry(app.world_mut(), "test/spawn", b"").unwrap();
    set_score(app.world_mut(), 3.0);
    fake.push(program());
    let r = generation::rebuild(app.world_mut()).unwrap();
    assert_eq!(r.status, ReloadStatus::Applied);
    assert_eq!(app.world_mut().query::<&TsSpawned>().iter(app.world()).count(), 0);
    assert_eq!(score(app.world_mut()), Some(0.0));
    assert_eq!(fake.restarts(), 1, "a fresh isolate");
    assert_eq!(app.world().resource::<generation::Generations>().restarts, 1);
}

#[test]
fn a_failing_reapply_leaves_the_empty_generation() {
    let (mut app, fake) = app();
    fake.push(program());
    generation::apply(app.world_mut(), set(), false);
    plugin_api::call_entry(app.world_mut(), "test/spawn", b"").unwrap();
    fake.push(FakeProgram { load_error: Some(vec![ScriptError::new("boom")]), ..Default::default() });
    let r = generation::rebuild(app.world_mut()).unwrap();
    assert_eq!(r.status, ReloadStatus::Failed);
    assert_eq!(app.world().resource::<ReloadLog>().reports.last().unwrap().status, ReloadStatus::Failed, "on reload+watch");
    assert_eq!(app.world_mut().query::<&TsSpawned>().iter(app.world()).count(), 0, "the play world is gone");
    assert_eq!(app.world().resource::<TsSystems>().iter().count(), 0);
    assert_eq!(score(app.world_mut()), None, "no resource until a generation applies");
    let logged = app.world().resource::<bevy_ts::log::ScriptLog>().since(0).any(|e| e.message.contains("boom"));
    assert!(logged, "the error is on the log");
    // The live set is kept: the next Stop tries again.
    fake.push(program());
    assert_eq!(generation::rebuild(app.world_mut()).unwrap().status, ReloadStatus::Applied);
    assert_eq!(score(app.world_mut()), Some(0.0));
}
