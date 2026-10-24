//! Change script-time, stage 1: Bevy's four clocks read from TS, the editor clock leaving a
//! game's pause alone, and `Timer`/`Stopwatch` ported from `bevy_time` (docs/time.md).

use crate::common;

use bevy::prelude::*;
use bevy::time::{Fixed, Real, Stopwatch, TimeUpdateStrategy, Timer, TimerMode, Virtual};
use bevy_ts::runtime::ScriptClock;
use common::*;
use serde_json::{Value, json};
use std::time::Duration;

const HEAD: &str = r#"import { app, Component, Fixed, FixedUpdate, Real, Resource, Stopwatch, Time, Timer, Update, Virtual } from "bevy-ts";
import type { Local, Mut, Query, Res, ResMut } from "bevy-ts";
"#;

/// The JSON a TS system logged after `prefix`, one per line, in order. `console.log` reaches the
/// log when the backend is pumped, in the next frame's `First`: pump now.
fn logged(app: &mut App, prefix: &str) -> Vec<Value> {
    bevy_ts::runtime::pump(app.world_mut());
    messages(app).iter().filter_map(|m| m.strip_prefix(prefix)).map(|j| serde_json::from_str(j).unwrap()).collect()
}

/// An app on fixed frame steps of `ms`, with no max-delta clamp.
fn stepped(src: &str, name: &str, ms: u64) -> App {
    let (_, mut app) = load(name, &format!("{HEAD}{src}"));
    app.insert_resource(TimeUpdateStrategy::ManualDuration(Duration::from_millis(ms)));
    app.world_mut().resource_mut::<Time<Virtual>>().set_max_delta(Duration::from_secs(10));
    app
}

fn f32_of(v: &Value) -> f32 {
    v.as_f64().unwrap() as f32
}

#[test]
fn clocks_read_bevy_values() {
    let src = r#"
export function clocks(time: Res<Time>, real: Res<Time<Real>>, game: Res<Time<Virtual>>, fixed: Res<Time<Fixed>>) {
  console.log("clocks " + JSON.stringify({ time, real, game, fixed }));
}
app.addSystems(Update, clocks);
"#;
    let mut app = stepped(src, "time-clocks", 20);
    app.world_mut().resource_mut::<Time<Virtual>>().set_relative_speed(0.5);
    app.world_mut().resource_mut::<Time<Fixed>>().set_timestep_hz(64.0);
    updates(&mut app, 4);
    let all = logged(&mut app, "clocks ");
    let last = all.last().expect("the system ran");
    let w = app.world();
    let (time, real, game, fixed) = (w.resource::<Time>(), w.resource::<Time<Real>>(), w.resource::<Time<Virtual>>(), w.resource::<Time<Fixed>>());
    // Every clock's shared fields, as Bevy's getters give them.
    for (view, delta, elapsed, wrapped, period) in [
        (&last["time"], time.delta_secs_f64(), time.elapsed_secs_f64(), time.elapsed_secs_wrapped_f64(), time.wrap_period()),
        (&last["real"], real.delta_secs_f64(), real.elapsed_secs_f64(), real.elapsed_secs_wrapped_f64(), real.wrap_period()),
        (&last["game"], game.delta_secs_f64(), game.elapsed_secs_f64(), game.elapsed_secs_wrapped_f64(), game.wrap_period()),
        (&last["fixed"], fixed.delta_secs_f64(), fixed.elapsed_secs_f64(), fixed.elapsed_secs_wrapped_f64(), fixed.wrap_period()),
    ] {
        assert_eq!(view["delta_secs_f64"].as_f64(), Some(delta), "{view}");
        assert_eq!(view["elapsed_secs_f64"].as_f64(), Some(elapsed), "{view}");
        assert_eq!(view["elapsed_secs_wrapped_f64"].as_f64(), Some(wrapped), "{view}");
        assert_eq!(view["wrap_period"].as_f64(), Some(period.as_secs_f64()), "{view}");
        assert_eq!(f32_of(&view["delta_secs"]), delta as f32);
        assert_eq!(f32_of(&view["elapsed_secs_wrapped"]), wrapped as f32);
    }
    // The game clock's state; real time runs twice as fast as the game's.
    let g = &last["game"];
    assert_eq!((g["relative_speed"].as_f64(), g["is_paused"].as_bool(), g["was_paused"].as_bool()), (Some(0.5), Some(false), Some(false)));
    assert_eq!(g["effective_speed_f64"].as_f64(), Some(0.5));
    assert_eq!(g["max_delta"].as_f64(), Some(10.0));
    let (rd, td) = (last["real"]["delta_secs_f64"].as_f64().unwrap(), last["time"]["delta_secs_f64"].as_f64().unwrap());
    assert!(td > 0.0 && (rd / td - 2.0).abs() < 1e-6, "real {rd}, game {td}");
    // The fixed clock in Update.
    let f = &last["fixed"];
    assert_eq!(f["timestep"].as_f64(), Some(1.0 / 64.0));
    assert_eq!(f["overstep"].as_f64(), Some(fixed.overstep().as_secs_f64()));
    let frac = f["overstep_fraction_f64"].as_f64().unwrap();
    assert!((0.0..1.0).contains(&frac), "{frac}");
    // `Time<Real>` shows no Instants.
    assert!(last["real"].get("startup").is_none() && last["real"].get("is_paused").is_none());
}

