//! ts-systems-as-bevy-systems: each TS system is a Bevy system with exactly its access, ordered
//! and synced as Bevy orders Rust systems, and keeping its state across reloads.

use crate::common;

use bevy::ecs::schedule::{InternedSystemSet, LogLevel, ScheduleBuildSettings, ScheduleLabel, SystemKey};
use bevy::ecs::system::SystemAccess;
use bevy::prelude::*;
use bevy_ts::bevy_system::{LiveSystems, TsSystemSet};
use bevy_ts::BevyTsSet;
use common::*;

const HEAD: &str = r#"import { app, chain, Changed, Component, inState, Message, Resource, runOnce, resourceChanged, Startup, States, systems, Transform, u32, Update } from "bevy-ts";
import type { Commands, Local, MessageReader, MessageWriter, Mut, NextState, Query, ResMut } from "bevy-ts";
export class Vel extends Component { static path = "game::Vel"; declare x: u32 }
export class Enemy extends Component { static path = "game::Enemy" }
export class Score extends Resource { static path = "game::Score"; declare value: u32 }
export class Count extends Resource { static path = "game::Count"; declare n: u32 }
export class Game extends States("Menu", "Playing") { static path = "game::Game" }
export class Ping extends Message { static path = "game::Ping"; declare n: u32 }
"#;

fn src(body: &str) -> String {
    format!("{HEAD}{body}")
}

fn said(app: &mut App) -> Vec<String> {
    bevy_ts::runtime::pump(app.world_mut());
    messages(app).into_iter().filter(|m| !m.contains(' ')).collect()
}

fn edits(app: &App) -> u64 {
    app.world().resource::<LiveSystems>().edits
}

/// The set of the TS system whose identity ends with `suffix` (system names need Bevy's `debug`).
fn ts(app: &App, suffix: &str) -> InternedSystemSet {
    let systems = app.world().resource::<bevy_ts::systems::TsSystems>();
    let identity = systems.iter().map(|s| s.decl.name.clone()).find(|n| n.ends_with(suffix)).unwrap_or_else(|| panic!("no TS system *{suffix}"));
    TsSystemSet(identity).intern()
}

/// The one system in `set` in `label`.
fn key_in(app: &App, label: impl ScheduleLabel, set: InternedSystemSet) -> SystemKey {
    let schedules = app.world().resource::<Schedules>();
    let keys = schedules.get(label).unwrap().graph().systems_in_set(set).unwrap();
    assert_eq!(keys.len(), 1, "one system in {set:?}");
    *keys.first().unwrap()
}

fn access_of(app: &App, label: impl ScheduleLabel + Clone, set: InternedSystemSet) -> SystemAccess {
    let key = key_in(app, label.clone(), set);
    let schedules = app.world().resource::<Schedules>();
    let (_, s) = schedules.get(label).unwrap().systems_with_access().unwrap().find(|(k, _)| *k == key).unwrap();
    s.access().clone()
}

#[derive(SystemSet, Debug, Clone, PartialEq, Eq, Hash)]
struct RustWriter;

#[derive(SystemSet, Debug, Clone, PartialEq, Eq, Hash)]
struct RustFuel;

#[derive(Component)]
struct Fuel;

fn rust_writer(_: Query<&mut Transform>) {}

fn rust_fuel(_: Query<&mut Fuel>) {}

#[test]
fn a_ts_system_is_a_bevy_system_with_exactly_its_access() {
    let (_, mut app) = load("bsys-access", &src("export function move(q: Query<[Mut<Transform>, Vel]>) {}\napp.addSystems(Update, move);\n"));
    app.update();
    let transform = app.world().components().component_id::<Transform>().unwrap();
    let (vel, _) = bevy_ts::plugin_api::component_by_name(app.world(), "game::Vel").unwrap();
    let SystemAccess::Shared(set) = access_of(&app, Update, ts(&app, "#move")) else { panic!("a TS system's access is shared") };
    let a = set.combined_access();
    assert!(a.has_write(transform), "writes Transform");
    assert!(a.has_read(vel) && !a.has_write(vel), "reads Vel only");
}

