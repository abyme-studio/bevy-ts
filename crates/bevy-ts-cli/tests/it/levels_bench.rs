//! Levels at scale (levels task 6.3): load, reload and expansion of 1,000 and 10,000 scene
//! component instances, and the writer's latency per edit. Ignored by default; run it in release:
//! `cargo static test --release -p bevy-ts-cli --test it levels_bench:: -- --ignored --nocapture`.

use crate::common;

use bevy::prelude::*;
use bevy_ts_levels::edit::{WriteRequest, write_level};
use bevy_ts_levels::engine::{Levels, LevelsPlugin, reconcile_level};
use bevy_ts_levels::route::loaded_hash;
use bevy_ts_levels::writer::{Op, WValue};
use common::*;
use std::path::Path;
use std::time::Instant;

const ENEMIES: &str = r#"import { app, Component, SceneComponent, type f32, type u8 } from "bevy-ts";
import { Transform } from "bevy-ts/bevy";
export class Health extends Component { hp: f32 = 100; max: f32 = 100 }
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
app.level("levels/arena.bsn");
"#;

fn level(n: usize) -> String {
    (0..n)
        .map(|i| format!("#G{i}\n@enemies::Grunt {{ @hp: {}.0 }}\nTransform {{ translation: Vec3 {{ x: {}.0, z: -4.0 }} }}\n", 100 + i % 50, i % 100))
        .collect::<Vec<_>>()
        .join("--\n")
}

fn level_app(dir: &Path) -> App {
    let mut app = App::new();
    // Canonical: Bevy's file watcher panics on a symlinked root (macOS /var).
    let assets = dir.join("assets").canonicalize().unwrap();
    app.add_plugins((MinimalPlugins, AssetPlugin { file_path: assets.to_string_lossy().into_owned(), ..Default::default() }))
        .init_asset::<Mesh>()
        .init_asset::<StandardMaterial>()
        .add_plugins(bevy_ts::BevyTsPlugin::new(bevy_ts_v8::factory(Default::default())).dev(true))
        .add_plugins(bevy_ts_tsx::TsxPlugin)
        .add_plugins(LevelsPlugin);
    app.world_mut().resource_mut::<Levels>().asset_root = assets;
    app.insert_resource(bevy_ts::generation::DevSource(std::sync::Arc::new(bevy_ts_cli::compile::Compiler::new(dir.join("src/enemies.tsx"), true).unwrap())));
    app
}

fn ms(t: Instant) -> f64 {
    t.elapsed().as_secs_f64() * 1000.0
}

fn run(n: usize) -> String {
    let dir = project(&format!("levels-bench-{n}"), &[("package.json", "{}"), ("src/enemies.tsx", ENEMIES), ("assets/levels/arena.bsn", &level(n))]);
    let mut app = level_app(&dir);
    let t = Instant::now();
    assert!(reload(&mut app, false).errors.is_empty());
    loop {
        app.update();
        let levels = app.world().resource::<Levels>();
        if levels.roots.values().all(|r| !r.dirty && !r.applied.is_empty()) {
            break;
        }
    }
    let load = ms(t);
    let spawned = app.world().resource::<bevy_ts_tsx::TsxTree>().entities.len();
    assert!(spawned >= n * 2, "{spawned} entities");

    // A reconcile of the unchanged file: expansion plus a diff that writes nothing.
    let t = Instant::now();
    reconcile_level(app.world_mut(), "levels/arena.bsn");
    let reconcile = ms(t);
    assert_eq!(app.world().resource::<bevy_ts_tsx::TsxTree>().last.written, 0);

    // The expansion host call alone.
    let items: Vec<bevy_ts_tsx::ExpandItem> = (0..n)
        .map(|i| bevy_ts_tsx::ExpandItem {
            component: "enemies::Grunt".into(),
            props: serde_json::json!({ "hp": 100 + i % 50 }).as_object().unwrap().clone(),
            at: bevy_ts::backend::SourceLocation { file: "levels/arena.bsn".into(), line: 1, column: 1 },
        })
        .collect();
    let t = Instant::now();
    bevy_ts_tsx::expand(app.world_mut(), &items).unwrap();
    let expand = ms(t);

    // Writer latency: one-field edits, each a full write (splice, verify, atomic write, reload).
    let edits = 20;
    let t = Instant::now();
    for k in 0..edits {
        let base = loaded_hash(app.world(), "levels/arena.bsn").unwrap();
        let op = Op::Set { path: vec![format!("G{}", k * (n / edits))], component: "Transform".into(), field: vec!["translation".into(), "x".into()], value: WValue::Float { v: 0.5 + k as f64, f32: true }, containers: vec!["Vec3".into()] };
        let r = write_level(app.world_mut(), WriteRequest { id: k as u64, file: "levels/arena.bsn".into(), base, ops: vec![op] });
        assert_eq!(r.status, "written", "{r:?}");
    }
    let write = ms(t) / edits as f64;

    // Where one write goes: the writer (splice, two parses, verify), the parse of the new text
    // for the asset, the level's compose without expansion, and tsx's resolve and diff.
    let text = std::fs::read_to_string(dir.join("assets/levels/arena.bsn")).unwrap();
    let op = Op::Set { path: vec!["G7".into()], component: "Transform".into(), field: vec!["translation".into(), "x".into()], value: WValue::Float { v: 99.0, f32: true }, containers: vec!["Vec3".into()] };
    let t = Instant::now();
    let w = bevy_ts_levels::writer::apply(&text, &[op], false, None).unwrap();
    let splice = ms(t);
    let t = Instant::now();
    bevy_ts_levels::syntax::parse(&w.text).unwrap();
    let parse = ms(t);
    let (composed, _) = bevy_ts_levels::engine::compose_level(app.world_mut(), "levels/arena.bsn");
    let nodes = composed.unwrap().nodes;
    let t = Instant::now();
    bevy_ts_tsx::reconcile_external(app.world_mut(), "level:levels/arena.bsn", None, &nodes).unwrap();
    let tsx = ms(t);
    let compose = reconcile - expand - tsx;
    println!("{n}: write {write:.1} ms = writer {splice:.1} (splice, 2 parses, verify) + asset parse {parse:.1} + compose {compose:.1} + expand {expand:.1} + tsx {tsx:.1}, roughly");
    format!("| {n} | {} | {load:.0} | {reconcile:.1} | {expand:.1} | {write:.1} |", spawned)
}

#[test]
#[ignore]
fn levels_at_scale() {
    let mut rows = vec![
        "| instances | entities | load (ms) | unchanged reconcile (ms) | expand call (ms) | write per edit (ms) |".to_string(),
        "|---:|---:|---:|---:|---:|---:|".to_string(),
    ];
    for n in [1_000, 10_000] {
        rows.push(run(n));
    }
    println!("{}", rows.join("\n"));
}
