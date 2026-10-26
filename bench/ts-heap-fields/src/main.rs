//! What heap fields cost (openspecs ts-component-heap-fields tasks 1.20, 2.10, and
//! ts-heap-fields-2 tasks 1.8, ...), through V8 in release. Each arm is one TS system over 1,000
//! rows or reads; its time is the system's run time less an empty system's over the same rows, per
//! row, median of 200 frames (docs/reflection.md's method). Despawn is timed in Rust: 1,000 rows
//! despawned, per row. A lifecycle value's read is timed by frame: 1,000 `On<Remove>` events a
//! frame, read against not read. Messages and events are timed by whole frame: 1,000 written and
//! read, or triggered and observed, a frame, plain against with a string.
//!
//! `plain` (the first argument) runs the arms of a component with no heap field only, with no
//! API this change added, so the same file builds against main: `run.sh` runs them there too,
//! and the two must match.

use bevy::prelude::*;
use bevy_ts::BevyTsPlugin;
use bevy_ts::components::TsComponents;
use bevy_ts::generation::DevSource;
use bevy_ts::systems::TsSystems;
use bevy_ts_cli::compile::Compiler;
use bevy_ts_v8::V8Options;
use std::sync::Arc;
use std::time::Instant;

const ROWS: usize = 1000;
/// Column and hit arms read each row this many times a run, for timer resolution.
const REPEAT: usize = 10;
const FRAMES: usize = 200;
const WARMUP: usize = 20;
/// Apps built for the despawn arm; the median is taken.
const DESPAWNS: usize = 15;

const PLAIN: &str = r#"import { app, chain, Component, Resource, Startup, Update } from "bevy-ts";
import type { Commands, f32, Local, Query, Res } from "bevy-ts";
export class Plain extends Component { hp: f32 = 1; x: f32 = 0 }
export class Score extends Resource { value: f32 = 1 }
const REPEAT = __REPEAT__;
let sink = 0;
export function make(commands: Commands) { for (let i = 0; i < __ROWS__; i++) commands.spawn(Plain.of({ hp: i })); }
export function empty(q: Query<[Plain]>) { q.each(() => { sink++; }); }
export function plainColumn(q: Query<[Plain]>) {
  for (let k = 0; k < REPEAT; k++) q.each((p) => { sink += p.hp; });
}
export function plainRes(q: Query<[Plain]>, s: Res<Score>) {
  for (let k = 0; k < REPEAT; k++) q.each(() => { sink += s.value; });
}
export function plainLocal(q: Query<[Plain]>, l: Local<{ n: f32 }>) {
  l.n = 1;
  for (let k = 0; k < REPEAT; k++) q.each(() => { sink += l.n; });
}
app.insertResource(Score.of({ value: 2 }));
app.addSystems(Startup, make);
app.addSystems(Update, chain(__SYSTEM__));
"#;

