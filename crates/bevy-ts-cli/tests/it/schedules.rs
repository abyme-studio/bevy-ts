//! ts-schedules: every main and fixed schedule, the startup lists, custom schedules, running a
//! schedule or a one-shot system from commands, and Rust's side of each.

use crate::common;

use bevy::ecs::schedule::ScheduleLabel;
use bevy::prelude::*;
use bevy::time::TimeUpdateStrategy;
use bevy_ts::schedules::{RunTsSystem, TsSchedule};
use bevy_ts_native::BevyTsNativeExt;
use common::*;

const HEAD: &str = r#"import { app, Event, FixedFirst, FixedLast, FixedPostUpdate, FixedPreUpdate, FixedUpdate, Last, PostStartup, PostUpdate, PreStartup, Resource, schedule, ScheduleLabel, Startup, systems, u32, Update, f32 } from "bevy-ts";
import type { Commands, In, Res, ResMut } from "bevy-ts";
export class Score extends Resource { static path = "game::Score"; declare value: u32 }
export class SpawnAt extends Event { static path = "game::SpawnAt"; declare x: f32 }
export class Waves extends ScheduleLabel { static path = "game::Waves" }
"#;

fn src(body: &str) -> String {
    format!("{HEAD}{body}")
}

/// The log lines that are bare words a system printed, in order.
fn said(app: &mut App) -> Vec<String> {
    // console output reaches the log when the runtime is pumped, in the next frame's `First`.
    bevy_ts::runtime::pump(app.world_mut());
    messages(app).into_iter().filter(|m| !m.contains(' ')).collect()
}

fn systems_in(app: &mut App, label: impl ScheduleLabel) -> usize {
    app.world_mut().resource_mut::<Schedules>().get_mut(label).map_or(0, |s| {
        s.initialize(&mut World::new()).ok();
        s.systems_len()
    })
}

#[test]
fn a_system_in_last_runs_after_post_update_and_its_stage_is_made_on_load() {
    let (dir, mut app) = load("sched-last", &src(""));
    app.update();
    let before = systems_in(&mut app, Last);
    write(
        &dir,
        "game.ts",
        &src("export function late() { console.log('late'); }\nexport function post() { console.log('post'); }\napp.addSystems(Last, late).addSystems(PostUpdate, post);\n"),
    );
    let r = reload(&mut app, false);
    assert!(r.errors.is_empty(), "{:?}", r.errors);
    updates(&mut app, 2);
    assert_eq!(said(&mut app), ["post", "late", "post", "late"]);
    assert!(systems_in(&mut app, Last) > before, "the stage made in Last");
    let identity = app.world().resource::<bevy_ts::systems::TsSystems>().iter().map(|s| s.decl.name.clone()).find(|n| n.ends_with("#late")).unwrap();
    let schedules = app.world().resource::<Schedules>();
    let late = schedules.get(Last).unwrap().graph().systems_in_set(bevy_ts::bevy_system::TsSystemSet(identity).intern()).unwrap().len();
    assert_eq!(late, 1, "late is one Bevy system in Last");
}

#[test]
fn an_unused_schedule_has_no_bevy_ts_system() {
    let (_, mut app) = load("sched-unused", &src("export function a() {}\napp.addSystems(Update, a);\n"));
    let before = systems_in(&mut app, SpawnScene);
    app.update();
    assert_eq!(systems_in(&mut app, SpawnScene), before);
    assert!(!app.world().resource::<bevy_ts::schedules::MadeStages>().stages.contains(&bevy_ts::manifest::Stage::SpawnScene));
}

#[test]
fn fixed_schedules_run_in_order_once_per_step() {
    let body = r#"export function a() { console.log("first"); }
export function b() { console.log("pre"); }
export function c() { console.log("update"); }
export function d() { console.log("post"); }
export function e() { console.log("last"); }
app.addSystems(FixedFirst, a).addSystems(FixedPreUpdate, b).addSystems(FixedUpdate, c).addSystems(FixedPostUpdate, d).addSystems(FixedLast, e);
"#;
    let (_, mut app) = load("sched-fixed", &src(body));
    let step = app.world().resource::<Time<Fixed>>().timestep();
    app.insert_resource(TimeUpdateStrategy::ManualDuration(step * 2));
    // The first frame starts the clock; the next two run two fixed steps each.
    app.update();
    let skip = said(&mut app).len();
    updates(&mut app, 2);
    let got: Vec<String> = said(&mut app).into_iter().skip(skip).collect();
    let one = ["first", "pre", "update", "post", "last"];
    let want: Vec<&str> = one.iter().chain(&one).chain(&one).chain(&one).copied().collect();
    assert_eq!(got, want);
}