#[test]
fn a_rust_system_writing_what_ts_writes_is_ambiguous_and_one_writing_else_is_not() {
    let (_, mut app) = load("bsys-ambiguous", &src("export function move(q: Query<[Mut<Transform>]>) {}\napp.addSystems(Update, move);\n"));
    app.add_systems(Update, (rust_writer.in_set(RustWriter), rust_fuel.in_set(RustFuel)));
    app.edit_schedule(Update, |s| {
        s.set_build_settings(ScheduleBuildSettings { ambiguity_detection: LogLevel::Warn, ..default() });
    });
    app.update();
    let mv = ts(&app, "#move");
    let access = access_of(&app, Update, mv);
    assert!(!access.is_compatible(&access_of(&app, Update, RustWriter.intern())), "a writer of Transform never overlaps move");
    assert!(access.is_compatible(&access_of(&app, Update, RustFuel.intern())), "a writer of Fuel may run beside move");

    let (mv, writer, fuel) = (key_in(&app, Update, mv), key_in(&app, Update, RustWriter.intern()), key_in(&app, Update, RustFuel.intern()));
    let schedules = app.world().resource::<Schedules>();
    let pairs = &schedules.get(Update).unwrap().graph().conflicting_systems().0;
    let with_move = |other: SystemKey| pairs.iter().any(|(a, b, _)| (*a == mv && *b == other) || (*b == mv && *a == other));
    assert!(with_move(writer), "move and the Transform writer are reported ambiguous");
    assert!(!with_move(fuel), "move and the Fuel writer are not");
}

#[test]
fn a_changed_filter_alone_reads_its_component() {
    let (_, mut app) = load("bsys-changed", &src("export function seen(q: Query<[Vel], Changed<Transform>>) {}\napp.addSystems(Update, seen);\n"));
    app.add_systems(Update, rust_writer.in_set(RustWriter));
    app.update();
    let transform = app.world().components().component_id::<Transform>().unwrap();
    let SystemAccess::Shared(set) = access_of(&app, Update, ts(&app, "#seen")) else { panic!("a TS system's access is shared") };
    assert!(set.combined_access().has_read(transform), "Changed<Transform> reads Transform's ticks");
    assert!(!access_of(&app, Update, ts(&app, "#seen")).is_compatible(&access_of(&app, Update, RustWriter.intern())), "a writer of Transform never overlaps it");
}

