//! What reading Rust messages from TS costs (openspec rust-events, task 2.4). Each frame Rust
//! writes 1,000 `CursorMoved`; one TS system reads them all through `MessageReader<CursorMoved>`
//! and sums a field. The same app with a system that takes no reader is the base. The difference
//! is the cost of the reads: encoding in Rust along the message's codec (`reflect_codec`), the
//! crossing, decoding and the loop in JS. Encoding alone is timed in Rust on the same messages.

use bevy::input::InputPlugin;
use bevy::prelude::*;
use bevy::reflect::Typed;
use bevy::window::{CursorMoved, ExitCondition, WindowPlugin};
use bevy_ts::BevyTsPlugin;
use bevy_ts::generation::DevSource;
use bevy_ts::reflect_codec;
use bevy_ts_cli::compile::Compiler;
use bevy_ts_v8::V8Options;
use std::sync::Arc;
use std::time::Instant;

const MESSAGES: usize = 1_000;
const FRAMES: usize = 300;
const WARMUP: usize = 30;

const BASE: &str = r#"import { app, Update } from "bevy-ts";
export function read() {}
app.addSystems(Update, read);
"#;

const READING: &str = r#"import { app, Update } from "bevy-ts";
import type { MessageReader } from "bevy-ts";
import { CursorMoved } from "bevy-ts/bevy";
export function read(r: MessageReader<CursorMoved>) {
  let s = 0;
  r.each((m) => { s += m.position[0]; });
  if (s === -1) console.log("never");
}
app.addSystems(Update, read);
"#;

fn app(name: &str, src: &str) -> App {
    let dir = std::env::temp_dir().join("bevy-ts-rust-events-bench").join(name);
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("package.json"), "{}").unwrap();
    std::fs::write(dir.join("game.ts"), src).unwrap();
    let mut app = App::new();
    app.add_plugins(MinimalPlugins)
        .add_plugins(BevyTsPlugin::new(bevy_ts_v8::factory(V8Options::default())))
        .add_plugins(InputPlugin)
        .add_plugins(WindowPlugin { primary_window: None, exit_condition: ExitCondition::DontExit, ..default() })
        .add_plugins(bevy_ts_cli::rust_events::RustEventsPlugin)
        .insert_resource(DevSource(Arc::new(Compiler::new(&dir.join("game.ts"), true).unwrap())));
    let r = bevy_ts::generation::reload_now(app.world_mut(), false).unwrap();
    assert!(r.errors.is_empty(), "{:?}", r.errors);
    app
}

fn cursor(window: Entity, i: usize) -> CursorMoved {
    CursorMoved { window, position: Vec2::new(i as f32, 2.0 * i as f32), delta: Some(Vec2::ONE) }
}

/// Microseconds per frame, median of `FRAMES`, each frame after writing `MESSAGES` messages.
fn frames(app: &mut App) -> f64 {
    let window = app.world_mut().spawn_empty().id();
    let mut frame = || {
        for i in 0..MESSAGES {
            app.world_mut().write_message(cursor(window, i));
        }
        let t = Instant::now();
        app.update();
        t.elapsed().as_secs_f64() * 1e6
    };
    for _ in 0..WARMUP {
        frame();
    }
    median((0..FRAMES).map(|_| frame()).collect())
}

/// Microseconds to encode `MESSAGES` messages along their codec, as the reader does.
fn encoding(app: &App) -> f64 {
    let registry = app.world().resource::<AppTypeRegistry>().clone();
    let registry = registry.read();
    let messages: Vec<CursorMoved> = (0..MESSAGES).map(|i| cursor(Entity::PLACEHOLDER, i)).collect();
    let node = reflect_codec::compile(&registry, CursorMoved::type_info(), 0);
    let mut buf = Vec::new();
    let mut once = || {
        let t = Instant::now();
        buf.clear();
        buf.extend_from_slice(&(messages.len() as u32).to_le_bytes());
        for m in &messages {
            reflect_codec::put(&registry, &node, m, &mut buf).unwrap();
        }
        t.elapsed().as_secs_f64() * 1e6
    };
    for _ in 0..WARMUP {
        once();
    }
    median((0..FRAMES).map(|_| once()).collect())
}

fn median(mut v: Vec<f64>) -> f64 {
    v.sort_by(f64::total_cmp);
    v[v.len() / 2]
}

fn main() {
    let base = frames(&mut app("base", BASE));
    let reading_app = &mut app("reading", READING);
    let with = frames(reading_app);
    let ser = encoding(reading_app);
    let reads = with - base;
    println!(
        "{MESSAGES} CursorMoved a frame: frame without a reader {base:.0} us, with {with:.0} us: {:.3} ms for the reads, {:.0} ns per message ({:.0} ns encoding, {:.0} ns crossing, decode and loop)",
        reads / 1e3,
        reads * 1e3 / MESSAGES as f64,
        ser * 1e3 / MESSAGES as f64,
        (reads - ser) * 1e3 / MESSAGES as f64,
    );
    if reads > 1_000.0 {
        println!("OVER BUDGET: the reads cost more than 1 ms a frame");
    }
}