#[test]
fn startup_lists_run_in_order_once_with_commands_between() {
    let body = r#"export function a() { console.log("a"); }
export function b(s: Res<Score>) { console.log(s.value === 7 ? "b7" : "b?"); }
export function c(s: ResMut<Score>) { console.log("c"); s.value = 7; }
app.insertResource(Score.of({ value: 1 })).addSystems(PostStartup, a).addSystems(Startup, b).addSystems(PreStartup, c);
"#;
    let (dir, mut app) = load("sched-startups", &src(body));
    app.update();
    assert_eq!(said(&mut app), ["c", "b7", "a"]);
    write(&dir, "game.ts", &src(&format!("{body}\n// edited\n")));
    reload(&mut app, false);
    app.update();
    assert_eq!(said(&mut app), ["c", "b7", "a"], "a reload runs none of them again");
}

#[test]
fn a_custom_schedule_after_update_moves_on_reload() {
    let body = |place: &str| {
        format!(
            "export function spawnWave() {{ console.log('wave'); }}\nexport function tick() {{ console.log('update'); }}\napp.{place}(Update, Waves).addSystems(Waves, spawnWave).addSystems(Update, tick);\n"
        )
    };
    let (dir, mut app) = load("sched-waves", &src(&body("insertScheduleAfter")));
    updates(&mut app, 2);
    assert_eq!(said(&mut app), ["update", "wave", "update", "wave"]);
    let order = app.world().resource::<bevy::app::MainScheduleOrder>().labels.clone();
    let waves = TsSchedule::new("game::Waves").intern();
    let at = order.iter().position(|l| *l == waves).unwrap();
    assert_eq!(order[at - 1], Update.intern());
    assert_eq!(order[at + 1], SpawnScene.intern());

    write(&dir, "game.ts", &src(&body("insertScheduleBefore")));
    let r = reload(&mut app, false);
    assert!(r.errors.is_empty(), "{:?}", r.errors);
    let n = said(&mut app).len();
    updates(&mut app, 1);
    assert_eq!(said(&mut app)[n..], ["wave", "update"]);
    let order = app.world().resource::<bevy::app::MainScheduleOrder>().labels.clone();
    assert_eq!(order.iter().filter(|l| **l == waves).count(), 1);
}

/// Saves `src` and applies it as the watcher does: compiled, sent, applied in `First` inside a frame.
fn save_and_apply_in_frame(app: &mut App, dir: &std::path::Path, src: &str) {
    use bevy_ts::generation::{DevChannel, DevEvent, ModuleSource};
    write(dir, "game.ts", src);
    let (tx, channel) = DevChannel::new();
    app.insert_resource(channel);
    let set = bevy_ts_cli::compile::Compiler::new(dir.join("game.ts"), true).unwrap().compile();
    tx.send(DevEvent::Compiled(set, std::time::Duration::ZERO)).unwrap();
    app.update();
    let errors: Vec<String> = messages(app).into_iter().filter(|m| m.contains("failed")).collect();
    assert!(errors.is_empty(), "{errors:?}");
}

#[test]
fn a_schedule_moved_by_a_saved_file_moves_from_the_next_frame() {
    let body = |place: &str| format!("export function spawnWave() {{ console.log('wave'); }}\nexport function tick() {{ console.log('update'); }}\napp.{place}(Update, Waves).addSystems(Waves, spawnWave).addSystems(Update, tick);\n");
    let (dir, mut app) = load("sched-wavessaved", &src(&body("insertScheduleAfter")));
    updates(&mut app, 2);
    // The save is applied inside a frame, while `Main` holds `MainScheduleOrder`.
    save_and_apply_in_frame(&mut app, &dir, &src(&body("insertScheduleBefore")));
    let n = said(&mut app).len();
    updates(&mut app, 1);
    assert_eq!(said(&mut app)[n..], ["wave", "update"]);
    let waves = TsSchedule::new("game::Waves").intern();
    let order = app.world().resource::<bevy::app::MainScheduleOrder>().labels.clone();
    assert_eq!(order.iter().filter(|l| **l == waves).count(), 1);
    assert_eq!(order[order.iter().position(|l| *l == waves).unwrap() + 1], Update.intern());
}

