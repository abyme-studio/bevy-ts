//! Slots around exposed Rust sets (native decision 7), against the fake backend.

use bevy::ecs::schedule::ScheduleLabel;
use bevy::prelude::*;
use bevy_ts::backend::ModuleSet;
use bevy_ts::fake::{FakeHandle, FakeProgram};
use bevy_ts::generation::{self, ReloadStatus};
use bevy_ts::manifest::{ConditionDecl, GroupDecl, Manifest, ScheduleDecl, Stage, SystemDecl};
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
    push_groups(handle, decls, vec![], log, &[]);
}

/// As `push`, with groups; system `i` logs `names[i]`, or "ts".
fn push_groups(handle: &FakeHandle, decls: Vec<SystemDecl>, groups: Vec<GroupDecl>, log: &Log, names: &[&'static str]) {
    let systems = (0..decls.len())
        .map(|i| {
            let l = log.clone();
            let name = names.get(i).copied().unwrap_or("ts");
            Box::new(move |_: &mut bevy_ts::fake::FakeCall| {
                l.0.lock().unwrap().push(name);
                Ok(())
            }) as bevy_ts::fake::FakeFn
        })
        .collect();
    handle.push(FakeProgram { manifest: Manifest { systems: decls, groups, ..Default::default() }, systems, ..Default::default() });
}

fn edits(app: &App) -> u64 {
    app.world().resource::<bevy_ts::bevy_system::LiveSystems>().edits
}

fn apply(app: &mut App) -> generation::ReloadReport {
    generation::apply(app.world_mut(), ModuleSet { entry: PathBuf::from("/game.ts"), modules: vec![] }, false)
}

fn slots(app: &App) -> usize {
    app.world().get_resource::<bevy_ts::sets::MadeSlots>().map_or(0, |m| m.0.len())
}

/// How many Bevy systems `Update` holds for TS system `identity` (names need Bevy's `debug`).
fn in_update(app: &App, identity: &str) -> usize {
    let schedules = app.world().resource::<Schedules>();
    let graph = schedules.get(Update.intern()).unwrap().graph();
    graph.systems_in_set(bevy_ts::bevy_system::TsSystemSet(identity.into()).intern()).map_or(0, |s| s.len())
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

    // A generation naming no set adds no slot, only its one Bevy system.
    push(&fake, vec![sys("game.ts#plain", Stage::Update, &[])], &log);
    assert_eq!(apply(&mut app).status, ReloadStatus::Applied);
    app.update();
    assert_eq!(in_update(&app, "game.ts#plain"), 1);
    assert_eq!(slots(&app), 0);

    // Naming the set makes one slot, after the Rust set and before what orders after the slot.
    push(&fake, vec![sys("game.ts#thrust", Stage::Update, &["game:step"])], &log);
    assert_eq!(apply(&mut app).status, ReloadStatus::Applied);
    assert_eq!(frame(&mut app, &log), vec!["rust step", "ts", "rust after slot"]);
    assert_eq!((in_update(&app, "game.ts#plain"), in_update(&app, "game.ts#thrust")), (0, 1));
    assert_eq!(slots(&app), 1);

    // A reload moves the system back to its stage; the slot set stays, empty, and is not made again.
    // Only `Update`'s schedule is edited.
    let before = edits(&app);
    push(&fake, vec![sys("game.ts#thrust", Stage::Update, &[])], &log);
    assert_eq!(apply(&mut app).status, ReloadStatus::Applied);
    assert_eq!(edits(&app), before + 1);
    let order = frame(&mut app, &log);
    assert_eq!(order.iter().filter(|s| **s == "ts").count(), 1);
    assert_eq!(in_update(&app, "game.ts#thrust"), 1);
    assert_eq!(slots(&app), 1);
}

#[test]
fn a_group_split_across_a_stage_and_a_slot_is_evaluated_once_per_schedule_run() {
    let (mut app, fake, log) = app();
    app.update();
    let mut a = sys("game.ts#a", Stage::Update, &[]);
    let mut b = sys("game.ts#b", Stage::Update, &["game:step"]);
    a.groups = vec!["game.ts#a/0".into()];
    b.groups = vec!["game.ts#a/0".into()];
    let group = GroupDecl { id: "game.ts#a/0".into(), condition: ConditionDecl::RunOnce };
    push_groups(&fake, vec![a, b], vec![group], &log, &["a", "b"]);
    assert_eq!(apply(&mut app).status, ReloadStatus::Applied);
    let first = frame(&mut app, &log);
    assert!(first.contains(&"a") && first.contains(&"b"), "{first:?}");
    for _ in 0..2 {
        let later = frame(&mut app, &log);
        assert!(!later.contains(&"a") && !later.contains(&"b"), "{later:?}");
    }
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