#[test]
fn ordering_against_the_stage_set_holds_for_every_ts_system() {
    let body = "export function a() { console.log('a'); }\nexport function b() { console.log('b'); }\napp.addSystems(Update, a, b);\n";
    let (_, mut app) = load("bsys-set", &src(body));
    #[derive(Resource, Default)]
    struct Seen(Vec<&'static str>);
    app.init_resource::<Seen>();
    app.add_systems(Update, ((|mut s: ResMut<Seen>| s.0.push("before")).before(BevyTsSet::Update), (|mut s: ResMut<Seen>| s.0.push("after")).after(BevyTsSet::Update)));
    app.update();
    assert_eq!(app.world().resource::<Seen>().0, ["before", "after"]);
    assert_eq!(said(&mut app), ["a", "b"]);
}

#[test]
fn a_spawn_is_seen_by_the_next_system_in_the_same_frame() {
    let body = "export function a(c: Commands, s: Local<{ n: u32 }>) { if (s.n++ === 0) c.spawn(Enemy); }\nexport function b(q: Query<[Enemy]>) { console.log(`b${q.len}`); }\napp.addSystems(Update, a, b);\n";
    let (_, mut app) = load("bsys-spawn", &src(body));
    app.update();
    assert_eq!(said(&mut app), ["b1"]);
}

#[test]
fn two_configs_with_the_same_condition_keep_separate_state() {
    let body = r#"export function a() { console.log("a"); }
export function b() { console.log("b"); }
export function play(n: NextState<Game>, c: ResMut<Count>) { if (++c.n === 3) n.set(Game.Playing); }
app.initState(Game.Menu).insertResource(Count.of({ n: 0 })).addSystems(Update, play,
  systems(a).runIf(inState(Game.Menu).andThen(runOnce())),
  systems(b).runIf(inState(Game.Playing).andThen(runOnce())));
"#;
    let (_, mut app) = load("bsys-twoconfigs", &src(body));
    updates(&mut app, 6);
    assert_eq!(said(&mut app), ["a", "b"]);
}

#[test]
fn a_body_only_reload_edits_no_schedule_and_a_moved_system_edits_one() {
    let body = |b: &str| src(&format!("export function a() {{ console.log('{b}'); }}\nexport function b() {{}}\napp.addSystems(Update, a, b);\n"));
    let (dir, mut app) = load("bsys-edits", &body("one"));
    app.update();
    let before = edits(&app);
    write(&dir, "game.ts", &body("two"));
    reload(&mut app, false);
    app.update();
    assert_eq!(edits(&app), before, "a body-only change edits no schedule");
    assert_eq!(said(&mut app), ["one", "two"]);

    write(&dir, "game.ts", &src("export function a() { console.log('two'); }\nexport function b() {}\napp.addSystems(Update, b, a);\n"));
    reload(&mut app, false);
    app.update();
    assert_eq!(edits(&app), before + 1, "one schedule edited");
}

/// Reload keeps state: `last_run` (a `Changed` query does not refire), `Local`s, message cursors and
/// a `resourceChanged` tick, for a kept system (body-only change) and for rebuilt ones (reordered).
#[test]
fn a_reload_keeps_every_systems_state() {
    let systems = r#"export function spawn(c: Commands) { c.spawn(Vel.of({ x: 1 })); }
export function send(w: MessageWriter<Ping>, c: ResMut<Count>) { if (c.n++ === 0) w.write(Ping.of({ n: 1 })); }
export function read(r: MessageReader<Ping>) { if (r.len > 0) console.log(`r${r.len}`); }
export function seen(q: Query<[Vel], Changed<Vel>>) { if (q.len > 0) console.log(`seen${q.len}`); }
export function count(l: Local<{ n: u32 }>) { l.n += 1; console.log(`n${l.n}`); }
export function redraw() { console.log("redraw"); }
app.addMessage(Ping).insertResource(Count.of({ n: 0 })).insertResource(Score.of({ value: 0 })).addSystems(Startup, spawn)
"#;
    let forward = src(&format!("{systems}  .addSystems(Update, chain(send, read, seen, count, systems(redraw).runIf(resourceChanged(Score))));\n"));
    let (dir, mut app) = load("bsys-keep", &forward);
    app.update();
    assert_eq!(said(&mut app), ["r1", "seen1", "n1", "redraw"]);

    let before = edits(&app);
    write(&dir, "game.ts", &format!("{forward}// edited\n"));
    reload(&mut app, false);
    updates(&mut app, 2);
    assert_eq!(edits(&app), before, "kept systems");

    let reversed = src(&format!("{systems}  .addSystems(Update, chain(systems(redraw).runIf(resourceChanged(Score)), count, seen, read, send));\n"));
    write(&dir, "game.ts", &reversed);
    reload(&mut app, false);
    updates(&mut app, 2);
    assert!(edits(&app) > before, "rebuilt systems");
    assert_eq!(said(&mut app), ["r1", "seen1", "n1", "redraw", "n2", "n3", "n4", "n5"]);
}

/// A group's gate goes with its condition: a changed predicate (hot reload) and a restart each leave
/// one gate, so the predicate is one call a frame and the schedule holds no stale gate.
#[test]
fn a_changed_group_predicate_and_a_restart_leave_one_gate() {
    let body = |p: &str| src(&format!("export function {p}() {{ return false; }}\nexport function a() {{}}\nexport function b() {{}}\napp.addSystems(Update, systems(a, b).runIf({p}));\n"));
    let (dir, mut app) = load("bsys-groupgate", &body("ready"));
    updates(&mut app, 2);
    let len = |app: &App| app.world().resource::<Schedules>().get(Update).unwrap().systems_len();
    let systems = len(&app);
    assert_eq!(calls_over(&mut app, 3), 3, "one call a frame");

    write(&dir, "game.ts", &body("armed"));
    reload(&mut app, false);
    updates(&mut app, 2);
    assert_eq!(calls_over(&mut app, 3), 3, "one call a frame after the predicate changed");
    assert_eq!(len(&app), systems, "the old gate is gone");

    reload(&mut app, true);
    updates(&mut app, 2);
    assert_eq!(calls_over(&mut app, 3), 3, "one call a frame after a restart");
    assert_eq!(len(&app), systems, "no gate left from before the restart");
}

/// Calls into JS over `n` frames.
fn calls_over(app: &mut App, n: usize) -> u64 {
    let before = calls(app);
    updates(app, n);
    calls(app) - before
}