const HEAP: &str = r#"import { app, chain, Component, Startup, Update } from "bevy-ts";
import type { Commands, f32, Local, Mut, Query, Res, Vec3 } from "bevy-ts";
import { Resource, vec3 } from "bevy-ts";
export class Tag extends Component { label: string = "crate"; hp: f32 = 1; scores: f32[] = [] }
export class Trail extends Component { points: Vec3[] = [] }
export class Title extends Resource { text: string = "a title" }
app.insertResource(Title.of({ text: "a title" }));
const REPEAT = __REPEAT__;
let sink = 0;
let n = 0;
export function make(commands: Commands) {
  for (let i = 0; i < __ROWS__; i++) commands.spawn(Tag.of({ label: "tag " + i, hp: i, scores: [1, 2, 3, 4, 5, 6, 7, i] }));
  const points = [1, 2, 3, 4, 5, 6, 7, 8].map((k) => vec3(k, 0, 1));
  for (let i = 0; i < __ROWS__; i++) commands.spawn(Trail.of({ points }));
}
export function empty(q: Query<[Tag]>) { q.each(() => { sink++; }); }
export function emptyTrail(q: Query<[Trail]>) { q.each(() => { sink++; }); }
export function vecHit(q: Query<[Trail]>) {
  for (let k = 0; k < REPEAT; k++) q.each((t) => { sink += t.points[7].x; });
}
export function vecMiss(q: Query<[Trail]>) { q.each((t) => { sink += t.points[7].x; }); }
export function heapPlainField(q: Query<[Tag]>) {
  for (let k = 0; k < REPEAT; k++) q.each((t) => { sink += t.hp; });
}
export function stringHit(q: Query<[Tag]>) {
  for (let k = 0; k < REPEAT; k++) q.each((t) => { sink += t.label.length; });
}
export function stringMiss(q: Query<[Tag]>) { q.each((t) => { sink += t.label.length; }); }
export function stringWrite(q: Query<[Mut<Tag>]>) { n++; q.each((t) => { t.label = n % 2 ? "odd" : "even"; }); }
export function listHit(q: Query<[Tag]>) {
  for (let k = 0; k < REPEAT; k++) q.each((t) => { sink += t.scores.length; });
}
export function listMiss(q: Query<[Tag]>) { q.each((t) => { sink += t.scores.length; }); }
export function listWrite(q: Query<[Mut<Tag>]>) { n++; q.each((t) => { t.scores = [n, 2, 3, 4, 5, 6, 7, 8]; }); }
export function resString(q: Query<[Tag]>, t: Res<Title>) {
  for (let k = 0; k < REPEAT; k++) q.each(() => { sink += t.text.length; });
}
export function localString(q: Query<[Tag]>, l: Local<{ text: string }>) {
  if (l.text === "") l.text = "a local";
  for (let k = 0; k < REPEAT; k++) q.each(() => { sink += l.text.length; });
}
app.addSystems(Startup, make);
app.addSystems(Update, chain(__SYSTEM__));
"#;

/// 1,000 `On<Remove<Tag>>` events a frame: `cycle` removes and re-inserts each row's `Tag`, and the
/// observer reads the removed value's `label`, or not.
const LIFECYCLE: &str = r#"import { app, Component, Startup, Update } from "bevy-ts";
import type { Commands, Entity, f32, On, Query, Remove } from "bevy-ts";
export class Tag extends Component { label: string = "crate"; hp: f32 = 1 }
let sink = 0;
export function make(commands: Commands) { for (let i = 0; i < __ROWS__; i++) commands.spawn(Tag.of({ label: "tag " + i, hp: i })); }
export function cycle(q: Query<[Entity, Tag]>, commands: Commands) { q.each((e) => { commands.entity(e).remove(Tag).insert(Tag.of({ label: "tag", hp: 1 })); }); }
export function read(on: On<Remove<Tag>>) { sink += on.label.length; }
export function skip(on: On<Remove<Tag>>) { sink += on.hp; }
app.addSystems(Startup, make);
app.addSystems(Update, cycle);
app.addObserver(__SYSTEM__);
"#;

/// 1,000 messages written and read a frame, or 1,000 events triggered and observed: `__SYSTEM__`
/// is the registration. Plain types only, so `run.sh` builds it against main too.
const PLAIN_PAYLOADS: &str = r#"import { app, chain, Event, Message, Update } from "bevy-ts";
import type { Commands, f32, MessageReader, MessageWriter, On } from "bevy-ts";
export class Ping extends Message { n: f32 = 0 }
export class Hit extends Event { n: f32 = 0 }
let sink = 0;
export function send(w: MessageWriter<Ping>) { for (let i = 0; i < __ROWS__; i++) w.write(Ping.of({ n: i })); }
export function read(r: MessageReader<Ping>) { r.each((m) => { sink += m.n; }); }
export function fire(c: Commands) { for (let i = 0; i < __ROWS__; i++) c.trigger(Hit.of({ n: i })); }
export function hear(on: On<Hit>) { sink += on.n; }
app.addMessage(Ping);
__SYSTEM__
"#;

