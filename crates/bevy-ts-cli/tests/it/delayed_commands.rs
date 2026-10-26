//! Change delayed-commands: `commands.delayed()` from TS on Bevy's own `DelayedCommandQueue`, timed
//! as Bevy times it (docs/time.md, "Delayed commands").

use crate::common;

use bevy::ecs::component::ComponentId;
use bevy::prelude::*;
use bevy::time::{DelayedCommandQueue, DelayedCommandsExt, TimeUpdateStrategy, Virtual};
use bevy_ts::runtime::ScriptClock;
use common::*;
use std::time::Duration;

const HEAD: &str = r#"import { app, Component, Event, FixedUpdate, Message, Startup, Time, u32, Update, With } from "bevy-ts";
import type { Commands, DelayedCommands, Entity, MessageReader, On, Query, Res } from "bevy-ts";
import { DelayedCommandQueue } from "bevy-ts/bevy";
export class Dummy extends Component { static path = "game::Dummy" }
export class Gate extends Component { static path = "game::Gate" }
"#;

/// A Rust twin of the TS `Dummy`, for the same calls made from Rust.
#[derive(Component)]
struct RustDummy;

/// An app on frame steps of `ms` of game time.
fn stepped(name: &str, src: &str, ms: u64) -> App {
    let (_, mut app) = load(name, &format!("{HEAD}{src}"));
    app.insert_resource(TimeUpdateStrategy::ManualDuration(Duration::from_millis(ms)));
    app
}

fn id_of(app: &App, component: &str) -> ComponentId {
    bevy_ts::plugin_api::component_by_name(app.world(), component).expect("live component").0
}

/// Entities with the TS component `component`.
fn count(app: &mut App, component: &str) -> usize {
    let id = id_of(app, component);
    let world = app.world_mut();
    QueryBuilder::<Entity>::new(world).with_id(id).build().iter(world).count()
}

fn rust_count(app: &mut App) -> usize {
    app.world_mut().query_filtered::<(), With<RustDummy>>().iter(app.world()).count()
}

/// Each pending queue's `submit_at`, sorted.
fn submit_ats(app: &mut App) -> Vec<Duration> {
    let mut out: Vec<Duration> = app.world_mut().query::<&DelayedCommandQueue>().iter(app.world()).map(|q| q.submit_at).collect();
    out.sort();
    out
}

fn elapsed(app: &App) -> f64 {
    app.world().resource::<Time<Virtual>>().elapsed_secs_f64()
}

/// What TS logged after `prefix`, in order. `console.log` reaches the log when pumped.
fn said(app: &mut App, prefix: &str) -> Vec<String> {
    bevy_ts::runtime::pump(app.world_mut());
    messages(app).iter().filter_map(|m| m.strip_prefix(prefix).map(str::to_owned)).collect()
}

/// A gate the TS systems below wait for: spawned from Rust, so a restart keeps it.
fn open_gate(app: &mut App) {
    spawn_ts(app, "game::Gate", serde_json::json!({}), ());
}

#[test]
fn bevys_test_matches_rust() {
    // Bevy's `delayed_queues_should_run_with_time_plugin_enabled`, from TS and from Rust.
    let src = r#"
export function queue(commands: Commands) {
  commands.delayed().secs(0.1).spawn(Dummy);
  commands.spawn(Dummy);
  const d = commands.delayed();
  d.secs(0.5).spawn(Dummy);
  const in1 = d.duration(1);
  in1.spawn(Dummy);
  in1.spawn(Dummy);
  in1.spawn(Dummy);
}
let read = false;
export function look(q: Query<[DelayedCommandQueue]>) {
  if (read) return;
  read = true;
  const at: number[] = [];
  for (const [c] of q) at.push(c.submit_at);
  console.log("submit_at " + JSON.stringify(at));
}
app.addSystems(Startup, queue).addSystems(Update, look);
"#;
    let mut ts = stepped("delayed-bevy", src, 200);
    let mut rust = App::new();
    rust.add_plugins(MinimalPlugins).insert_resource(TimeUpdateStrategy::ManualDuration(Duration::from_millis(200)));
    rust.add_systems(Startup, |mut commands: Commands| {
        commands.delayed().secs(0.1).spawn(RustDummy);
        commands.spawn(RustDummy);
        let mut delayed = commands.delayed();
        delayed.secs(0.5).spawn(RustDummy);
        let mut in_1_sec = delayed.duration(Duration::from_secs_f32(1.0));
        in_1_sec.spawn(RustDummy);
        in_1_sec.spawn(RustDummy);
        in_1_sec.spawn(RustDummy);
    });
    for frame in 0..10 {
        ts.update();
        rust.update();
        if frame == 0 {
            let (a, b) = (submit_ats(&mut ts), submit_ats(&mut rust));
            assert_eq!((a.len(), &a), (3, &b), "the queues after the first frame");
            let read: Vec<f64> = serde_json::from_str(&said(&mut ts, "submit_at ")[0]).unwrap();
            let mut read: Vec<Duration> = read.into_iter().map(Duration::from_secs_f64).collect();
            read.sort();
            assert_eq!(read, b, "a TS query reads each queue's submit_at");
        }
        assert_eq!(count(&mut ts, "game::Dummy"), rust_count(&mut rust), "frame {frame}");
    }
    assert_eq!(count(&mut ts, "game::Dummy"), 6);
}

