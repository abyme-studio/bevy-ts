//! What a chunk remesh costs from TS beside the same copies in Rust (script-assets design
//! decision 5). A 24,000-vertex chunk surface: positions, normals, UVs, colours and `U32` indices.
//!
//! - (a) Rust: `Assets::get_mut` and the same copies into the mesh's own `Vec`s, the floor.
//! - (b) TS: `meshes.getMut` and four `insertAttribute` and one `insertIndices`, same lengths.
//! - (c) TS: as (b), the vertex count changing every frame, so every buffer is allocated.
//! - (d) TS: `Mesh.new` and `meshes.add` of a whole chunk, the old one dropped (per chunk).
//! - (0) TS: the same system with nothing to do, the frame's own cost, subtracted from each arm.
//!
//! Each arm runs 3 times, 200 frames after 20 to warm up, in a fresh app. Ignored by default; run
//! with `cargo test --release -p bevy-ts-cli --test mesh_update_bench -- --ignored --nocapture`.

mod common;

use bevy::asset::RenderAssetUsages;
use bevy::mesh::{Indices, MeshPlugin, PrimitiveTopology, VertexAttributeValues};
use bevy::prelude::*;
use bevy_ts::BevyTsPlugin;
use bevy_ts::generation::DevSource;
use bevy_ts_cli::compile::Compiler;
use bevy_ts_v8::V8Options;
use common::*;
use std::sync::Arc;
use std::time::Instant;

const N: usize = 24_000;
const FRAMES: usize = 200;
const WARM: usize = 20;
const RUNS: usize = 3;

const TS: &str = r#"import { app, assets, Update } from "bevy-ts";
import type { Assets, AssetRef, MeshData, ResMut } from "bevy-ts";
import { Indices, Mesh, PrimitiveTopology } from "bevy-ts/assets";

const ARM = "__ARM__";
const n = 24000;
const P = new Float32Array(n * 3), N = new Float32Array(n * 3), UV = new Float32Array(n * 2), C = new Float32Array(n * 4), I = new Uint32Array(n * 1.5);
for (let i = 0; i < P.length; i++) P[i] = i * 0.001;
for (let i = 0; i < I.length; i++) I[i] = i % n;
// Six vertices fewer, for the arm whose count changes.
const P2 = P.subarray(0, (n - 6) * 3), N2 = N.subarray(0, (n - 6) * 3), UV2 = UV.subarray(0, (n - 6) * 2), C2 = C.subarray(0, (n - 6) * 4), I2 = I.subarray(0, (n - 6) * 1.5);
const full = () => Mesh.new(PrimitiveTopology.TriangleList).insertAttribute(Mesh.ATTRIBUTE_POSITION, P).insertAttribute(Mesh.ATTRIBUTE_NORMAL, N).insertAttribute(Mesh.ATTRIBUTE_UV_0, UV).insertAttribute(Mesh.ATTRIBUTE_COLOR, C).insertIndices(Indices.U32(I));
let r: AssetRef<MeshData> | undefined;
let frame = 0;
export function remesh(meshes: ResMut<Assets<Mesh>>) {
  frame++;
  if (!r) {
    r = meshes.add(full());
    return;
  }
  if (ARM === "empty") return;
  P[0] = frame;
  if (ARM === "same") {
    meshes.getMut(r)!.insertAttribute(Mesh.ATTRIBUTE_POSITION, P).insertAttribute(Mesh.ATTRIBUTE_NORMAL, N).insertAttribute(Mesh.ATTRIBUTE_UV_0, UV).insertAttribute(Mesh.ATTRIBUTE_COLOR, C).insertIndices(Indices.U32(I));
  } else if (ARM === "resize") {
    const odd = frame % 2 === 1;
    meshes.getMut(r)!.insertAttribute(Mesh.ATTRIBUTE_POSITION, odd ? P2 : P).insertAttribute(Mesh.ATTRIBUTE_NORMAL, odd ? N2 : N).insertAttribute(Mesh.ATTRIBUTE_UV_0, odd ? UV2 : UV).insertAttribute(Mesh.ATTRIBUTE_COLOR, odd ? C2 : C).insertIndices(Indices.U32(odd ? I2 : I));
  } else if (ARM === "add") {
    const next = meshes.add(full());
    assets.drop(r);
    r = next;
  }
}
app.addSystems(Update, remesh);
"#;

/// The chunk's data for the Rust arm.
#[derive(Resource)]
struct Chunk {
    mesh: Handle<Mesh>,
    p: Vec<[f32; 3]>,
    n: Vec<[f32; 3]>,
    uv: Vec<[f32; 2]>,
    c: Vec<[f32; 4]>,
    i: Vec<u32>,
    frame: u32,
}

