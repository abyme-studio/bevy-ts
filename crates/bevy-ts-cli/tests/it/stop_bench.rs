//! What a Stop costs (play-stop task 6.1, stop-incremental task 6.1): the levels bench's level at
//! 1,000 and 10,000 instances (2,000 and 20,000 entities), in the app `bevy-ts dev --viewport`
//! builds, rendering on. Four arms, three Stops each: the full remount (the level made dirty, so
//! it is not kept), and the restore with nothing touched, 100 enemies moved, and every one moved.
//! Each Stop end to end through `viewport_mode`, then its parts called one by one as Stop calls
//! them: the plan (scan, file hashes), `OnExit(Play)`, the rebuild in its steps (reset, a fresh
//! isolate, load, sections, apply, startups), the restore and the settle. And the kept tree's
//! memory.
//! Ignored by default; run it in release:
//! `cargo static test --release -p bevy-ts-cli --test it stop_bench:: -- --ignored --nocapture`.
#![cfg(target_os = "macos")]

use crate::common;

use bevy::prelude::*;
use bevy::remote::{RemoteMethodSystemId, RemoteMethods, RemotePlugin};
use bevy::window::ExitCondition;
use bevy::winit::WinitPlugin;
use bevy_ts::EngineMode;
use bevy_ts::generation::DevSource;
use bevy_ts_cli::compile::Compiler;
use bevy_ts_levels::engine::{Levels, LevelsPlugin};
use bevy_ts_v8::V8Options;
use common::{project, reload};
use serde_json::{Value, json};
use std::path::Path;
use std::sync::Arc;
use std::time::Instant;

const ENEMIES: &str = r#"import { app, Component, SceneComponent, type Commands, Startup, type f32, type u8 } from "bevy-ts";
import { Transform } from "bevy-ts/bevy";
export class Health extends Component { hp: f32 = 100; max: f32 = 100 }
export class Bullet extends Component {}
export class Grunt extends SceneComponent {
  team: u8 = 0;
  static scene({ hp = 100 }: { hp?: f32 }) {
    return (
      <entity name="grunt">
        <Health initialHp={hp} max={hp} />
        <entity key="eye"><Transform translation={[0, 0.6, 0.5]} /></entity>
      </entity>
    );
  }
}
export function fire(commands: Commands) { commands.spawn(Bullet); }
app.level("levels/arena.bsn");
app.addSystems(Startup, fire);
"#;

fn level(n: usize) -> String {
    (0..n)
        .map(|i| format!("#G{i}\n@enemies::Grunt {{ @hp: {}.0 }}\nTransform {{ translation: Vec3 {{ x: {}.0, z: -4.0 }} }}\n", 100 + i % 50, i % 100))
        .collect::<Vec<_>>()
        .join("--\n")
}

fn bench_app(dir: &Path) -> App {
    let assets = dir.join("assets").canonicalize().unwrap();
    let mut app = App::new();
    app.add_plugins(
        DefaultPlugins
            .build()
            .disable::<WinitPlugin>()
            .set(WindowPlugin { primary_window: None, exit_condition: ExitCondition::DontExit, ..default() })
            .set(AssetPlugin { file_path: assets.to_string_lossy().into_owned(), ..default() }),
    )
    .add_plugins(RemotePlugin::default())
    .add_plugins(bevy_ts_viewport::ViewportPlugin { windowed: false })
    .add_plugins(bevy_ts::BevyTsPlugin::new(bevy_ts_v8::factory(V8Options::default())).dev(true))
    .add_plugins(bevy_ts_tsx::TsxPlugin)
    .add_plugins(LevelsPlugin);
    app.world_mut().resource_mut::<Levels>().asset_root = assets;
    app.insert_resource(DevSource(Arc::new(Compiler::new(dir.join("src/enemies.tsx"), true).unwrap())));
    bevy_ts_remote::add_methods(&mut app);
    app.insert_state(EngineMode::Edit);
    app.world_mut().resource_mut::<bevy_ts::generation::Generations>().started = false;
    app.world_mut().resource_mut::<bevy_ts::runtime::ScriptClock>().paused = true;
    app.finish();
    app.cleanup();
    app
}