/// As `PLAIN_PAYLOADS`, each message and event carrying a string the reader reads.
const HEAP_PAYLOADS: &str = r#"import { app, chain, Event, Message, Update } from "bevy-ts";
import type { Commands, f32, MessageReader, MessageWriter, On } from "bevy-ts";
export class Chat extends Message { text: string = ""; n: f32 = 0 }
export class Shout extends Event { text: string = ""; n: f32 = 0 }
let sink = 0;
export function send(w: MessageWriter<Chat>) { for (let i = 0; i < __ROWS__; i++) w.write(Chat.of({ text: "hello", n: i })); }
export function read(r: MessageReader<Chat>) { r.each((m) => { sink += m.text.length; }); }
export function fire(c: Commands) { for (let i = 0; i < __ROWS__; i++) c.trigger(Shout.of({ text: "hello", n: i })); }
export function hear(on: On<Shout>) { sink += on.text.length; }
app.addMessage(Chat);
__SYSTEM__
"#;

const MESSAGES: &str = "app.addSystems(Update, chain(send, read));";
const EVENTS: &str = "app.addSystems(Update, fire); app.addObserver(hear);";

/// Median frame time in microseconds with `observer`, after a warm-up.
fn frames(observer: &str) -> f64 {
    frame_us(observer, LIFECYCLE, observer)
}

/// Nanoseconds a message or event: a frame of `ROWS` of them, whole.
fn payload_ns(src: &str, system: &str) -> f64 {
    (frame_us("payloads", src, system) * 1000.0 / ROWS as f64).round()
}

/// Median frame time in microseconds of `src` with `system`, after a warm-up.
fn frame_us(name: &str, src: &str, system: &str) -> f64 {
    let mut app = app(name, src, system, None);
    for _ in 0..WARMUP {
        app.update();
    }
    let us: Vec<f64> = (0..FRAMES)
        .map(|_| {
            let t = Instant::now();
            app.update();
            t.elapsed().as_secs_f64() * 1e6
        })
        .collect();
    median(us)
}

/// A component name, marked changed on every row before the TS stage, so each read misses.
#[derive(Resource)]
struct Touch(&'static str);

fn touch(world: &mut World) {
    let Some(name) = world.get_resource::<Touch>().map(|t| t.0) else { return };
    let Some(id) = world.resource::<TsComponents>().live(name).map(|c| c.id) else { return };
    let es: Vec<Entity> = QueryBuilder::<Entity>::new(world).with_id(id).build().iter(world).collect();
    for e in es {
        world.entity_mut(e).get_mut_by_id(id).unwrap().set_changed();
    }
}

fn app(name: &str, src: &str, system: &str, miss: Option<&'static str>) -> App {
    let src = src.replace("__REPEAT__", &REPEAT.to_string()).replace("__ROWS__", &ROWS.to_string()).replace("__SYSTEM__", system);
    let dir = std::env::temp_dir().join(format!("bevy-ts-heap-fields-bench-{}", std::process::id())).join(name);
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("package.json"), "{}").unwrap();
    std::fs::write(dir.join("game.ts"), src).unwrap();
    let mut app = App::new();
    app.add_plugins(MinimalPlugins)
        .add_plugins(BevyTsPlugin::new(bevy_ts_v8::factory(V8Options::default())))
        .insert_resource(DevSource(Arc::new(Compiler::new(&dir.join("game.ts"), true).unwrap())));
    if let Some(c) = miss {
        app.insert_resource(Touch(c)).add_systems(Update, touch.before(bevy_ts::BevyTsSet::Update));
    }
    let r = bevy_ts::generation::reload_now(app.world_mut(), false).unwrap();
    assert!(r.errors.is_empty(), "{name}: {:?}", r.errors);
    app
}

/// Median run time in microseconds of `system` over `FRAMES` frames, after a warm-up.
fn run(src: &str, system: &str, miss: Option<&'static str>) -> f64 {
    let mut app = app(system, src, system, miss);
    for _ in 0..WARMUP {
        app.update();
    }
    let mut us = Vec::with_capacity(FRAMES);
    for _ in 0..FRAMES {
        app.update();
        let systems = app.world().resource::<TsSystems>();
        let s = systems.iter().find(|s| s.decl.name.rsplit('#').next() == Some(system)).expect("system");
        assert!(s.last_error.is_none(), "{system}: {:?}", s.last_error);
        us.push(s.last_run_us as f64);
    }
    median(us)
}

