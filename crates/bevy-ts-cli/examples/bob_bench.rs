//! Frame cost of the cube demo's `bob` over 1, 1,000 and 100,000 entities, with and without the
//! per-row tick store. Headless, release: `cargo static run --release -p bevy-ts-cli --example bob_bench`.
//!
//! Variants, all writing `translation.y` of every row:
//! - `accessor+tick`: `q.each` with the generated setter (stores the row's tick), the default;
//! - `raw, no tick`: the column's `f32` array through `table.read`, no tick written;
//! - `raw+tick`: `table.split` and `ticks[row] = ctx.tick`.
//!
//! Reported per variant: the TS system's run (view building, the one call into JS, post-run
//! ticks), and the whole `app.update()`.

// A release build of this bench links Bevy statically, as what ships does: `cargo static`
// (docs/build-cache.md, Bevy as a dylib for tests).
#[cfg(all(not(debug_assertions), not(bevy_ts_static)))]
compile_error!("a release build of this bench links the Bevy dylib: build it with `cargo static`");

use bevy::prelude::*;
use bevy_ts::BevyTsPlugin;
use bevy_ts::components::TsComponents;
use bevy_ts::generation::DevSource;
use bevy_ts::systems::TsSystems;
use bevy_ts_cli::compile::Compiler;
use bevy_ts_v8::V8Options;
use std::sync::Arc;
use std::time::Instant;

const HEAD: &str = r#"import { app, Component, f32, Time, Transform, Update } from "bevy-ts";
import type { Mut, Query, Res } from "bevy-ts";
export class Mover extends Component { static path = "game::Mover"; speed: f32 = 1; amplitude: f32 = 2 }
"#;

fn variant(name: &str) -> String {
    let body = match name {
        "accessor+tick" => "q.each((m, t) => { t.translation.y = Math.sin(time.elapsed_secs * m.speed) * m.amplitude; });",
        "raw, no tick" => {
            "for (const tb of q.tables) { const m = tb.read(Mover), t = tb.read(Transform); const ms = m.size / 4, ts = t.size / 4, sp = m.offsets.speed / 4, am = m.offsets.amplitude / 4, ty = t.offsets.translation / 4 + 1; for (let r = 0; r < tb.len; r++) t.f32![r * ts + ty] = Math.sin(time.elapsed_secs * m.f32![r * ms + sp]) * m.f32![r * ms + am]; }"
        }
        _ => {
            "for (const tb of q.tables) { const m = tb.read(Mover), t = tb.split(Transform); const ms = m.size / 4, ts = t.size / 4, sp = m.offsets.speed / 4, am = m.offsets.amplitude / 4, ty = t.offsets.translation / 4 + 1, tick = q.tick, ticks = t.ticks; for (let r = 0; r < tb.len; r++) { t.f32![r * ts + ty] = Math.sin(time.elapsed_secs * m.f32![r * ms + sp]) * m.f32![r * ms + am]; ticks[r] = tick; } }"
        }
    };
    format!("{HEAD}export function bob(time: Res<Time>, q: Query<[Mover, Mut<Transform>]>) {{ {body} }}\napp.addSystems(Update, bob);\n")
}

fn median(mut v: Vec<f64>) -> f64 {
    v.sort_by(f64::total_cmp);
    v[v.len() / 2]
}

fn main() {
    let dir = std::env::temp_dir().join("bevy-ts-bob-bench");
    std::fs::create_dir_all(&dir).unwrap();
    let entry = dir.join("game.ts");
    println!("| entities | variant | bob run (µs, median) | app.update (µs, median) |");
    println!("|---|---|---|---|");
    for n in [1usize, 1_000, 100_000] {
        for v in ["accessor+tick", "raw, no tick", "raw+tick"] {
            std::fs::write(&entry, variant(v)).unwrap();
            let mut app = App::new();
            app.add_plugins(MinimalPlugins).add_plugins(BevyTsPlugin::new(bevy_ts_v8::factory(V8Options::default())));
            app.insert_resource(DevSource(Arc::new(Compiler::new(&entry, false).unwrap())));
            let r = bevy_ts::generation::reload_now(app.world_mut(), false).unwrap();
            assert!(r.errors.is_empty(), "{:?}", r.errors);
            let id = app.world().resource::<TsComponents>().live("game::Mover").unwrap().id;
            let bytes = [1f32.to_le_bytes(), 2f32.to_le_bytes()].concat();
            for i in 0..n {
                let mut e = app.world_mut().spawn(Transform::from_xyz(i as f32, 0.0, 0.0));
                unsafe { bevy_ts::commands::insert_bytes(&mut e, id, &bytes) };
            }
            for _ in 0..100 {
                app.update();
            }
            let (mut runs, mut frames) = (Vec::new(), Vec::new());
            for _ in 0..300 {
                let t = Instant::now();
                app.update();
                frames.push(t.elapsed().as_secs_f64() * 1e6);
                runs.push(app.world().resource::<TsSystems>().iter().next().unwrap().last_run_us as f64);
            }
            println!("| {n} | {v} | {:.0} | {:.0} |", median(runs), median(frames));
        }
    }
}
