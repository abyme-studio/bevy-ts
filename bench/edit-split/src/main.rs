//! Where one level write's time goes (openspec change `touched-path-reconcile`, task 6.2): a
//! level of N `@enemies::Grunt` instances (two entities each), the level and Stop benches' level,
//! and five kinds of write on it (a move, a prop edit, an add, a delete, a rename), each timed by
//! step. Prints one `RESULT` line per size, arm and kind: wall ms, medians of `EDITS` writes. The
//! arms: `whole` (`PartialEdits(false)`, every write reconciles the level whole, as before the
//! change) and `partial`.
//! `--summary <file>` reads those lines back and prints the medians across runs as a table.

use bevy::prelude::*;
use bevy_ts_levels::edit::{EditSteps, PartialEdits, WriteRequest, write_level};
use bevy_ts_levels::engine::{Levels, LevelsPlugin};
use bevy_ts_levels::route::loaded_hash;
use bevy_ts_levels::writer::{Base, Op, WValue};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
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

const LEVEL: &str = "levels/arena.bsn";
/// Writes per kind, per arm: the median is the 4th.
const EDITS: usize = 7;
const KINDS: [&str; 5] = ["move", "prop", "add", "delete", "rename"];
const STEPS: [&str; 8] = ["writer", "parse", "shift", "compose", "expand", "reconcile", "stamp", "other"];

fn level(n: usize) -> String {
    (0..n)
        .map(|i| format!("#G{i}\n@enemies::Grunt {{ @hp: {}.0 }}\nTransform {{ translation: Vec3 {{ x: {}.0, z: -4.0 }} }}\n", 100 + i % 50, i % 100))
        .collect::<Vec<_>>()
        .join("--\n")
}