#[test]
fn spawn_now_despawn_later() {
    let src = r#"
let done = false;
export function queue(commands: Commands, time: Res<Time>) {
  if (done) return;
  done = true;
  const d = commands.delayed();
  const e = d.secs(1).spawn(Dummy);
  d.secs(2).entity(e).despawn();
  commands.entity(e).insert(Gate);
  console.log("queued " + JSON.stringify([e, time.elapsed_secs_f64]));
}
app.addSystems(Update, queue);
"#;
    let mut app = stepped("delayed-spawn-now", src, 250);
    app.update();
    let [e, start]: [f64; 2] = serde_json::from_str(&said(&mut app, "queued ")[0]).unwrap();
    let e = Entity::from_bits(e as u64);
    assert!(
        messages(&app).iter().any(|m| m.contains(&format!("insert: entity {e} does not exist"))),
        "a command now names the entity before it is spawned: {:?}",
        messages(&app)
    );
    let mut seen = Vec::new();
    while elapsed(&app) < start + 2.5 {
        app.update();
        let t = elapsed(&app) - start;
        let exists = app.world().get_entity(e).is_ok();
        assert_eq!(exists, (1.0..2.0).contains(&t), "at {t} s of game time");
        seen.push(exists);
    }
    assert!(seen.contains(&true));
}

/// A TS system and a Rust one each queue `delayed().secs(1)` on their first run, in one frame.
fn pair(name: &str, schedule: &str, ms: u64) -> App {
    let src = format!(
        r#"
let done = false;
export function queue(commands: Commands) {{
  if (done) return;
  done = true;
  commands.delayed().secs(1).spawn(Dummy);
}}
app.addSystems({schedule}, queue);
"#
    );
    let mut app = stepped(name, &src, ms);
    let rust = |mut commands: Commands, mut done: Local<bool>| {
        if !*done {
            *done = true;
            commands.delayed().secs(1.0).spawn(RustDummy);
        }
    };
    match schedule {
        "FixedUpdate" => app.add_systems(FixedUpdate, rust),
        _ => app.add_systems(Update, rust),
    };
    app
}

/// Steps `frames` frames with `each` before each, asserting the TS and Rust spawns land on the
/// same frame; the frame they landed on.
fn landed(app: &mut App, frames: usize, mut each: impl FnMut(&mut App, usize)) -> Option<usize> {
    let mut at = None;
    for f in 0..frames {
        each(app, f);
        app.update();
        let (ts, rust) = (count(app, "game::Dummy"), rust_count(app));
        assert_eq!(ts, rust, "frame {f}");
        if ts == 1 && at.is_none() {
            at = Some(f);
        }
    }
    at
}

#[test]
fn delays_follow_the_game_clock() {
    // Frames of 0.25 s: the delay lands on frame 4 (1 s after frame 0) when nothing holds it.
    let mut app = pair("delayed-clock-plain", "Update", 250);
    let plain = landed(&mut app, 12, |_, _| {}).expect("it landed");
    // The game pauses `Time<Virtual>` for 8 frames (2 s of wall time) after queueing.
    let mut app = pair("delayed-clock-game-pause", "Update", 250);
    let paused = landed(&mut app, 20, |app, f| {
        let mut v = app.world_mut().resource_mut::<Time<Virtual>>();
        match f {
            1 => v.pause(),
            9 => v.unpause(),
            _ => {}
        }
    });
    assert_eq!(paused, Some(plain + 8), "a game pause holds the delay");
    // Half speed doubles it in wall time.
    let mut app = pair("delayed-clock-speed", "Update", 250);
    let slow = landed(&mut app, 20, |app, f| {
        if f == 1 {
            app.world_mut().resource_mut::<Time<Virtual>>().set_relative_speed(0.5);
        }
    });
    assert!(slow.is_some_and(|s| s >= 2 * plain - 1), "half speed: {slow:?}, plain {plain}");
    // The editor's `bevy-ts/pause` holds it too.
    let mut app = pair("delayed-clock-editor", "Update", 250);
    let held = landed(&mut app, 20, |app, f| match f {
        1 => app.world_mut().resource_mut::<ScriptClock>().paused = true,
        9 => app.world_mut().resource_mut::<ScriptClock>().paused = false,
        _ => {}
    });
    assert_eq!(held, Some(plain + 8), "the editor's pause holds the delay");
}

