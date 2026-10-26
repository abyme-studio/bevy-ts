//! Bench 3.1 (ecs-schedules, stage 3): what a TS predicate condition costs per frame, against a
//! Rust condition. It gated building predicates, so it measures a stand-in: a TS system reading
//! `Res<Score>` and computing a `boolean`, run through `run_one` as a predicate now is: the call,
//! the view setup and the param. The cost `docs/scripts.md` quotes for `runIf(pred)` is this one. Its members are skipped by a Rust condition, which
//! costs no call, so a frame's extra time over the Rust arm is the predicate's evaluations alone.
//!
//! Arms, for 1 and 8 gated systems: `rust` (the members behind `inState`, no call), `collective`
//! (one predicate call per frame, as `systems(...).runIf(pred)`), `distributive` (one call per
//! member, as `distributiveRunIf(pred)`), and 64 calls for the slope of one evaluation. Ignored by
//! default; run it in release:
//! `cargo static test --release -p bevy-ts-cli --test it predicate_bench -- --ignored --nocapture`.

use crate::common;

use common::*;
use std::sync::mpsc;
use std::time::Instant;

const FRAMES: usize = 500;

/// A program with `members` systems gated off by `inState(Game.Playing)` (the game is in Menu)
/// and `preds` predicate-shaped systems run every frame; with `busy`, one more TS system runs every
/// frame, so the stage already enters JS and the arm measures the predicate's calls alone.
fn program(members: usize, preds: usize, busy: bool) -> String {
    let mut s = String::from(
        r#"import { app, inState, Resource, States, systems, u32, Update } from "bevy-ts";
import type { Res } from "bevy-ts";
export class Score extends Resource { static path = "game::Score"; declare value: u32 }
export class Game extends States("Menu", "Playing") { static path = "game::Game" }
"#,
    );
    for i in 0..members {
        s += &format!("export function member{i}() {{ console.log('never'); }}\n");
    }
    for i in 0..preds {
        s += &format!("export function pred{i}(s: Res<Score>): boolean {{ return s.value >= 100; }}\n");
    }
    if busy {
        s += "export function busy(s: Res<Score>) { if (s.value === 12345) console.log('never'); }\n";
    }
    let members: Vec<String> = (0..members).map(|i| format!("member{i}")).collect();
    let mut preds: Vec<String> = (0..preds).map(|i| format!("pred{i}")).collect();
    if busy {
        preds.push("busy".into());
    }
    s += &format!(
        "app.initState(Game.Menu).insertResource(Score.of({{ value: 0 }})).addSystems(Update, systems({}).runIf(inState(Game.Playing)){});\n",
        members.join(", "),
        if preds.is_empty() { String::new() } else { format!(", {}", preds.join(", ")) }
    );
    s
}

/// Each arm's median microseconds per frame over `FRAMES` and its calls into JS per frame. The
/// arms' frames are interleaved, one frame of each in turn, so load drifting over the run falls on
/// every arm alike. Each arm's app lives on its own thread: a V8 isolate stays on the thread that
/// made it.
fn frames(arms: &[(String, String)]) -> Vec<(f64, f64)> {
    let workers: Vec<_> = arms
        .iter()
        .map(|(name, src)| {
            let (name, src) = (name.clone(), src.clone());
            let (go, step) = mpsc::channel::<()>();
            let (done, took) = mpsc::channel::<f64>();
            let handle = std::thread::spawn(move || {
                let (_, mut app) = load(&name, &src);
                for _ in 0..50 {
                    app.update();
                }
                let before = calls(&app);
                done.send(0.0).unwrap();
                while step.recv().is_ok() {
                    let t = Instant::now();
                    app.update();
                    done.send(t.elapsed().as_secs_f64() * 1e6).unwrap();
                }
                (calls(&app) - before) as f64 / FRAMES as f64
            });
            took.recv().unwrap();
            (go, took, handle)
        })
        .collect();
    let mut us = vec![Vec::with_capacity(FRAMES); workers.len()];
    for _ in 0..FRAMES {
        for ((go, took, _), us) in workers.iter().zip(us.iter_mut()) {
            go.send(()).unwrap();
            us.push(took.recv().unwrap());
        }
    }
    workers
        .into_iter()
        .zip(us.iter_mut())
        .map(|((go, _, handle), us)| {
            drop(go);
            us.sort_by(f64::total_cmp);
            (us[FRAMES / 2], handle.join().unwrap())
        })
        .collect()
}

#[test]
#[ignore]
fn predicate_against_a_rust_condition() {
    // (label, gated systems, predicate calls per frame, busy). 64 calls on 8 systems gives the
    // slope of one evaluation over noise the small arms sit in. A quiet stage enters JS only for
    // the predicate; a busy one already runs a TS system, as a game's Update does.
    let arms = [
        ("quiet stage, 1 systems, rust", 1, 0, false),
        ("quiet stage, 1 systems, collective", 1, 1, false),
        ("quiet stage, 8 systems, rust", 8, 0, false),
        ("quiet stage, 8 systems, collective", 8, 1, false),
        ("quiet stage, 8 systems, distributive", 8, 8, false),
        ("quiet stage, 8 systems, 64 calls", 8, 64, false),
        ("busy stage, 8 systems, rust", 8, 0, true),
        ("busy stage, 8 systems, collective", 8, 1, true),
        ("busy stage, 8 systems, distributive", 8, 8, true),
        ("busy stage, 8 systems, 64 calls", 8, 64, true),
    ];
    let srcs: Vec<(String, String)> = arms.iter().enumerate().map(|(i, (_, m, p, b))| (format!("pred-bench-{i}"), program(*m, *p, *b))).collect();
    let got = frames(&srcs);
    let base = |m: usize, b: bool| got[arms.iter().position(|a| a.1 == m && a.2 == 0 && a.3 == b).unwrap()];
    for ((label, members, _, busy), (us, calls)) in arms.iter().zip(&got) {
        let (base_us, base_calls) = base(*members, *busy);
        let (extra, evals) = (us - base_us, calls - base_calls);
        let per = if evals > 0.0 { format!(", {:.2} us per evaluation", extra / evals) } else { String::new() };
        println!("{label}: {us:.1} us per frame, {evals:.1} predicate calls, {extra:+.1} us{per}");
    }
}
