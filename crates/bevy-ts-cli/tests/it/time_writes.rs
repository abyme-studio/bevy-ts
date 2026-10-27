//! Change script-time, stage 2: TS writes `ResMut<Time<Virtual>>` and `ResMut<Time<Fixed>>`,
//! the `VirtualTime` and `FixedTime` exports of `bevy-ts/time` (docs/time.md, Writing the clocks).
//! Each write is checked against the same call from a Rust system in a twin app.

use crate::common;

use bevy::prelude::*;
use bevy::time::{Fixed, TimeUpdateStrategy, Virtual};
use bevy_ts::host::HostParams;
use bevy_ts::runtime::ScriptClock;
use bevy_ts_cli::time::ScriptTimePlugin;
use common::*;
use std::time::Duration;

const HEAD: &str = r#"import { app, Fixed, FixedUpdate, Real, runOnce, systems, Time, Update, Virtual } from "bevy-ts";
import type { Local, Res, ResMut } from "bevy-ts";
"#;

const FRAME: Duration = Duration::from_millis(100);

/// A TS project with the time plugin, on fixed frame steps.
fn ts_app(name: &str, body: &str) -> (TestDir, App) {
    let dir = project(name, &[("game.ts", &format!("{HEAD}{body}"))]);
    let mut app = app(&dir.join("game.ts"));
    app.add_plugins(ScriptTimePlugin);
    app.insert_resource(TimeUpdateStrategy::ManualDuration(FRAME));
    let r = reload(&mut app, false);
    assert!(r.errors.is_empty(), "{:?}", r.errors);
    (dir, app)
}

/// The load errors of a TS project with the time plugin.
fn ts_errors(name: &str, body: &str) -> Vec<bevy_ts::backend::ScriptError> {
    let dir = project(name, &[("game.ts", &format!("{HEAD}{body}"))]);
    let mut app = app(&dir.join("game.ts"));
    app.add_plugins(ScriptTimePlugin);
    reload(&mut app, false).errors
}

/// Rust's twin: the same plugins' clocks and frame steps, `call` run once in `Update`.
fn twin<C: Send + Sync + 'static + Default>(call: impl Fn(&mut Time<C>) + Send + Sync + 'static) -> App {
    let mut app = App::new();
    app.add_plugins(MinimalPlugins).insert_resource(TimeUpdateStrategy::ManualDuration(FRAME));
    app.add_systems(Update, (move |mut t: ResMut<Time<C>>| call(&mut t)).run_if(run_once));
    app
}

/// Fixed steps run, counted by a Rust system in `FixedUpdate`.
#[derive(Resource, Default)]
struct Steps(u32);

fn count_steps(app: &mut App) {
    app.init_resource::<Steps>().add_systems(FixedUpdate, |mut s: ResMut<Steps>| s.0 += 1);
}

fn virtual_state(app: &App) -> String {
    let (t, g) = (app.world().resource::<Time<Virtual>>(), app.world().resource::<Time>());
    format!(
        "delta {} elapsed {} wrapped {} paused {} speed {} effective {} max {:?} wrap {:?} time.delta {}",
        t.delta_secs_f64(),
        t.elapsed_secs_f64(),
        t.elapsed_secs_wrapped_f64(),
        t.is_paused(),
        t.relative_speed_f64(),
        t.effective_speed_f64(),
        t.max_delta(),
        t.wrap_period(),
        g.delta_secs_f64()
    )
}

fn fixed_state(app: &App) -> String {
    let t = app.world().resource::<Time<Fixed>>();
    format!("steps {} timestep {:?} overstep {:?} elapsed {:?} wrap {:?}", app.world().resource::<Steps>().0, t.timestep(), t.overstep(), t.elapsed(), t.wrap_period())
}

#[test]
fn virtual_writes_match_bevy() {
    type Call = fn(&mut Time<Virtual>);
    let cases: [(&str, Call); 7] = [
        ("time.pause()", |t| t.pause()),
        ("time.pause(); time.unpause()", |t| {
            t.pause();
            t.unpause()
        }),
        ("time.toggle()", |t| t.toggle()),
        ("time.setRelativeSpeed(0.5)", |t| t.set_relative_speed(0.5)),
        ("time.setRelativeSpeedF64(2)", |t| t.set_relative_speed_f64(2.0)),
        ("time.setMaxDelta(0.05)", |t| t.set_max_delta(Duration::from_millis(50))),
        ("time.setWrapPeriod(0.25)", |t| t.set_wrap_period(Duration::from_millis(250))),
    ];
    for (i, (ts, rust)) in cases.into_iter().enumerate() {
        let (_, mut app) = ts_app(&format!("time-writes-virtual{i}"), &format!("export function w(time: ResMut<Time<Virtual>>) {{ {ts}; }}\napp.addSystems(Update, systems(w).runIf(runOnce()));\n"));
        let mut bevy = twin::<Virtual>(rust);
        for frame in 0..5 {
            app.update();
            bevy.update();
            assert_eq!(virtual_state(&app), virtual_state(&bevy), "{ts}, frame {frame}");
        }
    }
}