fn project(n: usize) -> PathBuf {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("target/projects").join(format!("{n}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    for (f, text) in [("package.json", "{}".to_string()), ("src/enemies.tsx", ENEMIES.to_string()), ("assets/levels/arena.bsn", level(n))] {
        let p = dir.join(f);
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(p, text).unwrap();
    }
    dir.canonicalize().unwrap()
}

fn level_app(dir: &Path) -> App {
    let mut app = App::new();
    let assets = dir.join("assets");
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

fn median(mut v: Vec<f64>) -> f64 {
    v.sort_by(f64::total_cmp);
    v[v.len() / 2]
}

/// The `k`th write of a kind, on its own instance (`k` is unique across arms).
fn op(kind: &str, n: usize, k: usize) -> Op {
    let g = |i: usize| format!("G{}", (i * 997 + 13) % n);
    match kind {
        "move" => Op::Set { path: vec![g(k)], component: "Transform".into(), field: vec!["translation".into(), "x".into()], value: WValue::Float { v: 0.5 + k as f64, f32: true }, containers: vec!["Vec3".into()] },
        "prop" => Op::SetProp { path: vec![g(k + 100)], prop: "hp".into(), value: WValue::Float { v: 200.0 + k as f64, f32: true }, field: false },
        "add" => Op::Add { parent: vec![], name: Some(format!("New{k}")), base: Base::Scene { component: "enemies::Grunt".into(), props: vec![], fields: vec![] }, patches: vec![], asset: None },
        "delete" => Op::Delete { path: vec![g(k + 200)] },
        "rename" => Op::Rename { path: vec![g(k + 300)], to: format!("Renamed{k}") },
        _ => unreachable!(),
    }
}

/// One write, its wall time and, where the engine records them, its steps.
fn write(app: &mut App, op: Op, id: u64) -> (f64, BTreeMap<&'static str, f64>) {
    let base = loaded_hash(app.world(), LEVEL).unwrap();
    let t = Instant::now();
    let r = write_level(app.world_mut(), WriteRequest { id, file: LEVEL.into(), base, ops: vec![op] });
    let total = ms(t);
    assert_eq!(r.status, "written", "{r:?}");
    (total, steps(app.world(), total))
}

/// The engine's record of the last write, by step; "other" is the total less the rest.
fn steps(world: &World, total: f64) -> BTreeMap<&'static str, f64> {
    let s = world.resource::<EditSteps>();
    let mut out = BTreeMap::from([("writer", s.writer), ("parse", s.parse), ("shift", s.shift), ("compose", s.compose), ("expand", s.expand), ("reconcile", s.reconcile), ("stamp", s.stamp)]);
    out.insert("other", total - out.values().sum::<f64>());
    out
}

fn arm(app: &mut App, n: usize, arm: &str, first: usize) {
    app.insert_resource(PartialEdits(arm == "partial"));
    for kind in KINDS {
        let mut totals = Vec::new();
        let mut by_step: BTreeMap<&str, Vec<f64>> = BTreeMap::new();
        let mut whole = Vec::new();
        for k in first..first + EDITS {
            let (total, steps) = write(app, op(kind, n, k), k as u64);
            if let Some(why) = &app.world().resource::<EditSteps>().why {
                whole.push(why.clone());
            }
            totals.push(total);
            for (s, v) in steps {
                by_step.entry(s).or_default().push(v);
            }
        }
        let mut line = format!("RESULT n={} arm={arm} op={kind} total={:.2}", n * 2, median(totals));
        for s in STEPS {
            if let Some(v) = by_step.remove(s) {
                line.push_str(&format!(" {s}={:.2}", median(v)));
            }
        }
        println!("{line}");
        if arm == "partial" && !whole.is_empty() {
            println!("WHOLE n={} op={kind}: {}", n * 2, whole.join("; "));
        }
    }
}

fn run(n: usize) {
    let dir = project(n);
    let mut app = level_app(&dir);
    let report = bevy_ts::generation::reload_now(app.world_mut(), false).unwrap();
    assert!(report.errors.is_empty(), "{:?}", report.errors);
    loop {
        app.update();
        if app.world().resource::<Levels>().roots.values().all(|r| !r.dirty && !r.applied.is_empty()) {
            break;
        }
    }
    // The first write after a load pays for warming caches: one untimed.
    write(&mut app, op("move", n, 99), 999);
    arm(&mut app, n, "whole", 0);
    arm(&mut app, n, "partial", EDITS);
    drop(app);
    let _ = std::fs::remove_dir_all(dir);
}

/// Medians across runs of each `RESULT` field, as a markdown table.
fn summary(file: &str) {
    let text = std::fs::read_to_string(file).unwrap();
    let mut rows: BTreeMap<(usize, String, usize), BTreeMap<String, Vec<f64>>> = BTreeMap::new();
    let mut columns: Vec<String> = Vec::new();
    for line in text.lines().filter_map(|l| l.strip_prefix("RESULT ")) {
        let fields: BTreeMap<&str, &str> = line.split(' ').filter_map(|f| f.split_once('=')).collect();
        let kind = KINDS.iter().position(|k| *k == fields["op"]).unwrap();
        let row = rows.entry((fields["n"].parse().unwrap(), fields["arm"].to_string(), kind)).or_default();
        for (k, v) in fields.iter().filter(|(k, _)| !matches!(**k, "n" | "arm" | "op")) {
            if !columns.iter().any(|c| c == k) {
                columns.push(k.to_string());
            }
            row.entry(k.to_string()).or_default().push(v.parse().unwrap());
        }
    }
    columns.sort_by_key(|c| if c == "total" { 0 } else { 1 + STEPS.iter().position(|s| s == c).unwrap_or(99) });
    println!("| entities | arm | write | {} |", columns.join(" | "));
    println!("|---:|---|---|{}|", columns.iter().map(|_| "---:").collect::<Vec<_>>().join("|"));
    for ((n, arm, kind), mut cells) in rows {
        let values: Vec<String> = columns.iter().map(|c| cells.remove(c).map_or("".into(), |v| format!("{:.1}", median(v)))).collect();
        println!("| {n} | {arm} | {} | {} |", KINDS[kind], values.join(" | "));
    }
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if let [_, flag, file] = &args[..]
        && flag == "--summary"
    {
        summary(file);
        return;
    }
    for n in [1_000, 10_000] {
        run(n);
    }
}