#[test]
fn a_schedule_moved_to_the_fixed_order_leaves_the_main_one() {
    let (dir, mut app) = load("sched-wavesfixed", &src("export function w() {}\napp.insertScheduleAfter(Update, Waves).addSystems(Waves, w);\n"));
    app.update();
    save_and_apply_in_frame(&mut app, &dir, &src("export function w() {}\napp.insertFixedScheduleAfter(FixedUpdate, Waves).addSystems(Waves, w);\n"));
    app.update();
    let waves = TsSchedule::new("game::Waves").intern();
    assert!(!app.world().resource::<bevy::app::MainScheduleOrder>().labels.contains(&waves));
    let fixed = app.world().resource::<bevy::app::FixedMainScheduleOrder>().labels.clone();
    assert_eq!(fixed[fixed.iter().position(|l| *l == FixedUpdate.intern()).unwrap() + 1], waves);
}

#[test]
fn an_anchor_the_new_generation_drops_fails_the_load() {
    let two = "export class Boss extends ScheduleLabel { static path = \"game::Boss\" }\napp.insertScheduleAfter(Update, Waves).insertScheduleAfter(Waves, Boss);\n";
    let (dir, mut app) = load("sched-dropanchor", &src(two));
    app.update();
    write(&dir, "game.ts", &src("export class Boss extends ScheduleLabel { static path = \"game::Boss\" }\napp.insertScheduleAfter(Waves, Boss);\n"));
    let r = reload(&mut app, false);
    assert!(r.errors.iter().any(|e| e.message.contains("game::Boss") && e.message.contains("Waves")), "{:?}", r.errors);
}

#[test]
fn inserting_relative_to_a_schedule_not_in_the_order_fails() {
    let errors = load_errors("sched-badorder", &src("export class Other extends ScheduleLabel { static path = \"game::Other\" }\napp.insertScheduleAfter(Other, Waves);\n"));
    assert!(errors.iter().any(|e| e.message.contains("game::Waves") && e.message.contains("game::Other")), "{errors:?}");
}

#[test]
fn run_schedule_runs_it_once_before_the_next_system() {
    let body = r#"export function spawnWave() { console.log("wave"); }
export function caller(c: Commands, s: ResMut<Score>) { if (s.value++ === 0) { console.log("call"); c.runSchedule(Waves); } }
export function next() { console.log("next"); }
app.insertResource(Score.of({ value: 0 })).initSchedule(Waves).addSystems(Waves, spawnWave).addSystems(Update, caller, next);
"#;
    let (_, mut app) = load("sched-runschedule", &src(body));
    updates(&mut app, 3);
    assert_eq!(said(&mut app), ["call", "wave", "next", "next", "next"]);
}

#[test]
fn a_schedule_run_inside_itself_is_skipped_with_an_error() {
    let body = r#"export function again(c: Commands) { console.log("wave"); c.runSchedule(Waves); }
export function caller(c: Commands, s: ResMut<Score>) { if (s.value++ === 0) c.runSchedule(Waves); }
app.insertResource(Score.of({ value: 0 })).addSystems(Waves, again).addSystems(Update, caller);
"#;
    let (_, mut app) = load("sched-reentry", &src(body));
    updates(&mut app, 2);
    assert_eq!(said(&mut app), ["wave"]);
    let m = messages(&app);
    assert!(m.iter().any(|l| l.contains("runSchedule(game::Waves)") && l.contains("again")), "{m:?}");
}

#[derive(ScheduleLabel, Debug, Clone, PartialEq, Eq, Hash)]
struct NetTick;

#[test]
fn a_ports_schedule_runs_its_ts_systems_when_the_port_runs_it() {
    let dir = project("sched-netschedule", &[("game.ts", &src("export function sync() { console.log('sync'); }\napp.addSystems(schedule(\"net:tick\"), sync);\n"))]);
    let mut app = app(&dir.join("game.ts"));
    app.ts_schedule("net:tick", NetTick);
    app.init_schedule(NetTick);
    let r = reload(&mut app, false);
    assert!(r.errors.is_empty(), "{:?}", r.errors);
    updates(&mut app, 2);
    assert!(said(&mut app).is_empty());
    app.world_mut().run_schedule(NetTick);
    assert_eq!(said(&mut app), ["sync"]);
}