#[test]
fn fixed_writes_match_bevy() {
    type Call = fn(&mut Time<Fixed>);
    let cases: [(&str, Call); 5] = [
        ("time.setTimestep(0.05)", |t| t.set_timestep(Duration::from_millis(50))),
        ("time.setTimestepSeconds(0.025)", |t| t.set_timestep_seconds(0.025)),
        ("time.setTimestepHz(10)", |t| t.set_timestep_hz(10.0)),
        ("time.discardOverstep(0.01)", |t| t.discard_overstep(Duration::from_millis(10))),
        ("time.setWrapPeriod(1)", |t| t.set_wrap_period(Duration::from_secs(1))),
    ];
    for (i, (ts, rust)) in cases.into_iter().enumerate() {
        let (_, mut app) = ts_app(&format!("time-writes-fixed{i}"), &format!("export function w(time: ResMut<Time<Fixed>>) {{ {ts}; }}\napp.addSystems(Update, systems(w).runIf(runOnce()));\n"));
        count_steps(&mut app);
        let mut bevy = twin::<Fixed>(rust);
        count_steps(&mut bevy);
        for frame in 0..6 {
            app.update();
            bevy.update();
            assert_eq!(fixed_state(&app), fixed_state(&bevy), "{ts}, frame {frame}");
        }
    }
}

#[test]
fn a_write_reads_back_in_the_same_run() {
    let body = r#"
export function w(time: ResMut<Time<Virtual>>, fixed: ResMut<Time<Fixed>>) {
  const before = time.is_paused;
  time.pause();
  fixed.setTimestepHz(10);
  time.setRelativeSpeed(0.5);
  console.log(`read ${before} ${time.is_paused} ${fixed.timestep} ${time.relative_speed}`);
}
app.addSystems(Update, systems(w).runIf(runOnce()));
"#;
    let (_, mut app) = ts_app("time-writes-readback", body);
    updates(&mut app, 2);
    let m = messages(&app);
    assert!(m.iter().any(|l| l == "read false true 0.1 0.5"), "{m:?}");
}

#[test]
fn bad_clock_writes_throw() {
    let calls = [
        ("time.setRelativeSpeed(-1)", "VirtualTime.setRelativeSpeed: tried to go back in time"),
        ("time.setRelativeSpeedF64(Infinity)", "VirtualTime.setRelativeSpeedF64: tried to go infinitely fast"),
        ("time.setMaxDelta(0)", "VirtualTime.setMaxDelta: tried to set max delta to zero"),
        ("time.setMaxDelta(-1)", "VirtualTime.setMaxDelta: takes seconds, a finite number >= 0; got -1"),
        ("time.setWrapPeriod(0)", "VirtualTime.setWrapPeriod: division by zero"),
        ("fixed.setTimestep(0)", "FixedTime.setTimestep: attempted to set fixed timestep to zero"),
        ("fixed.setTimestepSeconds(-1)", "FixedTime.setTimestepSeconds: seconds less than or equal to zero"),
        ("fixed.setTimestepSeconds(Infinity)", "FixedTime.setTimestepSeconds: seconds is infinite"),
        ("fixed.setTimestepSeconds(1e300)", "FixedTime.setTimestepSeconds: seconds is too large for a timestep; got 1e300"),
        ("fixed.setTimestepHz(-5)", "FixedTime.setTimestepHz: Hz less than or equal to zero"),
        ("fixed.setTimestepHz(Infinity)", "FixedTime.setTimestepHz: Hz is infinite"),
        ("fixed.setTimestepHz(0)", "FixedTime.setTimestepHz: seconds is infinite"),
        ("fixed.discardOverstep(NaN)", "FixedTime.discardOverstep: takes seconds, a finite number >= 0; got NaN"),
        ("fixed.setWrapPeriod(0)", "FixedTime.setWrapPeriod: division by zero"),
    ];
    let tries: String = calls.iter().map(|(c, _)| format!("  try {{ {c}; console.log(\"no throw\"); }} catch (e) {{ console.log(`threw ${{(e as Error).message}}`); }}\n")).collect();
    let body = format!("export function w(time: ResMut<Time<Virtual>>, fixed: ResMut<Time<Fixed>>) {{\n{tries}}}\napp.addSystems(Update, systems(w).runIf(runOnce()));\n");
    let (_, mut app) = ts_app("time-writes-bad", &body);
    let fixed = *app.world().resource::<Time<Fixed>>();
    updates(&mut app, 2);
    let threw: Vec<String> = messages(&app).iter().filter_map(|m| m.strip_prefix("threw ")).map(String::from).collect();
    let want: Vec<String> = calls.iter().map(|(_, w)| w.to_string()).collect();
    assert_eq!(threw, want, "{:?}", messages(&app));
    // Every clock as it was: the checks ran before Bevy's call.
    let t = app.world().resource::<Time<Virtual>>();
    assert_eq!((t.relative_speed_f64(), t.max_delta(), t.wrap_period(), t.is_paused()), (1.0, Duration::from_millis(250), Duration::from_secs(3600), false));
    let f = app.world().resource::<Time<Fixed>>();
    assert_eq!((f.timestep(), f.wrap_period()), (fixed.timestep(), fixed.wrap_period()));
}

