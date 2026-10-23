//! Slots around exposed Rust sets (native decision 7), against the fake backend.

use bevy::ecs::schedule::ScheduleLabel;
use bevy::prelude::*;
use bevy_ts::backend::ModuleSet;
use bevy_ts::fake::{FakeHandle, FakeProgram};
use bevy_ts::generation::{self, ReloadStatus};
use bevy_ts::manifest::{Manifest, ScheduleDecl, Stage, SystemDecl};
use bevy_ts::sets::{self, BevyTsSlot};
use bevy_ts::BevyTsPlugin;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

#[derive(SystemSet, Debug, Clone, PartialEq, Eq, Hash)]
struct Step;

#[derive(Resource, Clone, Default)]
struct Log(Arc<Mutex<Vec<&'static str>>>);

fn app() -> (App, FakeHandle, Log) {
    let handle = FakeHandle::default();
    let h = handle.clone();
    let log = Log::default();
    let mut app = App::new();
    app.add_plugins(MinimalPlugins).add_plugins(BevyTsPlugin::new(move || Box::new(h.backend())));
    sets::expose(&mut app, "game:step", Update, Step);
    let l = log.clone();
    app.add_systems(Update, (move || l.0.lock().unwrap().push("rust step")).in_set(Step));
    let l = log.clone();
    app.add_systems(Update, (move || l.0.lock().unwrap().push("rust after slot")).after(BevyTsSlot::after("game:step")));
    app.insert_resource(log.clone());
    (app, handle, log)
}

fn sys(name: &str, stage: Stage, after: &[&str]) -> SystemDecl {
    SystemDecl {
        name: name.into(),
        schedule: ScheduleDecl::Stage { stage },
        params: vec![],
        after: after.iter().map(|s| s.to_string()).collect(),
        before: vec![],
        sets: vec![],
        condition: None,
        groups: vec![],
        location: None,
    }
}

fn push(handle: &FakeHandle, decls: Vec<SystemDecl>, log: &Log) {
    let systems = decls
        .iter()
        .map(|_| {
            let l = log.clone();
            Box::new(move |_: &mut bevy_ts::fake::FakeCall| {
                l.0.lock().unwrap().push("ts");
                Ok(())
            }) as bevy_ts::fake::FakeFn
        })
        .collect();
    handle.push(FakeProgram { manifest: Manifest { systems: decls, ..Default::default() }, systems, ..Default::default() });
}

fn apply(app: &mut App) -> generation::ReloadReport {
    generation::apply(app.world_mut(), ModuleSet { entry: PathBuf::from("/game.ts"), modules: vec![] }, false)
}

fn update_systems(app: &mut App) -> usize {
    app.world_mut().resource_scope(|_, s: Mut<Schedules>| s.get(Update.intern()).map_or(0, |s| s.systems_len()))
}

fn frame(app: &mut App, log: &Log) -> Vec<&'static str> {
    log.0.lock().unwrap().clear();
    app.update();
    log.0.lock().unwrap().clone()
}

#[test]
fn a_slot_runs_after_its_set_and_exists_only_once_named() {
    let (mut app, fake, log) = app();
    app.update();
    let base = update_systems(&mut app);

    // A generation naming no set adds no slot.
    push(&fake, vec![sys("game.ts#plain", Stage::Update, &[])], &log);
    assert_eq!(apply(&mut app).status, ReloadStatus::Applied);
    app.update();
    assert_eq!(update_systems(&mut app), base);

    // Naming the set makes one slot, after the Rust set and before what orders after the slot.
    push(&fake, vec![sys("game.ts#thrust", Stage::Update, &["game:step"])], &log);
    assert_eq!(apply(&mut app).status, ReloadStatus::Applied);
    assert_eq!(frame(&mut app, &log), vec!["rust step", "ts", "rust after slot"]);
    assert_eq!(update_systems(&mut app), base + 1);

    // A reload moves the system back to its stage; the slot stays, empty, and no schedule edit.
    push(&fake, vec![sys("game.ts#thrust", Stage::Update, &[])], &log);
    assert_eq!(apply(&mut app).status, ReloadStatus::Applied);
    let order = frame(&mut app, &log);
    assert_eq!(order.iter().filter(|s| **s == "ts").count(), 1);
    assert_eq!(update_systems(&mut app), base + 1);
}

#[test]
fn placement_errors_name_the_system_the_set_and_both_schedules() {
    let (mut app, fake, log) = app();
    app.update();
    push(&fake, vec![sys("game.ts#thrust", Stage::FixedUpdate, &["game:step"])], &log);
    let r = apply(&mut app);
    assert_eq!(r.status, ReloadStatus::Failed);
    let m = &r.errors[0].message;
    assert!(m.contains("game.ts#thrust") && m.contains("game:step") && m.contains("FixedUpdate") && m.contains("Update"), "{m}");

    // TS-to-TS ordering stays within a slot.
    push(&fake, vec![sys("game.ts#a", Stage::Update, &["game:step"]), sys("game.ts#b", Stage::Update, &["game.ts#a"])], &log);
    let r = apply(&mut app);
    assert_eq!(r.status, ReloadStatus::Failed);
    assert!(r.errors[0].message.contains("game.ts#b"), "{:?}", r.errors);
}
