//! What one exported-param call costs end to end (writable-native-params): a TS system making
//! 10,000 calls per run, against the same loop with no call. The difference over the calls is one
//! call: the JS glue, the V8 fast op and the Rust half. Three arms: a read (`Res` + `Query<&T>`),
//! a write (`ResMut` + `Query<&mut T>`), and a write that also queues one `Commands` insert.
//! Ignored by default; run it in release:
//! `cargo test --release -p bevy-ts-cli --test it native_params_bench -- --ignored --nocapture`.

use crate::common;

use bevy::ecs::system::SystemParam;
use bevy::prelude::*;
use bevy_ts::host::HostParams;
use bevy_ts_native::prelude::*;
use common::*;
use std::time::Instant;

const CALLS: u64 = 10_000;
const FRAMES: usize = 200;

#[derive(Resource)]
pub struct Ground(pub f32);

#[derive(Component)]
pub struct Pos(pub f32);

#[derive(Component)]
pub struct Tag;

#[derive(SystemParam)]
pub struct Probe<'w, 's> {
    ground: Res<'w, Ground>,
    q: Query<'w, 's, &'static Pos>,
}

#[export(ts = "Probe", module = "bevy-ts/bench")]
impl Probe<'_, '_> {
    pub fn height(&self, x: f32) -> f32 {
        self.q.iter().next().map_or(0.0, |p| p.0) + x - self.ground.0
    }
}

#[derive(SystemParam)]
pub struct Edit<'w, 's> {
    ground: ResMut<'w, Ground>,
    q: Query<'w, 's, (Entity, &'static mut Pos)>,
    commands: Commands<'w, 's>,
}

#[export(ts = "Edit", module = "bevy-ts/bench")]
impl Edit<'_, '_> {
    pub fn nudge(&mut self, x: f32) -> f32 {
        self.ground.0 += 1e-9;
        self.q.iter_mut().next().map_or(0.0, |(_, mut p)| {
            p.0 += 1e-9;
            p.0
        }) + x
    }
    pub fn nudge_and_tag(&mut self, x: f32) -> f32 {
        let v = self.nudge(x);
        if let Some((e, _)) = self.q.iter().next() {
            self.commands.entity(e).insert(Tag);
        }
        v
    }
}

/// A TS module making 10,000 calls of `call` (an expression over `p` and `i`) per run.
fn calling(param: &str, call: &str) -> String {
    format!(
        r#"import {{ app, Update }} from "bevy-ts";
import type {{ {param} }} from "bevy-ts/bench";
export function calls(p: {param}) {{
  let s = 0;
  for (let i = 0; i < 10000; i++) s += {call};
  if (s === -1) console.log("never");
}}
app.addSystems(Update, calls);
"#
    )
}

const LOOPING: &str = r#"import { app, Update } from "bevy-ts";
import type { Probe } from "bevy-ts/bench";
export function calls(p: Probe) {
  let s = 0;
  for (let i = 0; i < 10000; i++) s += i * 0.5;
  if (s === -1) console.log("never");
}
app.addSystems(Update, calls);
"#;

/// Microseconds per frame, median of `FRAMES`, and the host calls the frames made.
fn frames(name: &str, src: &str) -> (f64, u64) {
    let dir = project(name, &[("game.ts", src)]);
    let mut app = app(&dir.join("game.ts"));
    app.ts_param::<Probe<'static, 'static>>().ts_param::<Edit<'static, 'static>>();
    app.insert_resource(Ground(2.0));
    app.world_mut().spawn(Pos(1.0));
    let r = reload(&mut app, false);
    assert!(r.errors.is_empty(), "{:?}", r.errors);
    for _ in 0..20 {
        app.update();
    }
    let before = app.world().resource::<HostParams>().calls;
    let mut us: Vec<f64> = (0..FRAMES)
        .map(|_| {
            let t = Instant::now();
            app.update();
            t.elapsed().as_secs_f64() * 1e6
        })
        .collect();
    let calls = app.world().resource::<HostParams>().calls - before;
    us.sort_by(f64::total_cmp);
    (us[FRAMES / 2], calls)
}

#[test]
#[ignore]
fn one_call_end_to_end() {
    let (base, none) = frames("native-bench-loop", LOOPING);
    assert_eq!(none, 0);
    for (arm, param, call) in [("read", "Probe", "p.height(i)"), ("write", "Edit", "p.nudge(i)"), ("write+commands", "Edit", "p.nudgeAndTag(i)")] {
        let (with, calls) = frames(&format!("native-bench-{}", arm.replace('+', "-")), &calling(param, call));
        assert_eq!(calls, CALLS * FRAMES as u64);
        let per_call_ns = (with - base) * 1e3 / CALLS as f64;
        println!("{arm}: frame without calls {base:.0} us, with {CALLS} calls {with:.0} us: {per_call_ns:.1} ns per call");
    }
}