#[test]
fn fixed_update_reads_timestep() {
    let src = r#"
export function step(time: Res<Time>) { console.log("fixed " + JSON.stringify(time.delta_secs_f64)); }
app.addSystems(FixedUpdate, step);
"#;
    let mut app = stepped(src, "time-fixed", 50);
    app.world_mut().resource_mut::<Time<Fixed>>().set_timestep_hz(64.0);
    updates(&mut app, 6);
    let deltas = logged(&mut app, "fixed ");
    assert!(deltas.len() >= 6, "fixed steps ran: {deltas:?}");
    assert!(deltas.iter().all(|d| d.as_f64() == Some(1.0 / 64.0)), "{deltas:?}");
}

#[test]
fn clock_writes_fail_the_load() {
    // The clocks TS writes are time_writes.rs's; these two it never writes.
    let head = "import { app, Real, Time, Update } from \"bevy-ts\";\nimport type { ResMut } from \"bevy-ts\";\n";
    let errors = load_errors("time-write", &format!("{head}export function w(t: ResMut<Time<Real>>) {{}}\napp.addSystems(Update, w);\n"));
    let e = &errors[0];
    assert!(e.message.contains("ResMut<Time<Real>> is not allowed"), "{e:?}");
    assert_eq!(e.location.as_ref().map(|l| l.line), Some(3), "{e:?}");
    let errors = load_errors("time-write-generic", &format!("{head}export function w(t: ResMut<Time>) {{}}\napp.addSystems(Update, w);\n"));
    assert!(errors[0].message.contains("ResMut<Time> is not allowed"), "{:?}", errors[0]);
}

const COUNTER: &str = r#"
export function ran(time: Res<Time>) { console.log("ran " + JSON.stringify(time.delta_secs_f64)); }
app.addSystems(Update, ran);
"#;

#[test]
fn game_pause_survives_next_frame() {
    let mut app = stepped(COUNTER, "time-game-pause", 16);
    updates(&mut app, 2);
    // A game pausing its clock from Rust, as `Time<Virtual>::pause()` in a system does.
    app.world_mut().resource_mut::<Time<Virtual>>().pause();
    let before = logged(&mut app, "ran ").len();
    for _ in 0..10 {
        app.update();
        assert!(app.world().resource::<Time<Virtual>>().is_paused(), "the editor clock unpaused the game");
    }
    let after = logged(&mut app, "ran ");
    assert_eq!(after.len(), before + 10, "the stages keep running");
    assert!(after[before..].iter().all(|d| d.as_f64() == Some(0.0)), "{after:?}");
}

