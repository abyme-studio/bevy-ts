//! What a Stop costs (play-stop task 6.1): the levels bench's level at 1,000 and 10,000 instances,
//! in the app `bevy-ts dev --viewport` builds. Each Stop end to end through `viewport_mode`, and
//! its parts called one by one as Stop calls them: `OnExit(Play)`, the rebuild (a fresh isolate
//! and the generation applied), the levels' restore and settle; then the next Play's `Startup`.
//! Ignored by default; run it in release:
//! `cargo test --release -p bevy-ts-cli --test it stop_bench:: -- --ignored --nocapture`.
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

/// One size: three end-to-end Stops, then three split ones; medians.
fn run(n: usize) -> String {
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

    let mut total = vec![];
    let mut startup = vec![];
    for _ in 0..3 {
        let t = Instant::now();
        assert_eq!(mode(&mut app, "play")["started"], true);
        startup.push(ms(t));
        app.update();
        let t = Instant::now();
        assert_eq!(mode(&mut app, "edit")["started"], false);
        total.push(ms(t));
        assert!(app.world().resource::<bevy_ts_tsx::TsxTree>().entities.len() >= n * 2, "the level stands when Stop answers");
        app.update();
    }

    // The parts, as `stop` calls them after `enter_edit`.
    let (mut exit, mut rebuild, mut settle) = (vec![], vec![], vec![]);
    for _ in 0..3 {
        mode(&mut app, "play");
        app.update();
        let open = bevy_ts_levels::engine::open_set(app.world());
        let world = app.world_mut();
        let t = Instant::now();
        world.resource_mut::<NextState<EngineMode>>().set(EngineMode::Edit);
        world.run_schedule(bevy::state::state::StateTransition);
        exit.push(ms(t));
        let t = Instant::now();
        bevy_ts::generation::rebuild(world).unwrap();
        rebuild.push(ms(t));
        let t = Instant::now();
        bevy_ts_levels::engine::restore_open(world, &open);
        bevy_ts_levels::engine::settle(world);
        settle.push(ms(t));
        bevy_ts::runtime::reset_clock(world);
        // Back in step with the viewport's own mode.
        world.resource_mut::<NextState<EngineMode>>().set(EngineMode::Play);
        world.run_schedule(bevy::state::state::StateTransition);
        mode(&mut app, "edit");
        app.update();
    }
    println!("{n}: stop {total:.1?} ms; onexit {exit:.2?}, rebuild {rebuild:.1?}, settle {settle:.1?}; next play {startup:.1?}");
    format!(
        "| {n} | {entities} | {:.0} | {:.2} | {:.0} | {:.0} | {:.1} |",
        median(total),
        median(exit),
        median(rebuild),
        median(settle),
        median(startup)
    )
}

#[test]
#[ignore]
fn stop_at_scale() {
    let mut rows = vec![
        "| instances | entities | Stop (ms) | OnExit (ms) | rebuild (ms) | levels settle (ms) | next Play with Startup (ms) |".to_string(),
        "|---:|---:|---:|---:|---:|---:|---:|".to_string(),
    ];
    for n in [1_000, 10_000] {
        rows.push(run(n));
    }
    println!("{}", rows.join("\n"));
}