fn rust_remesh(mut chunk: ResMut<Chunk>, mut meshes: ResMut<Assets<Mesh>>) {
    chunk.frame += 1;
    chunk.p[0][0] = chunk.frame as f32;
    let chunk = &*chunk;
    let mut m = meshes.get_mut(&chunk.mesh).unwrap();
    let copy = |m: &mut Mesh, attr, src: &[f32]| match m.attribute_mut(attr).unwrap() {
        VertexAttributeValues::Float32x2(v) => v.as_flattened_mut().copy_from_slice(src),
        VertexAttributeValues::Float32x3(v) => v.as_flattened_mut().copy_from_slice(src),
        VertexAttributeValues::Float32x4(v) => v.as_flattened_mut().copy_from_slice(src),
        _ => unreachable!(),
    };
    copy(&mut m, Mesh::ATTRIBUTE_POSITION, chunk.p.as_flattened());
    copy(&mut m, Mesh::ATTRIBUTE_NORMAL, chunk.n.as_flattened());
    copy(&mut m, Mesh::ATTRIBUTE_UV_0, chunk.uv.as_flattened());
    copy(&mut m, Mesh::ATTRIBUTE_COLOR, chunk.c.as_flattened());
    let Some(Indices::U32(i)) = m.indices_mut() else { unreachable!() };
    i.copy_from_slice(&chunk.i);
}

fn base_app() -> App {
    let mut app = App::new();
    app.add_plugins((MinimalPlugins, AssetPlugin::default(), MeshPlugin));
    app
}

fn ts_app(arm: &str) -> App {
    let dir = project(&format!("mesh-bench-{arm}"), &[("game.ts", &TS.replace("__ARM__", arm))]);
    let mut app = base_app();
    app.add_plugins(BevyTsPlugin::new(bevy_ts_v8::factory(V8Options::default())).dev(true)).add_plugins(bevy_ts_cli::assets::ScriptAssetsPlugin);
    app.insert_resource(DevSource(Arc::new(Compiler::new(&dir.join("game.ts"), true).unwrap())));
    let r = reload(&mut app, false);
    assert!(r.errors.is_empty(), "{:?}", r.errors);
    app
}

fn rust_app() -> App {
    let mut app = base_app();
    let p: Vec<[f32; 3]> = (0..N).map(|i| [i as f32 * 0.003, 0.0, 0.0]).collect();
    let n = vec![[0.0; 3]; N];
    let uv = vec![[0.0; 2]; N];
    let c = vec![[0.0; 4]; N];
    let i: Vec<u32> = (0..N * 3 / 2).map(|i| (i % N) as u32).collect();
    let mesh = Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::default())
        .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, p.clone())
        .with_inserted_attribute(Mesh::ATTRIBUTE_NORMAL, n.clone())
        .with_inserted_attribute(Mesh::ATTRIBUTE_UV_0, uv.clone())
        .with_inserted_attribute(Mesh::ATTRIBUTE_COLOR, c.clone())
        .with_inserted_indices(Indices::U32(i.clone()));
    let mesh = app.world_mut().resource_mut::<Assets<Mesh>>().add(mesh);
    app.insert_resource(Chunk { mesh, p, n, uv, c, i, frame: 0 }).add_systems(Update, rust_remesh);
    app
}

/// Frame times in µs of one run.
fn run(mut app: App) -> Vec<f64> {
    for _ in 0..WARM {
        app.update();
    }
    (0..FRAMES)
        .map(|_| {
            let t = Instant::now();
            app.update();
            t.elapsed().as_secs_f64() * 1e6
        })
        .collect()
}

fn pct(sorted: &[f64], p: f64) -> f64 {
    sorted[((sorted.len() - 1) as f64 * p).round() as usize]
}

/// Median and p95 µs over every run's frames.
fn arm(make: &dyn Fn() -> App) -> (f64, f64) {
    let mut all: Vec<f64> = (0..RUNS).flat_map(|_| run(make())).collect();
    all.sort_by(|a, b| a.partial_cmp(b).unwrap());
    (pct(&all, 0.5), pct(&all, 0.95))
}

#[test]
#[ignore = "a benchmark: run by hand in release"]
fn chunk_remesh_costs() {
    let (rust_frame, _) = arm(&|| {
        let mut app = base_app();
        app.add_systems(Update, || {});
        app
    });
    let (empty, _) = arm(&|| ts_app("empty"));
    let arms: [(&str, &str, f64, Box<dyn Fn() -> App>); 4] = [
        ("a", "Rust get_mut + copies (floor)", rust_frame, Box::new(rust_app)),
        ("b", "TS getMut + 4 insertAttribute + insertIndices, same length", empty, Box::new(|| ts_app("same"))),
        ("c", "TS, vertex count changing each frame", empty, Box::new(|| ts_app("resize"))),
        ("d", "TS Mesh.new + meshes.add, old dropped", empty, Box::new(|| ts_app("add"))),
    ];
    println!("chunk remesh, {N} vertices, {RUNS} runs x {FRAMES} frames; µs per remesh, frame cost without remesh subtracted");
    println!("frame alone: Rust {rust_frame:.1} µs, TS {empty:.1} µs");
    println!("| arm | what | median µs | p95 µs | ns/vertex |");
    println!("|---|---|---|---|---|");
    let mut medians = Vec::new();
    for (k, what, base, make) in arms {
        let (med, p95) = arm(&*make);
        let (med, p95) = (med - base, p95 - base);
        medians.push(med);
        println!("| {k} | {what} | {med:.1} | {p95:.1} | {:.2} |", med * 1000.0 / N as f64);
    }
    let target = 1.5 * medians[0] + 5.0;
    println!("target: (b) <= 1.5 x (a) + 5 µs = {target:.1} µs; (b) is {:.1} µs: {}", medians[1], if medians[1] <= target { "met" } else { "missed" });
}
