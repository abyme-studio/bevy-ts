//! What TS systems as Bevy systems cost (ts-systems-as-bevy-systems task 6), against the one
//! exclusive stage before it: the same file runs on both. Ignored by default; run it in release:
//! `cargo test --release -p bevy-ts-cli --test it stage_bench -- --ignored --nocapture --test-threads=1`.
//! Each line is `BENCH <name> <value>`. `STAGE_BENCH_ARM` runs one stage arm and `STAGE_BENCH_FRAMES`
//! sets its frames, for a profiler to sample.

use crate::common;

use bevy::ecs::schedule::ScheduleLabel;
use bevy::prelude::*;
use common::*;
use std::time::Instant;

const SYSTEMS: usize = 20;
const ENTITIES: usize = 1_000;
const FRAMES: usize = 2_000;

/// A module of `n` Update systems with `params` and `body`, after a Startup spawn of `ENTITIES`.
fn module(n: usize, params: &str, body: &str, extra: &str) -> String {
    let mut s = String::from(
        "import { app, Component, Startup, u32, Update } from \"bevy-ts\";\nimport type { Commands, Mut, Query } from \"bevy-ts\";\nexport class Vel extends Component { static path = \"game::Vel\"; declare x: u32 }\n",
    );
    s.push_str(&format!("export function spawn(c: Commands) {{ for (let i = 0; i < {ENTITIES}; i++) c.spawn(Vel.of({{ x: 0 }})); }}\n"));
    let names: Vec<String> = (0..n).map(|i| format!("s{i}")).collect();
    for name in &names {
        s.push_str(&format!("export function {name}({params}) {{ {body} }}\n"));
    }
    s.push_str(&format!("app.addSystems(Startup, spawn){};\n{extra}", if n == 0 { String::new() } else { format!(".addSystems(Update, {})", names.join(", ")) }));
    s
}

/// µs per frame over `FRAMES`, after a warm-up.
fn per_frame(app: &mut App) -> f64 {
    let frames = std::env::var("STAGE_BENCH_FRAMES").ok().and_then(|f| f.parse().ok()).unwrap_or(FRAMES);
    updates(app, 100);
    let t = Instant::now();
    updates(app, frames);
    t.elapsed().as_secs_f64() * 1e6 / frames as f64
}

#[test]
#[ignore = "a benchmark: run in release with --ignored"]
fn stage_bench() {
    let arms = [
        ("empty-stage", module(0, "", "", "")),
        ("20-no-params", module(SYSTEMS, "", "", "")),
        ("20-commands", module(SYSTEMS, "c: Commands", "", "")),
        ("20-query", module(SYSTEMS, "q: Query<[Mut<Vel>]>", "q.each((v) => { v.x += 1; });", "")),
    ];
    let only = std::env::var("STAGE_BENCH_ARM").ok();
    for (name, src) in arms.into_iter().filter(|(n, _)| only.as_deref().is_none_or(|o| o == *n)) {
        let (_, mut app) = load(&format!("stage-bench-{name}"), &src);
        println!("BENCH stage/{name} {:.2}", per_frame(&mut app));
    }
}

/// A reload of a 200-system Update: a body-only change, and one system's declaration changed.
#[test]
#[ignore = "a benchmark: run in release with --ignored"]
fn rebuild_bench() {
    const N: usize = 200;
    let src = |param0: &str, mark: u32| {
        let mut s = module(N, "q: Query<[Vel]>", "", &format!("// {mark}\n"));
        // s0's declaration: `Query<[Vel]>` or `param0`.
        s = s.replacen("export function s0(q: Query<[Vel]>)", &format!("export function s0({param0})"), 1);
        s
    };
    let (dir, mut app) = load("rebuild-bench", &src("q: Query<[Vel]>", 0));
    updates(&mut app, 10);
    let (mut body, mut decl) = (Vec::new(), Vec::new());
    let mut mark = 0;
    let mut timed = |app: &mut App, param0: &str| {
        mark += 1;
        write(&dir, "game.ts", &src(param0, mark));
        let t = Instant::now();
        reload(app, false);
        app.update();
        t.elapsed().as_secs_f64() * 1e3
    };
    for i in 0..7 {
        // s0's declaration flips, then only the comment moves.
        let param0 = if i % 2 == 0 { "q: Query<[Mut<Vel>]>" } else { "q: Query<[Vel]>" };
        decl.push(timed(&mut app, param0));
        body.push(timed(&mut app, param0));
        updates(&mut app, 5);
    }
    let median = |v: &mut Vec<f64>| {
        v.sort_by(f64::total_cmp);
        v[v.len() / 2]
    };
    println!("BENCH rebuild/body-only-ms {:.2}", median(&mut body));
    println!("BENCH rebuild/one-decl-ms {:.2}", median(&mut decl));
}

#[derive(ScheduleLabel, Debug, Clone, PartialEq, Eq, Hash)]
struct Barriers;

#[derive(Default)]
struct Counter(u64);

fn deferring(mut c: NonSendMut<Counter>, mut commands: Commands) {
    c.0 += 1;
    if c.0 == u64::MAX {
        commands.spawn_empty();
    }
}

/// What a sync point costs Bevy alone: 20 chained main-thread systems with `Commands`, with a sync
/// point after each (as TS systems are built) and with one at the end (the fewest).
#[test]
#[ignore = "a benchmark: run in release with --ignored"]
fn barrier_bench() {
    for (name, each) in [("sync-after-each", true), ("one-sync-at-end", false)] {
        let mut world = World::new();
        world.insert_non_send(Counter::default());
        let mut schedule = Schedule::new(Barriers);
        let systems = (deferring, deferring, deferring, deferring, deferring, deferring, deferring, deferring, deferring, deferring);
        let systems2 = (deferring, deferring, deferring, deferring, deferring, deferring, deferring, deferring, deferring, deferring);
        if each {
            schedule.add_systems((systems.chain(), systems2.chain()).chain());
        } else {
            schedule.add_systems((systems.chain_ignore_deferred(), systems2.chain_ignore_deferred()).chain_ignore_deferred());
        }
        for _ in 0..100 {
            schedule.run(&mut world);
        }
        let t = Instant::now();
        for _ in 0..20_000 {
            schedule.run(&mut world);
        }
        println!("BENCH barrier/{name} {:.2}", t.elapsed().as_secs_f64() * 1e6 / 20_000.0);
    }
}
