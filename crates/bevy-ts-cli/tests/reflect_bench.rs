//! What a reflected read costs in a TS system (reflect-coverage task 3.7), over 1,000 rows of
//! `DirectionalLight.color` (a hole) and `Projection` (reflect tier): reads on cache hits, reads on
//! misses through the codec, the JSON path (`read`'s JSON parsed) as the baseline, and writes;
//! against a layout-tier column read (`Transform.translation.x`). Each arm is one system; its time
//! is the system's run time less an empty system's over the same rows, per row, median of the
//! measured frames. Ignored by default; run it in release:
//! `cargo test --release -p bevy-ts-cli --test reflect_bench -- --ignored --nocapture`.

mod common;

use bevy::camera::{PerspectiveProjection, Projection};
use bevy::light::DirectionalLight;
use bevy::prelude::*;
use bevy_ts::systems::TsSystems;
use common::*;

const ROWS: usize = 1000;
/// Hit and column arms read each row this many times a run, for timer resolution.
const REPEAT: usize = 10;
const FRAMES: usize = 200;

const GAME: &str = r##"import { __reflectJson, __reflectTarget, app, chain, rustComponent, Update } from "bevy-ts";
import { DirectionalLight, Transform } from "bevy-ts/bevy";
import type { Entity, Mut, Query } from "bevy-ts";
const Projection = rustComponent("Projection");
const REPEAT = __REPEAT__;
let sink = 0;
export function empty(q: Query<[Transform, DirectionalLight, Projection]>) { q.each(() => { sink++; }); }
export function column(q: Query<[Transform]>) {
  for (let k = 0; k < REPEAT; k++) q.each((t) => { sink += t.translation.x; });
}
export function hitColor(q: Query<[DirectionalLight]>) {
  for (let k = 0; k < REPEAT; k++) q.each((l: any) => { if (l.color) sink++; });
}
export function hitProjection(q: Query<[Projection]>) {
  for (let k = 0; k < REPEAT; k++) q.each((p: any) => { if (p.value) sink++; });
}
export function missColor(q: Query<[DirectionalLight]>) { q.each((l: any) => { if (l.color) sink++; }); }
export function missProjection(q: Query<[Projection]>) { q.each((p: any) => { if (p.value) sink++; }); }
export function jsonColor(q: Query<[Entity, DirectionalLight]>) {
  const t = __reflectTarget(DirectionalLight, "color");
  q.each((e) => { if (__reflectJson(t, e)) sink++; });
}
export function jsonProjection(q: Query<[Entity, Projection]>) {
  const t = __reflectTarget(Projection);
  q.each((e) => { if (__reflectJson(t, e)) sink++; });
}
export function writeColor(q: Query<[Mut<DirectionalLight>]>) { q.each((l: any) => { l.color = "#ff8000"; }); }
export function writeProjection(q: Query<[Mut<Projection>]>) { q.each((p: any) => { p.value = { Perspective: { fov: 0.9 } }; }); }
let zoom = 0;
export function zoomOne(q: Query<[Entity, Mut<Projection>]>) {
  // One camera's zoom a frame (agent town): one Mut write, the other rows untouched.
  zoom = (zoom + 1) % 100;
  q.each((e, p: any) => { if (e === first) p.value = { Orthographic: { scale: 1 + zoom / 100 } }; });
}
export function emptyOne(q: Query<[Entity, Mut<Projection>]>) { q.each((e) => { if (e === first) sink++; }); }
let first = -1;
export function pickFirst(q: Query<[Entity, Projection]>) { if (first < 0) q.each((e) => { if (first < 0) first = e; }); }
app.addSystems(Update, chain(pickFirst, __SYSTEM__));
"##;

/// Marks every row changed before the TS stage, so each read misses.
fn touch(mut q: Query<(&mut DirectionalLight, &mut Projection)>) {
    for (mut l, mut p) in &mut q {
        l.set_changed();
        p.set_changed();
    }
}

/// Median run time in microseconds of `system` over `FRAMES` frames, after a warm-up.
fn run(system: &str, misses: bool) -> f64 {
    let src = GAME.replace("__REPEAT__", &REPEAT.to_string()).replace("__SYSTEM__", system);
    let dir = project(&format!("reflect-bench-{system}"), &[("game.ts", &src)]);
    let mut app = app(&dir.join("game.ts"));
    app.register_type::<Projection>().register_type::<DirectionalLight>();
    if misses {
        app.add_systems(Update, touch.before(bevy_ts::BevyTsSet::Update));
    }
    for i in 0..ROWS {
        app.world_mut().spawn((Transform::from_xyz(i as f32, 0.0, 0.0), DirectionalLight::default(), Projection::Perspective(PerspectiveProjection::default())));
    }
    app.update();
    let r = reload(&mut app, false);
    assert!(r.errors.is_empty(), "{system}: {:?}", r.errors);
    updates(&mut app, 20);
    let mut us = Vec::with_capacity(FRAMES);
    for _ in 0..FRAMES {
        app.update();
        let systems = app.world().resource::<TsSystems>();
        let s = systems.iter().find(|s| s.decl.name.rsplit('#').next() == Some(system)).expect("system");
        assert!(s.last_error.is_none(), "{system}: {:?}", s.last_error);
        us.push(s.last_run_us as f64);
    }
    us.sort_by(f64::total_cmp);
    us[FRAMES / 2]
}

#[test]
#[ignore]
fn reflected_reads_per_row() {
    let empty = run("empty", false);
    let per_row = |us: f64, repeat: usize| ((us - empty).max(0.0) * 1000.0 / (ROWS * repeat) as f64).round();
    let arms = [
        ("layout column (Transform.translation.x)", run("column", false), REPEAT),
        ("hit, DirectionalLight.color", run("hitColor", false), REPEAT),
        ("hit, Projection", run("hitProjection", false), REPEAT),
        ("miss through the codec, DirectionalLight.color", run("missColor", true), 1),
        ("miss through the codec, Projection", run("missProjection", true), 1),
        ("JSON baseline, DirectionalLight.color", run("jsonColor", false), 1),
        ("JSON baseline, Projection", run("jsonProjection", false), 1),
        ("write, DirectionalLight.color", run("writeColor", false), 1),
        ("write, Projection", run("writeProjection", false), 1),
    ];
    println!("| Arm | ns per row |\n|---|---|");
    for (name, us, repeat) in arms {
        println!("| {name} | {} |", per_row(us, repeat));
    }
    println!("(empty system over the same {ROWS} rows: {empty} µs a run, subtracted)");
    // One reflected write a frame on one of the rows, less the same loop without the write.
    let one = run("zoomOne", false) - run("emptyOne", false);
    println!("| one Mut<Projection> write a frame (agent town's zoom) | {} ns a frame |", (one.max(0.0) * 1000.0).round());
}