fn mode(app: &mut App, mode: &str) -> Value {
    let id = match app.world().resource::<RemoteMethods>().get("bevy-ts/viewport_mode") {
        Some(RemoteMethodSystemId::Instant(id)) => *id,
        _ => panic!("no viewport_mode"),
    };
    app.world_mut().run_system_with(id, Some(json!({ "mode": mode }))).unwrap().unwrap()
}

fn ms(t: Instant) -> f64 {
    t.elapsed().as_secs_f64() * 1000.0
}

fn median(mut v: Vec<f64>) -> f64 {
    v.sort_by(f64::total_cmp);
    v[v.len() / 2]
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum Arm {
    Full,
    Touched(usize),
}

const ROOT: &str = "level:levels/arena.bsn";

fn grunt(app: &App, i: usize) -> Entity {
    app.world().resource::<bevy_ts_tsx::TsxTree>().entity(bevy_ts_tsx::desc::node_id(ROOT, &[format!("G{i}")])).unwrap()
}

/// What the play does before the Stop: moves `k` enemies, or makes the level wait to compose so
/// the Stop mounts it again.
fn play(app: &mut App, n: usize, arm: Arm) {
    mode(app, "play");
    app.update();
    match arm {
        Arm::Full => app.world_mut().resource_mut::<Levels>().roots.get_mut("levels/arena.bsn").unwrap().dirty = true,
        Arm::Touched(k) => {
            for i in (0..n).step_by((n / k.max(1)).max(1)).take(k) {
                let e = grunt(app, i);
                app.world_mut().get_mut::<Transform>(e).unwrap().translation.y += 1.0;
            }
        }
    }
}

/// Bytes the kept tree holds beyond the components it shares with the template cache.
fn kept_bytes(app: &App) -> usize {
    fn walk(nodes: &[bevy_ts_tsx::reconcile::RNode]) -> usize {
        nodes
            .iter()
            .map(|n| {
                let strings = |v: &[String]| v.iter().map(|s| s.capacity() + std::mem::size_of::<String>()).sum::<usize>();
                std::mem::size_of::<bevy_ts_tsx::reconcile::RNode>() + strings(&n.path) + strings(&n.types) + n.source.len() * std::mem::size_of::<bevy_ts::backend::SourceLocation>() + n.source.iter().map(|l| l.file.capacity()).sum::<usize>() + walk(&n.children)
            })
            .sum()
    }
    app.world().resource::<bevy_ts_tsx::TsxTree>().roots[ROOT].resolved.as_deref().map_or(0, walk)
}

/// One size and arm: three end-to-end Stops, then three split ones.
fn arm(app: &mut App, n: usize, arm: Arm) -> String {
    let mut total = vec![];
    let mut respawned = vec![];
    for _ in 0..3 {
        play(app, n, arm);
        let t = Instant::now();
        assert_eq!(mode(app, "edit")["started"], false);
        total.push(ms(t));
        respawned.push(app.world().resource::<bevy_ts_tsx::TsxTree>().last.spawned);
        assert!(app.world().resource::<bevy_ts_tsx::TsxTree>().entities.len() >= n * 2, "the level stands when Stop answers");
        app.update();
    }
    // The parts, as `stop` calls them.
    let (mut plan, mut exit, mut rebuild, mut restore, mut settle) = (vec![], vec![], vec![], vec![], vec![]);
    let mut steps: [Vec<f64>; 6] = Default::default();
    for _ in 0..3 {
        play(app, n, arm);
        let open = bevy_ts_levels::engine::open_set(app.world());
        let world = app.world_mut();
        let t = Instant::now();
        let p = bevy_ts_levels::engine::plan_stop(world);
        world.insert_resource(p);
        plan.push(ms(t));
        let t = Instant::now();
        world.resource_mut::<NextState<EngineMode>>().set(EngineMode::Edit);
        world.run_schedule(bevy::state::state::StateTransition);
        exit.push(ms(t));
        // `generation::rebuild`, step by step.
        let t = Instant::now();
        let set = world.resource::<bevy_ts::generation::LiveSet>().0.clone();
        let mut a = bevy_ts::generation::Applying::begin(world, set, true);
        let s = Instant::now();
        a.reset(world);
        steps[0].push(ms(s));
        let s = Instant::now();
        a.restart_engine();
        steps[1].push(ms(s));
        let s = Instant::now();
        a.load(world);
        steps[2].push(ms(s));
        let s = Instant::now();
        a.sections(world);
        steps[3].push(ms(s));
        let s = Instant::now();
        let report = a.apply(world);
        steps[4].push(ms(s));
        assert!(report.errors.is_empty(), "{:?}", report.errors);
        let s = Instant::now();
        bevy_ts::generation::run_startups(world);
        steps[5].push(ms(s));
        rebuild.push(ms(t));
        let t = Instant::now();
        bevy_ts_levels::engine::restore_open(world, &open);
        bevy_ts_levels::engine::restore(world);
        restore.push(ms(t));
        let t = Instant::now();
        bevy_ts_levels::engine::settle(world);
        settle.push(ms(t));
        world.remove_resource::<bevy_ts_tsx::StopPlan>();
        bevy_ts::runtime::reset_clock(world);
        // Back in step with the viewport's own mode.
        world.resource_mut::<NextState<EngineMode>>().set(EngineMode::Play);
        world.run_schedule(bevy::state::state::StateTransition);
        mode(app, "edit");
        app.update();
    }
    let name = match arm {
        Arm::Full => "full remount".to_string(),
        Arm::Touched(0) => "restore, none touched".to_string(),
        Arm::Touched(k) if k >= n => "restore, all touched".to_string(),
        Arm::Touched(k) => format!("restore, {k} touched"),
    };
    let [reset, engine, load, sections, apply, startups] = steps;
    println!(
        "{n} {name}: stop {total:.1?} ms, respawned {respawned:?}; plan {plan:.1?}, onexit {exit:.2?}, rebuild {rebuild:.1?} (reset {reset:.1?}, engine {engine:.1?}, load {load:.1?}, sections {sections:.1?}, apply {apply:.1?}, startups {startups:.2?}), restore {restore:.1?}, settle {settle:.1?}"
    );
    let steps = [reset, engine, load, sections, apply, startups].map(median);
    format!(
        "| {n} | {name} | {:.0} | {:.1} | {:.2} | {:.0} | {:.1} | {:.1} | {:.1} | {:.1} | {:.1} | {:.2} | {:.1} | {:.0} |",
        median(total),
        median(plan),
        median(exit),
        median(rebuild),
        steps[0],
        steps[1],
        steps[2],
        steps[3],
        steps[4],
        steps[5],
        median(restore),
        median(settle)
    )
}

/// One size: the level loaded once, then each arm.
fn run(n: usize) -> Vec<String> {
    let dir = project(&format!("stop-bench-{n}"), &[("package.json", "{}"), ("src/enemies.tsx", ENEMIES), ("assets/levels/arena.bsn", &level(n))]);
    let mut app = bench_app(&dir);
    app.update();
    assert!(reload(&mut app, false).errors.is_empty());
    loop {
        app.update();
        let levels = app.world().resource::<Levels>();
        if levels.roots.values().all(|r| !r.dirty && !r.applied.is_empty()) {
            break;
        }
    }
    let entities = app.world().resource::<bevy_ts_tsx::TsxTree>().entities.len();
    assert!(entities >= n * 2, "{entities} entities");
    println!("{n}: {entities} entities, kept tree {:.1} MB", kept_bytes(&app) as f64 / 1e6);
    [Arm::Full, Arm::Touched(0), Arm::Touched(100), Arm::Touched(n)].into_iter().map(|a| arm(&mut app, n, a)).collect()
}

#[test]
#[ignore]
fn stop_at_scale() {
    let mut rows = vec![
        "| instances | arm | Stop (ms) | plan (ms) | OnExit (ms) | rebuild (ms) | reset | engine | load | sections | apply | startups | restore (ms) | settle (ms) |".to_string(),
        "|---:|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|".to_string(),
    ];
    for n in [1_000, 10_000] {
        rows.extend(run(n));
    }
    println!("{}", rows.join("\n"));
}