#[test]
fn run_system_runs_a_one_shot_after_the_callers_commands() {
    let body = r#"export function spawnWave() { console.log("wave"); }
export function caller(c: Commands, s: ResMut<Score>) { if (s.value++ === 0) { c.runSystem(spawnWave); c.runSystem(id); } }
export function next() { console.log("next"); }
const id = app.registerSystem(spawnWave);
app.insertResource(Score.of({ value: 0 })).addSystems(Update, caller, next);
"#;
    let (_, mut app) = load("sched-oneshot", &src(body));
    updates(&mut app, 2);
    assert_eq!(said(&mut app), ["wave", "wave", "next", "next"]);
}

#[test]
fn run_system_with_passes_its_input() {
    let body = r#"export function spawnAt(at: In<SpawnAt>, c: Commands) { console.log(`x${at.x}`); }
export function caller(c: Commands, s: ResMut<Score>) { if (s.value++ === 0) c.runSystemWith(spawnAt, SpawnAt.of({ x: 3 })); }
app.registerSystem(spawnAt);
app.insertResource(Score.of({ value: 0 })).addSystems(Update, caller);
"#;
    let (_, mut app) = load("sched-oneshotin", &src(body));
    updates(&mut app, 2);
    assert_eq!(said(&mut app), ["x3"]);
}

#[test]
fn running_an_unregistered_system_throws() {
    let body = r#"export function other() {}
export function caller(c: Commands) { c.runSystem(other); }
app.addSystems(Update, caller);
"#;
    let (_, mut app) = load("sched-unregistered", &src(body));
    app.update();
    let m = messages(&app);
    assert!(m.iter().any(|l| l.contains("register it with app.registerSystem") && l.contains("other")), "{m:?}");
}

#[test]
fn in_anywhere_but_first_fails_the_load() {
    let errors = load_errors("sched-insecond", &src("export function spawnAt(c: Commands, at: In<SpawnAt>) {}\napp.registerSystem(spawnAt);\n"));
    assert!(errors.iter().any(|e| e.message.contains("In<SpawnAt> must be the first param") && e.location.as_ref().is_some_and(|l| l.line == 6)), "{errors:?}");
}

#[test]
fn rust_runs_a_ts_one_shot_with_run_ts_system() {
    let body = r#"export function spawnAt(at: In<SpawnAt>) { console.log(`x${at.x}`); }
export function plain() { console.log("plain"); }
app.registerSystem(spawnAt);
app.registerSystem(plain);
"#;
    let (_, mut app) = load("sched-runtssystem", &src(body));
    app.world_mut().commands().queue(RunTsSystem::new("game.ts#plain"));
    app.world_mut().flush();
    app.world_mut().commands().queue(RunTsSystem::with("game.ts#spawnAt", 5.0f32.to_le_bytes().to_vec()));
    app.world_mut().flush();
    assert_eq!(said(&mut app), ["plain", "x5"]);
}

#[test]
fn a_one_shot_keeps_its_local_across_runs() {
    let body = r#"import type { Local } from "bevy-ts";
export function count(n: Local<{ runs: u32 }>) { n.runs += 1; console.log(`run${n.runs}`); }
app.registerSystem(count);
"#;
    let (_, mut app) = load("sched-oneshotlocal", &src(body));
    for _ in 0..2 {
        app.world_mut().commands().queue(RunTsSystem::new("game.ts#count"));
        app.world_mut().flush();
    }
    assert_eq!(said(&mut app), ["run1", "run2"]);
}

/// native-ordering "After propagation": a system after `bevy:transform-propagate` reads this
/// frame's `GlobalTransform`.
#[test]
fn after_propagation_reads_this_frames_global_transform() {
    let body = r#"import { GlobalTransform, systemSet, Transform } from "bevy-ts";
import type { Mut, Query } from "bevy-ts";
export function move(q: Query<[Mut<Transform>]>) { q.each((t) => { t.translation.x += 1; }); }
export function follow(q: Query<[GlobalTransform]>) { q.each((g) => console.log(`at${g.translation().x}`)); }
app.addSystems(Update, move).addSystems(PostUpdate, systems(follow).after(systemSet("bevy:transform-propagate")));
"#;
    let (_, mut app) = load("sched-afterpropagate", &src(body));
    app.add_plugins(bevy::transform::TransformPlugin);
    app.world_mut().spawn(Transform::default());
    updates(&mut app, 2);
    assert_eq!(said(&mut app), ["at1", "at2"]);
}