/// Nanoseconds a row to despawn `ROWS` rows of `component`, median of `DESPAWNS` apps.
fn despawn(src: &str, component: &'static str) -> f64 {
    let ns: Vec<f64> = (0..DESPAWNS)
        .map(|_| {
            let mut app = app("despawn", src, "empty", None);
            app.update();
            let world = app.world_mut();
            let id = world.resource::<TsComponents>().live(component).unwrap().id;
            let es: Vec<Entity> = QueryBuilder::<Entity>::new(world).with_id(id).build().iter(world).collect();
            assert_eq!(es.len(), ROWS);
            let t = Instant::now();
            for e in es {
                world.despawn(e);
            }
            t.elapsed().as_secs_f64() * 1e9 / ROWS as f64
        })
        .collect();
    median(ns)
}

fn median(mut v: Vec<f64>) -> f64 {
    v.sort_by(f64::total_cmp);
    v[v.len() / 2]
}

fn per_row(us: f64, empty: f64, reads: usize) -> f64 {
    ((us - empty).max(0.0) * 1000.0 / (ROWS * reads) as f64).round()
}

fn main() {
    let plain_only = std::env::args().nth(1).as_deref() == Some("plain");
    let tree = std::env::var("BENCH_TREE").unwrap_or_else(|_| "branch".into());
    let empty = run(PLAIN, "empty", None);
    println!("{tree} | plain column (Plain.hp) | {} ns a row", per_row(run(PLAIN, "plainColumn", None), empty, REPEAT));
    println!("{tree} | despawn, plain | {:.0} ns a row", despawn(PLAIN, "game::Plain"));
    println!("{tree} | plain Res read (Score.value) | {} ns a read", per_row(run(PLAIN, "plainRes", None), empty, REPEAT));
    println!("{tree} | plain Local read (n) | {} ns a read", per_row(run(PLAIN, "plainLocal", None), empty, REPEAT));
    println!("{tree} | plain message written and read (Ping.n), whole frame | {} ns a message", payload_ns(PLAIN_PAYLOADS, MESSAGES));
    println!("{tree} | plain event triggered and observed (Hit.n), whole frame | {} ns an event", payload_ns(PLAIN_PAYLOADS, EVENTS));
    if plain_only {
        return;
    }
    let empty = run(HEAP, "empty", None);
    let arms: [(&str, &str, Option<&'static str>, usize); 10] = [
        ("heap component's plain field (Tag.hp)", "heapPlainField", None, REPEAT),
        ("string hit (Tag.label)", "stringHit", None, REPEAT),
        ("string miss", "stringMiss", Some("game::Tag"), 1),
        ("string write", "stringWrite", None, 1),
        ("8-item f32[] hit (Tag.scores)", "listHit", None, REPEAT),
        ("8-item f32[] miss", "listMiss", Some("game::Tag"), 1),
        ("8-item f32[] write", "listWrite", None, 1),
        ("heap Res string read (Title.text, cached)", "resString", None, REPEAT),
        ("Local string read (text, cached)", "localString", None, REPEAT),
        ("empty system, µs a run (subtracted)", "empty", None, 0),
    ];
    for (label, system, miss, reads) in arms {
        if reads == 0 {
            println!("{tree} | {label} | {empty:.0}");
            continue;
        }
        println!("{tree} | {label} | {} ns a row", per_row(run(HEAP, system, miss), empty, reads));
    }
    println!("{tree} | despawn, a string and an 8-item list | {:.0} ns a row", despawn(HEAP, "game::Tag"));
    let empty_trail = run(HEAP, "emptyTrail", None);
    println!("{tree} | 8-item Vec3[] hit (Trail.points) | {} ns a row", per_row(run(HEAP, "vecHit", None), empty_trail, REPEAT));
    println!("{tree} | 8-item Vec3[] miss | {} ns a row", per_row(run(HEAP, "vecMiss", Some("game::Trail")), empty_trail, 1));
    let (read, skip) = (frames("read"), frames("skip"));
    println!("{tree} | On<Remove> value's string read (Tag.label) | {} ns an event", per_row(read, skip, 1));
    println!("{tree} | string message written and read (Chat.text), whole frame | {} ns a message", payload_ns(HEAP_PAYLOADS, MESSAGES));
    println!("{tree} | string event triggered and observed (Shout.text), whole frame | {} ns an event", payload_ns(HEAP_PAYLOADS, EVENTS));
}