#[test]
fn editor_resume_keeps_game_pause() {
    let mut app = stepped(COUNTER, "time-editor-resume", 16);
    updates(&mut app, 2);
    app.world_mut().resource_mut::<Time<Virtual>>().pause();
    app.update();
    // `bevy-ts/pause` true, then false.
    app.world_mut().resource_mut::<ScriptClock>().paused = true;
    updates(&mut app, 3);
    let paused_runs = logged(&mut app, "ran ").len();
    updates(&mut app, 2);
    assert_eq!(logged(&mut app, "ran ").len(), paused_runs, "the stages stop while the editor pauses");
    // A frame stepped over the game's pause runs the stages and advances no virtual time.
    app.world_mut().resource_mut::<ScriptClock>().step_frames = 1;
    app.update();
    let stepped = logged(&mut app, "ran ");
    assert_eq!((stepped.len(), stepped.last().and_then(Value::as_f64)), (paused_runs + 1, Some(0.0)));
    app.world_mut().resource_mut::<ScriptClock>().paused = false;
    updates(&mut app, 3);
    assert_eq!(logged(&mut app, "ran ").len(), paused_runs + 4, "the stages run again");
    assert!(app.world().resource::<Time<Virtual>>().is_paused(), "the game's pause holds");
    // The game resumes its own clock; the editor's next pause and resume still work.
    app.world_mut().resource_mut::<Time<Virtual>>().unpause();
    app.world_mut().resource_mut::<ScriptClock>().paused = true;
    app.update();
    assert!(app.world().resource::<Time<Virtual>>().is_paused());
    app.world_mut().resource_mut::<ScriptClock>().paused = false;
    app.update();
    assert!(!app.world().resource::<Time<Virtual>>().is_paused(), "the editor undoes its own pause");
    assert!(logged(&mut app, "ran ").last().unwrap().as_f64().unwrap() > 0.0);
}

/// Bevy's timer as the TS view's JSON.
fn timer_json(t: &Timer) -> Value {
    json!({
        "elapsed": t.elapsed().as_secs_f64(),
        "duration": t.duration().as_secs_f64(),
        "times_finished_this_tick": t.times_finished_this_tick(),
        "mode": if t.mode() == TimerMode::Repeating { "Repeating" } else { "Once" },
        "paused": t.is_paused(),
        "finished": t.is_finished(),
    })
}

/// A TS timer matches Bevy's: flags, counts and mode exactly, seconds within 1e-9.
fn assert_timer(ts: &Value, bevy: &Timer, at: &str) {
    let b = timer_json(bevy);
    for k in ["times_finished_this_tick", "mode", "paused", "finished"] {
        assert_eq!(ts[k], b[k], "{at}: {k}: ts {ts}, bevy {b}");
    }
    for k in ["elapsed", "duration"] {
        let (x, y) = (ts[k].as_f64().unwrap(), b[k].as_f64().unwrap());
        assert!((x - y).abs() < 1e-9, "{at}: {k}: ts {ts}, bevy {b}");
    }
}

#[test]
fn timer_matches_bevy() {
    let src = r#"
export class Cooldown extends Component { timer = Timer.fromSeconds(0.5, "Repeating") }
export class Wave extends Resource { timer = Timer.fromSeconds(0.5, "Repeating") }
const DELTAS = [0.2, 0.2, 0.2, 1.0];
export function tickAll(q: Query<[Mut<Cooldown>]>, wave: ResMut<Wave>, l: Local<{ n: u32; timer: Timer }>) {
  if (l.n >= DELTAS.length) return;
  if (l.n === 0) { l.timer.setDuration(0.5); l.timer.setMode("Repeating"); }
  const dt = DELTAS[l.n++];
  for (const [c] of q) {
    const just = c.timer.tick(dt).just_finished;
    wave.timer.tick(dt);
    l.timer.tick(dt);
    console.log("timer " + JSON.stringify({ c: c.timer, wave: wave.timer, local: l.timer, just }));
  }
}
app.insertResource(Wave.of({})).addSystems(Update, tickAll);
"#;
    let (_, mut app) = load("time-timer", &format!("{HEAD}{src}"));
    // The component's default comes through Rust's encoding, the resource's through JS's.
    spawn_ts(&mut app, "game::Cooldown", json!({}), ());
    updates(&mut app, 6);
    let ticks = logged(&mut app, "timer ");
    assert_eq!(ticks.len(), 4, "{:?}", messages(&app));
    let mut bevy = Timer::from_seconds(0.5, TimerMode::Repeating);
    for (i, (t, dt)) in ticks.iter().zip([0.2, 0.2, 0.2, 1.0]).enumerate() {
        bevy.tick(Duration::from_secs_f64(dt));
        for which in ["c", "wave", "local"] {
            assert_timer(&t[which], &bevy, &format!("tick {i} {which}"));
        }
        assert_eq!(t["just"], json!(bevy.just_finished()));
    }
    assert_eq!(ticks[3]["c"]["times_finished_this_tick"], json!(2));
    // What TS wrote is what Rust reads.
    let wave = read_res(&app, "game::Wave").unwrap();
    assert_timer(&wave["timer"], &bevy, "the resource read from Rust");
}