#[test]
fn ts_pause_survives_an_editor_step() {
    let body = r#"
export function w(time: ResMut<Time<Virtual>>, l: Local<{ runs: u32 }>) {
  if (++l.runs === 3) time.pause();
  console.log(`run ${l.runs}`);
}
app.addSystems(Update, w);
"#;
    let (_, mut app) = ts_app("time-writes-step", body);
    updates(&mut app, 2);
    // The editor pauses: the stages hold; a step runs one frame, whose run pauses the game clock.
    app.world_mut().resource_mut::<ScriptClock>().paused = true;
    updates(&mut app, 2);
    app.world_mut().resource_mut::<ScriptClock>().step_frames = 1;
    app.update();
    assert!(app.world().resource::<Time<Virtual>>().is_paused(), "the stepped frame's pause");
    updates(&mut app, 2);
    assert!(app.world().resource::<Time<Virtual>>().is_paused(), "held frames keep it");
    // Resume undoes the editor's pause only.
    app.world_mut().resource_mut::<ScriptClock>().paused = false;
    updates(&mut app, 3);
    assert!(app.world().resource::<Time<Virtual>>().is_paused(), "the TS pause holds after resume");
    bevy_ts::runtime::pump(app.world_mut());
    let runs = messages(&app).iter().filter(|m| m.starts_with("run ")).count();
    assert_eq!(runs, 6, "two runs, one step, three after resume: {:?}", messages(&app));
}

#[test]
fn unwritable_clocks_fail_the_load() {
    for (name, spelling) in [("time-writes-generic", "ResMut<Time>"), ("time-writes-real", "ResMut<Time<Real>>")] {
        let errors = ts_errors(name, &format!("export function w(t: {spelling}) {{}}\napp.addSystems(Update, w);\n"));
        let e = errors.iter().find(|e| e.message.contains(&format!("{spelling} is not allowed"))).unwrap_or_else(|| panic!("{spelling}: {errors:?}"));
        assert!(e.message.contains("ResMut<Time<Virtual>>") && e.message.contains("ResMut<Time<Fixed>>"), "{e:?}");
        assert_eq!(e.location.as_ref().map(|l| l.line as usize), Some(HEAD.lines().count() + 1), "{e:?}");
    }
}

#[test]
fn clock_reads_make_no_host_call() {
    let body = r#"
export function r(time: Res<Time<Virtual>>, fixed: Res<Time<Fixed>>, real: Res<Time<Real>>) {
  console.log(`read ${time.is_paused} ${time.relative_speed} ${fixed.timestep} ${real.delta_secs_f64 >= 0}`);
}
app.addSystems(Update, r);
"#;
    let (_, mut app) = ts_app("time-writes-reads", body);
    app.update();
    let before = app.world().resource::<HostParams>().calls;
    updates(&mut app, 3);
    assert_eq!(app.world().resource::<HostParams>().calls, before, "a Res clock is the per-run view");
    assert!(messages(&app).iter().any(|l| l == "read false 1 0.015625 true"), "{:?}", messages(&app));
}