#[test]
fn fixed_update_delays_count_from_the_fixed_clock() {
    let mut app = pair("delayed-fixed", "FixedUpdate", 200);
    until(&mut app, "no fixed step queued", |app| !submit_ats(app).is_empty());
    let queues = app.world_mut().query::<(&DelayedCommandQueue, Has<bevy_ts::commands::TsSpawned>)>().iter(app.world()).map(|(q, ts)| (q.submit_at, ts)).collect::<Vec<_>>();
    assert_eq!(queues.len(), 2, "{queues:?}");
    assert_eq!(queues[0].0, queues[1].0, "TS and Rust count from the same clock: {queues:?}");
    assert!(landed(&mut app, 10, |_, _| {}).is_some());
}

#[test]
fn nested_delays_add_up() {
    let src = r#"
let done = false;
export function queue(commands: Commands, time: Res<Time>) {
  if (done) return;
  done = true;
  commands.delayed().secs(1).delayed().secs(1).spawn(Dummy);
  console.log("queued " + time.elapsed_secs_f64);
}
app.addSystems(Update, queue);
"#;
    let mut app = stepped("delayed-nested", src, 250);
    app.update();
    let start: f64 = said(&mut app, "queued ")[0].parse().unwrap();
    while elapsed(&app) < start + 2.5 {
        app.update();
        let t = elapsed(&app) - start;
        assert_eq!(count(&mut app, "game::Dummy"), usize::from(t >= 2.0), "at {t} s");
    }
}

#[test]
fn every_command_kind_can_wait() {
    let src = r#"
export class Ping extends Message { static path = "game::DelayedPing"; declare n: u32 }
export class Boom extends Event { static path = "game::DelayedBoom"; declare n: u32 }
export class Child extends Component { static path = "game::Child" }
export class Batch extends Component { static path = "game::Batch" }
export function wave() { console.log("seen runSystem"); }
export function onBoom(on: On<Boom>) { console.log("seen trigger " + on.n); }
export function read(r: MessageReader<Ping>) { if (r.len > 0) console.log("seen write"); }
let done = false;
export function queue(commands: Commands, time: Res<Time>) {
  if (done) return;
  done = true;
  const later = commands.delayed().secs(1);
  later.write(Ping.of({ n: 1 }));
  later.trigger(Boom.of({ n: 2 }));
  later.runSystem(wave);
  const parent = later.spawn(Dummy);
  later.entity(parent).withChildren((p) => { p.spawn(Child); p.spawn(Child); });
  later.spawnBatch([[Batch], [Batch], [Batch]]);
  console.log("queued " + time.elapsed_secs_f64);
}
app.registerSystem(wave);
app.addObserver(onBoom).addSystems(Update, queue, read);
"#;
    let mut app = stepped("delayed-every-kind", src, 250);
    app.update();
    let start: f64 = said(&mut app, "queued ")[0].parse().unwrap();
    let kinds = ["runSystem", "trigger 2", "write"];
    let mut first: Vec<Option<f64>> = vec![None; kinds.len()];
    let mut structural = None;
    while elapsed(&app) < start + 2.0 {
        app.update();
        let t = elapsed(&app) - start;
        let seen = said(&mut app, "seen ");
        for (k, at) in kinds.iter().zip(first.iter_mut()) {
            if at.is_none() && seen.iter().any(|s| s == k) {
                *at = Some(t);
            }
        }
        let built = (count(&mut app, "game::Dummy"), count(&mut app, "game::Child"), count(&mut app, "game::Batch"));
        if built != (0, 0, 0) {
            assert_eq!(built, (1, 2, 3), "at {t} s");
            structural.get_or_insert(t);
        }
    }
    // Each is seen once its delay has passed, within a frame.
    for (k, at) in kinds.iter().zip(&first) {
        assert!(at.is_some_and(|t| (1.0..=1.25).contains(&t)), "{k} seen at {at:?}");
    }
    assert!(structural.is_some_and(|t| (1.0..1.25).contains(&t)), "spawns at {structural:?}");
}