#[test]
fn once_timer_stays_finished() {
    let src = r#"
export function once(l: Local<{ n: u32; timer: Timer }>) {
  if (l.n === 0) l.timer.setDuration(1);
  if (l.n < 2) console.log("once " + JSON.stringify({ t: l.timer, just: l.timer.tick([1.5, 0.1][l.n++]).just_finished, done: l.timer.is_finished }));
}
app.addSystems(Update, once);
"#;
    let (_, mut app) = load("time-once", &format!("{HEAD}{src}"));
    updates(&mut app, 3);
    let t = logged(&mut app, "once ");
    assert_eq!((t[0]["just"].as_bool(), t[0]["t"]["elapsed"].as_f64()), (Some(true), Some(1.0)));
    assert_eq!((t[1]["just"].as_bool(), t[1]["done"].as_bool()), (Some(false), Some(true)));
}

#[test]
fn timer_methods_match_bevy() {
    let src = r#"
export function methods(l: Local<{ done: bool; timer: Timer }>) {
  if (l.done) return;
  l.done = true;
  const t = l.timer, out: unknown[] = [];
  const log = () => out.push(t.toJSON());
  t.setDuration(1); log();
  t.tick(0.25); log();
  t.pause(); t.tick(0.5); log();
  t.unpause(); t.tick(0.25); log();
  t.almostFinish(); log();
  t.finish(); log();
  t.tick(0.1); log();
  t.setMode("Repeating"); log();
  t.reset(); t.setElapsed(0.75); log();
  t.tick(0.5); log();
  t.pause(); t.tick(0.1); log();
  t.unpause(); t.setDuration(2); t.finish(); log();
  t.setMode("Once"); t.tick(0.1); log();
  console.log("methods " + JSON.stringify({ steps: out, remaining: t.remaining, fraction: t.fraction, fraction_remaining: t.fraction_remaining, elapsed_secs: t.elapsed_secs }));
}
app.addSystems(Update, methods);
"#;
    let (_, mut app) = load("time-methods", &format!("{HEAD}{src}"));
    updates(&mut app, 2);
    let got = logged(&mut app, "methods ");
    let steps = got[0]["steps"].as_array().unwrap();
    let s = |secs: f64| Duration::from_secs_f64(secs);
    let mut t = Timer::default();
    let mut expect = Vec::new();
    t.set_duration(s(1.0));
    expect.push(t.clone());
    t.tick(s(0.25));
    expect.push(t.clone());
    t.pause();
    t.tick(s(0.5));
    expect.push(t.clone());
    t.unpause();
    t.tick(s(0.25));
    expect.push(t.clone());
    t.almost_finish();
    expect.push(t.clone());
    t.finish();
    expect.push(t.clone());
    t.tick(s(0.1));
    expect.push(t.clone());
    t.set_mode(TimerMode::Repeating);
    expect.push(t.clone());
    t.reset();
    t.set_elapsed(s(0.75));
    expect.push(t.clone());
    t.tick(s(0.5));
    expect.push(t.clone());
    t.pause();
    t.tick(s(0.1));
    expect.push(t.clone());
    t.unpause();
    t.set_duration(s(2.0));
    t.finish();
    expect.push(t.clone());
    t.set_mode(TimerMode::Once);
    t.tick(s(0.1));
    expect.push(t.clone());
    assert_eq!(steps.len(), expect.len());
    for (i, (ts, b)) in steps.iter().zip(&expect).enumerate() {
        assert_timer(ts, b, &format!("step {i}"));
    }
    let close = |k: &str, v: f64| assert!((got[0][k].as_f64().unwrap() - v).abs() < 1e-6, "{k}: {} vs {v}", got[0][k]);
    close("remaining", t.remaining().as_secs_f64());
    close("fraction", t.fraction() as f64);
    close("fraction_remaining", t.fraction_remaining() as f64);
    close("elapsed_secs", t.elapsed_secs() as f64);
}