#[test]
fn a_bad_delay_throws() {
    let src = r#"
let done = false;
export function queue(commands: Commands) {
  if (done) return;
  done = true;
  const d = commands.delayed();
  for (const f of [() => d.secs(-1), () => d.secs(NaN), () => d.duration(Infinity)]) {
    try { f(); } catch (e) { console.log("caught " + (e as Error).message); }
  }
  commands.spawn(Dummy);
}
app.addSystems(Update, queue);
"#;
    let mut app = stepped("delayed-bad", src, 250);
    app.update();
    let caught = said(&mut app, "caught ");
    assert_eq!(caught.len(), 3, "{caught:?}");
    for (c, call) in caught.iter().zip(["delayed().secs(-1)", "delayed().secs(NaN)", "delayed().duration(Infinity)"]) {
        assert!(c.starts_with(call), "{c}");
    }
    assert_eq!(count(&mut app, "game::Dummy"), 1, "a caught throw keeps the run's other commands");
    assert!(submit_ats(&mut app).is_empty());
}

#[test]
fn a_throwing_run_drops_its_delayed_commands() {
    let src = r#"
let done = false;
export function queue(commands: Commands) {
  if (done) return;
  done = true;
  commands.delayed().secs(0.25).spawn(Dummy);
  throw new Error("boom");
}
app.addSystems(Update, queue);
"#;
    let mut app = stepped("delayed-throw", src, 250);
    updates(&mut app, 4);
    assert!(messages(&app).iter().any(|m| m.contains("boom")));
    assert!(submit_ats(&mut app).is_empty());
    assert_eq!(count(&mut app, "game::Dummy"), 0);
}

#[test]
fn delayed_commands_kept_past_the_run_throw() {
    let src = r#"
let kept: DelayedCommands | null = null;
let later: Commands | null = null;
export function queue(commands: Commands) {
  if (!kept) {
    kept = commands.delayed();
    later = kept.secs(1);
    return;
  }
  for (const f of [() => kept!.secs(1), () => later!.spawn(Dummy), () => later!.delayed()]) {
    try { f(); } catch (e) { console.log("caught " + (e as Error).message); }
  }
  kept = null;
}
app.addSystems(Update, queue);
"#;
    let mut app = stepped("delayed-kept", src, 250);
    updates(&mut app, 2);
    let caught = said(&mut app, "caught ");
    assert_eq!(caught, vec!["delayed commands are used in the run that made them"; 3]);
    updates(&mut app, 6);
    assert_eq!(count(&mut app, "game::Dummy"), 0, "nothing kept queued anything");
}

/// The gate's system: one `delayed().secs(5).spawn(Dummy)` per gate, which it despawns.
const GATED: &str = r#"
export function queue(commands: Commands, gates: Query<[Entity], With<Gate>>) {
  for (const [g] of gates) {
    commands.delayed().secs(5).spawn(Dummy);
    commands.entity(g).despawn();
  }
}
app.addSystems(Update, queue);
"#;

#[test]
fn a_stop_drops_pending_queues() {
    let mut app = stepped("delayed-stop", GATED, 250);
    open_gate(&mut app);
    updates(&mut app, 2);
    assert_eq!(submit_ats(&mut app).len(), 1, "queued");
    // A Stop, as the viewport's: the TS world rebuilt from the live generation, the clocks reset.
    bevy_ts::generation::rebuild(app.world_mut()).unwrap();
    bevy_ts::runtime::reset_clock(app.world_mut());
    assert!(submit_ats(&mut app).is_empty(), "the Stop despawned the queue");
    app.world_mut().resource_mut::<Time<Virtual>>().unpause();
    updates(&mut app, 44);
    assert!(elapsed(&app) > 10.0);
    assert_eq!(count(&mut app, "game::Dummy"), 0);
}

#[test]
fn a_reload_drops_pending_queues() {
    let (dir, mut app) = load("delayed-reload", &format!("{HEAD}{GATED}"));
    app.insert_resource(TimeUpdateStrategy::ManualDuration(Duration::from_millis(250)));
    open_gate(&mut app);
    updates(&mut app, 2);
    let generation = app.world().resource::<bevy_ts::generation::Generations>().live;
    assert_eq!(submit_ats(&mut app).len(), 1, "queued");
    write(&dir, "game.ts", &format!("{HEAD}{GATED}// edited\n"));
    reload(&mut app, false);
    assert_ne!(app.world().resource::<bevy_ts::generation::Generations>().live, generation);
    updates(&mut app, 24);
    assert!(elapsed(&app) > 5.5);
    assert_eq!(count(&mut app, "game::Dummy"), 0, "nothing applied");
    let dropped: Vec<String> = messages(&app).into_iter().filter(|m| m.contains("dropped by a reload")).collect();
    assert_eq!(dropped, vec![format!("delayed commands from generation {generation} dropped by a reload (1 command)")]);
}