#[test]
fn stopwatch_pauses() {
    let src = r#"
export class Watch extends Resource { w = Stopwatch.new() }
export function watch(r: ResMut<Watch>, l: Local<{ n: u32; w: Stopwatch }>) {
  const step = l.n++;
  for (const w of [r.w, l.w]) {
    if (step === 1) w.pause();
    if (step === 2) w.unpause();
    if (step < 3) w.tick(1);
  }
  if (step === 2) console.log("watch " + JSON.stringify({ r: r.w, l: l.w, paused: l.w.is_paused }));
}
app.insertResource(Watch.of({ w: Stopwatch.new() })).addSystems(Update, watch);
"#;
    let (_, mut app) = load("time-stopwatch", &format!("{HEAD}{src}"));
    updates(&mut app, 4);
    let mut bevy = Stopwatch::new();
    bevy.tick(Duration::from_secs(1));
    bevy.pause();
    bevy.tick(Duration::from_secs(1));
    bevy.unpause();
    bevy.tick(Duration::from_secs(1));
    let got = &logged(&mut app, "watch ")[0];
    assert_eq!((got["r"]["elapsed"].as_f64(), got["r"]["paused"].as_bool()), (Some(2.0), Some(false)));
    assert_eq!(got["l"]["elapsed"].as_f64(), Some(bevy.elapsed_secs_f64()));
    assert_eq!(got["paused"], json!(false));
    let w = &read_res(&app, "game::Watch").unwrap()["w"];
    assert_eq!((w["elapsed"].as_f64(), w["paused"].as_bool()), (Some(2.0), Some(false)));
}

#[test]
fn timer_initialisers_are_constants() {
    let src = format!("{HEAD}const s = 2;\nexport class Cd extends Component {{ t = Timer.fromSeconds(s, \"Once\") }}\n");
    let errors = load_errors("time-init", &src);
    assert!(errors[0].message.contains("Timer.fromSeconds takes constant seconds"), "{:?}", errors[0]);
    assert_eq!(errors[0].location.as_ref().map(|l| l.line), Some(4), "{:?}", errors[0]);
}

#[test]
fn a_timer_named_like_the_accessors_state_ticks() {
    // `t` and `r` are names a generated accessor once kept its own state under.
    let src = r#"
export function slots(l: Local<{ r: u32; t: Timer }>) {
  if (l.r === 0) l.t.setDuration(1);
  if (l.r < 2) { l.t.tick(0.25); l.r++; console.log("slots " + JSON.stringify({ r: l.r, t: l.t })); }
}
app.addSystems(Update, slots);
"#;
    let (_, mut app) = load("time-slots", &format!("{HEAD}{src}"));
    updates(&mut app, 3);
    let t = logged(&mut app, "slots ");
    assert_eq!(t.len(), 2, "{:?}", messages(&app));
    assert_eq!((t[1]["r"].as_u64(), t[1]["t"]["elapsed"].as_f64()), (Some(2), Some(0.5)));
}

// ---------- Time conditions (stage 3) ----------

const CONDITIONS_HEAD: &str = r#"import { app, FixedUpdate, Update, not, onceAfterDelay, onceAfterRealDelay, onRealTimer, onTimer, paused, repeatingAfterDelay, repeatingAfterRealDelay, systems } from "bevy-ts";
"#;

/// The names Rust systems gated by Bevy's own conditions pushed this frame.
#[derive(Resource, Default)]
struct Fired(Vec<&'static str>);

/// Bevy's condition gating a Rust system that records `name` each time it runs.
fn bevy_gated<M>(app: &mut App, schedule: impl bevy::ecs::schedule::ScheduleLabel, name: &'static str, c: impl SystemCondition<M>) {
    app.add_systems(schedule, (move |mut f: ResMut<Fired>| f.0.push(name)).run_if(c));
}

/// Per frame, the sorted names TS logged as `fired <name>` and those Bevy's gating recorded.
fn fired_frames(app: &mut App, frames: usize, between: impl Fn(&mut App, usize)) -> (Vec<Vec<String>>, Vec<Vec<String>>) {
    let (mut ts, mut bevy) = (vec![], vec![]);
    let mut seen = 0;
    for frame in 0..frames {
        between(app, frame);
        app.update();
        bevy_ts::runtime::pump(app.world_mut());
        let all = messages(app);
        let mut t: Vec<String> = all[seen..].iter().filter_map(|m| m.strip_prefix("fired ")).map(String::from).collect();
        seen = all.len();
        let mut b: Vec<String> = app.world_mut().resource_mut::<Fired>().0.drain(..).map(String::from).collect();
        t.sort();
        b.sort();
        ts.push(t);
        bevy.push(b);
    }
    (ts, bevy)
}

#[test]
fn time_conditions_match_bevy() {
    use bevy::time::common_conditions::*;
    let src = r#"
export function a() { console.log("fired onTimer"); }
export function b() { console.log("fired onRealTimer"); }
export function c() { console.log("fired onceAfterDelay"); }
export function d() { console.log("fired onceAfterRealDelay"); }
export function e() { console.log("fired repeatingAfterDelay"); }
export function f() { console.log("fired repeatingAfterRealDelay"); }
export function g() { console.log("fired fixedOnTimer"); }
app.addSystems(Update, systems(a).runIf(onTimer(0.25)), systems(b).runIf(onRealTimer(0.3)), systems(c).runIf(onceAfterDelay(0.45)), systems(d).runIf(onceAfterRealDelay(0.2)),
  systems(e).runIf(repeatingAfterDelay(0.5)), systems(f).runIf(repeatingAfterRealDelay(0.35)));
app.addSystems(FixedUpdate, systems(g).runIf(onTimer(0.5)));
"#;
    let (_, mut app) = load("time-conditions", &format!("{CONDITIONS_HEAD}{src}"));
    app.insert_resource(TimeUpdateStrategy::ManualDuration(Duration::from_millis(100)));
    app.world_mut().resource_mut::<Time<Virtual>>().set_max_delta(Duration::from_secs(10));
    app.init_resource::<Fired>();
    let s = Duration::from_secs_f64;
    bevy_gated(&mut app, Update, "onTimer", on_timer(s(0.25)));
    bevy_gated(&mut app, Update, "onRealTimer", on_real_timer(s(0.3)));
    bevy_gated(&mut app, Update, "onceAfterDelay", once_after_delay(s(0.45)));
    bevy_gated(&mut app, Update, "onceAfterRealDelay", once_after_real_delay(s(0.2)));
    bevy_gated(&mut app, Update, "repeatingAfterDelay", repeating_after_delay(s(0.5)));
    bevy_gated(&mut app, Update, "repeatingAfterRealDelay", repeating_after_real_delay(s(0.35)));
    bevy_gated(&mut app, FixedUpdate, "fixedOnTimer", on_timer(s(0.5)));
    let (ts, bevy) = fired_frames(&mut app, 20, |_, _| {});
    assert_eq!(ts, bevy);
    let total = |n: &str| ts.iter().flatten().filter(|x| *x == n).count();
    assert!(total("onTimer") >= 7 && total("onceAfterDelay") == 1 && total("repeatingAfterDelay") > 10 && total("fixedOnTimer") >= 3, "{ts:?}");
}

#[test]
fn paused_follows_the_game_clock() {
    use bevy::time::common_conditions::paused;
    let src = r#"
export function a() { console.log("fired paused"); }
export function b() { console.log("fired running"); }
app.addSystems(Update, systems(a).runIf(paused()), systems(b).runIf(not(paused())));
"#;
    let (_, mut app) = load("time-paused", &format!("{CONDITIONS_HEAD}{src}"));
    app.init_resource::<Fired>();
    bevy_gated(&mut app, Update, "paused", paused);
    bevy_gated(&mut app, Update, "running", not(paused));
    let (ts, bevy) = fired_frames(&mut app, 6, |app, frame| {
        let mut t = app.world_mut().resource_mut::<Time<Virtual>>();
        match frame {
            2 => t.pause(),
            4 => t.unpause(),
            _ => {}
        }
    });
    assert_eq!(ts, bevy);
    assert_eq!(ts[3], ["paused"]);
    assert_eq!(ts[5], ["running"]);
}

#[test]
fn time_condition_survives_a_reload() {
    use bevy::time::common_conditions::on_timer;
    let body = |word: &str| format!("{CONDITIONS_HEAD}export function a() {{ console.log(\"fired onTimer\"); console.log(\"{word}\"); }}\napp.addSystems(Update, systems(a).runIf(onTimer(1)));\n");
    let (dir, mut app) = load("time-cond-reload", &body("one"));
    app.insert_resource(TimeUpdateStrategy::ManualDuration(Duration::from_millis(250)));
    app.init_resource::<Fired>();
    bevy_gated(&mut app, Update, "onTimer", on_timer(Duration::from_secs(1)));
    let (ts, bevy) = fired_frames(&mut app, 12, |app, frame| {
        // Mid-way to the second firing, a reload that keeps `onTimer(1)`.
        if frame == 6 {
            write(&dir, "game.ts", &body("two"));
            let r = reload(app, false);
            assert!(r.errors.is_empty(), "{:?}", r.errors);
        }
    });
    assert_eq!(ts, bevy, "the reload restarted the timer");
    assert!(ts.iter().flatten().count() >= 2, "{ts:?}");
    assert!(messages(&app).iter().any(|m| m == "two"), "the reload applied");
}

#[test]
fn a_bad_duration_fails_the_load() {
    for (arg, got) in [("-1", "-1"), ("NaN", "null"), ("Infinity", "null"), ("1e20", "1e+20")] {
        let src = format!("{CONDITIONS_HEAD}export function a() {{}}\napp.addSystems(Update, systems(a).runIf(onTimer({arg})));\n");
        let errors = load_errors(&format!("time-cond-bad{got}{}", arg.len()), &src);
        let want = format!("onTimer takes seconds, a finite number >= 0; got {got}");
        let e = errors.iter().find(|e| e.message.contains(&want)).unwrap_or_else(|| panic!("{arg}: {errors:?}"));
        let at = e.location.as_ref().unwrap();
        assert!(at.file.ends_with("game.ts") && at.line as usize == CONDITIONS_HEAD.lines().count() + 2, "{at:?}");
    }
}

#[test]
fn changed_seconds_start_a_fresh_timer() {
    use bevy::time::common_conditions::on_timer;
    let body = |secs: &str| format!("{CONDITIONS_HEAD}export function a() {{ console.log(\"fired {secs}\"); }}\napp.addSystems(Update, systems(a).runIf(onTimer({secs})));\n");
    let (dir, mut app) = load("time-cond-fresh", &body("1"));
    app.insert_resource(TimeUpdateStrategy::ManualDuration(Duration::from_millis(250)));
    app.init_resource::<Fired>();
    bevy_gated(&mut app, Update, "1", on_timer(Duration::from_secs(1)));
    let (ts, bevy) = fired_frames(&mut app, 16, |app, frame| {
        // Mid-way to the second firing, a reload to `onTimer(2)`, beside a Bevy timer of 2 s that
        // starts this frame.
        if frame == 6 {
            write(&dir, "game.ts", &body("2"));
            let r = reload(app, false);
            assert!(r.errors.is_empty(), "{:?}", r.errors);
            bevy_gated(app, Update, "2", on_timer(Duration::from_secs(2)));
        }
    });
    let only = |v: &[Vec<String>], n: &str| v.iter().map(|f| f.iter().filter(|x| *x == n).count()).collect::<Vec<_>>();
    assert_eq!(only(&ts[..6], "1"), only(&bevy[..6], "1"), "before the reload");
    assert_eq!(only(&ts, "2"), only(&bevy, "2"), "the reload kept the old timer");
    assert_eq!(only(&ts, "2").iter().sum::<usize>(), 1, "{ts:?}");
}
