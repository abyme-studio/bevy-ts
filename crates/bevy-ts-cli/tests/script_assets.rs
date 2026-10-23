//! script-assets: meshes built and updated from TS through `ResMut<Assets<Mesh>>` and Bevy's
//! `Mesh` API, compared with the same calls in Rust. Headless, no GPU.

mod common;

use bevy::asset::{AssetEvent, RenderAssetUsages};
use bevy::ecs::message::Messages;
use bevy::mesh::{Indices, MeshPlugin, PrimitiveTopology, VertexAttributeValues};
use bevy::prelude::*;
use bevy_ts::BevyTsPlugin;
use bevy_ts::generation::DevSource;
use bevy_ts::host::HostParams;
use bevy_ts::reflect_asset::ScriptAssets;
use bevy_ts_cli::compile::Compiler;
use bevy_ts_native::prelude::BevyTsNativeExt;
use bevy_ts_v8::V8Options;
use common::*;
use std::sync::Arc;

const IMPORTS: &str = r#"import { app, Startup, Update } from "bevy-ts";
import type { Assets, AssetRef, MeshData, ResMut } from "bevy-ts";
import { Indices, Mesh, PrimitiveTopology, RenderAssetUsages } from "bevy-ts/assets";
"#;

/// A headless app with meshes and `Meshes`, its script `body` after [`IMPORTS`], loaded.
fn mesh_app(name: &str, body: &str) -> App {
    mesh_app_with(name, body, |_| {})
}

/// As [`mesh_app`], with `extra` added before the load.
fn mesh_app_with(name: &str, body: &str, extra: impl FnOnce(&mut App)) -> App {
    let dir = project(name, &[("game.ts", &format!("{IMPORTS}{body}"))]);
    let mut app = App::new();
    app.add_plugins((MinimalPlugins, AssetPlugin::default(), MeshPlugin))
        .add_plugins(BevyTsPlugin::new(bevy_ts_v8::factory(V8Options::default())).dev(true))
        .add_plugins(bevy_ts_cli::assets::ScriptAssetsPlugin);
    extra(&mut app);
    app.insert_resource(DevSource(Arc::new(Compiler::new(&dir.join("game.ts"), true).unwrap())));
    let r = reload(&mut app, false);
    assert!(r.errors.is_empty(), "load failed: {:?}", r.errors);
    app
}

/// The number after `prefix` in the script's log.
fn logged(app: &App, prefix: &str) -> String {
    let m = messages(app);
    m.iter().find_map(|l| l.strip_prefix(prefix)).unwrap_or_else(|| panic!("no {prefix:?} in {m:?}")).to_string()
}

/// The mesh behind the script's ref `n`.
fn mesh(app: &App, n: u64) -> Mesh {
    let h = app.world().resource::<ScriptAssets>().handle(n).unwrap().typed::<Mesh>();
    app.world().resource::<Assets<Mesh>>().get(&h).expect("the mesh is in Assets<Mesh>").clone()
}

fn ref_of(app: &App, prefix: &str) -> u64 {
    logged(app, prefix).trim().parse().unwrap()
}

/// Attribute by attribute, and the indices, the same data in the same formats.
fn assert_same(a: &Mesh, b: &Mesh) {
    assert_eq!(a.primitive_topology(), b.primitive_topology());
    assert_eq!(a.asset_usage, b.asset_usage);
    let ids = |m: &Mesh| m.attributes().map(|(a, _)| a.id).collect::<Vec<_>>();
    let (mut ia, mut ib) = (ids(a), ids(b));
    ia.sort();
    ib.sort();
    assert_eq!(ia, ib);
    for (attr, va) in a.attributes() {
        let vb = b.attribute(attr.id).unwrap();
        assert_eq!(VertexFormat::from(va), VertexFormat::from(vb), "{}", attr.name);
        assert_eq!(va.get_bytes(), vb.get_bytes(), "{}", attr.name);
    }
    assert_eq!(a.indices().map(|i| i.iter().collect::<Vec<_>>()), b.indices().map(|i| i.iter().collect::<Vec<_>>()));
}

use bevy::mesh::VertexFormat;

const QUAD_POSITIONS: [[f32; 3]; 4] = [[-1.0, -1.0, 0.0], [1.0, -1.0, 0.0], [1.0, 1.0, 0.0], [-1.0, 1.0, 0.0]];
const QUAD_UVS: [[f32; 2]; 4] = [[0.0, 1.0], [1.0, 1.0], [1.0, 0.0], [0.0, 0.0]];

#[test]
fn a_quad_matches_the_same_rust_calls() {
    let app = mesh_app(
        "quad",
        r#"
export function build(meshes: ResMut<Assets<Mesh>>) {
  const mesh = Mesh.new(PrimitiveTopology.TriangleList, RenderAssetUsages.default())
    .insertAttribute(Mesh.ATTRIBUTE_POSITION, new Float32Array([-1, -1, 0, 1, -1, 0, 1, 1, 0, -1, 1, 0]))
    .insertAttribute(Mesh.ATTRIBUTE_UV_0, new Float32Array([0, 1, 1, 1, 1, 0, 0, 0]))
    .insertIndices(Indices.U32(new Uint32Array([0, 1, 2, 0, 2, 3])))
    .computeNormals();
  const r = meshes.add(mesh);
  console.log(`quad ${r.$asset}`);
}
app.addSystems(Startup, build);
"#,
    );
    let mut app = app;
    updates(&mut app, 2);
    let ts = mesh(&app, ref_of(&app, "quad "));
    let mut rust = Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::default())
        .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, QUAD_POSITIONS.to_vec())
        .with_inserted_attribute(Mesh::ATTRIBUTE_UV_0, QUAD_UVS.to_vec())
        .with_inserted_indices(Indices::U32(vec![0, 1, 2, 0, 2, 3]));
    rust.compute_normals();
    assert_same(&ts, &rust);
}

#[test]
fn every_attribute_round_trips() {
    let mut app = mesh_app(
        "attributes",
        r#"
const attrs = [Mesh.ATTRIBUTE_POSITION, Mesh.ATTRIBUTE_NORMAL, Mesh.ATTRIBUTE_UV_0, Mesh.ATTRIBUTE_UV_1, Mesh.ATTRIBUTE_TANGENT, Mesh.ATTRIBUTE_COLOR, Mesh.ATTRIBUTE_JOINT_WEIGHT, Mesh.ATTRIBUTE_JOINT_INDEX];
// Three vertices; attribute k holds 100k + i at number i.
const data = attrs.map((a, k) => {
  const d = new a.array(3 * a.components);
  for (let i = 0; i < d.length; i++) d[i] = 100 * k + i;
  return d;
});
let ref: AssetRef<MeshData> | undefined;
export function build(meshes: ResMut<Assets<Mesh>>) {
  if (ref) return;
  const mesh = Mesh.new(PrimitiveTopology.TriangleList);
  attrs.forEach((a, k) => mesh.insertAttribute(a, data[k] as any));
  mesh.insertIndices(Indices.U16(new Uint16Array([0, 1, 2])));
  console.log(`before add ${mesh.countVertices()} ${mesh.attribute(Mesh.ATTRIBUTE_UV_1)?.join(",")}`);
  ref = meshes.add(mesh);
  console.log(`attributes ${ref.$asset}`);
  const live = meshes.get(ref)!;
  const same = attrs.map((a, k) => {
    const back = live.attribute(a)!;
    return back.constructor === data[k].constructor && back.length === data[k].length && back.every((v, i) => v === data[k][i]);
  });
  const ix = live.indices() as { U16: Uint16Array };
  console.log(`read ${same.join(",")} ${live.countVertices()} ${live.primitiveTopology} ${live.assetUsage} ${ix.U16.join(",")}`);
}
app.addSystems(Update, build);
"#,
    );
    updates(&mut app, 2);
    assert_eq!(logged(&app, "before add "), "3 300,301,302,303,304,305");
    assert_eq!(logged(&app, "read "), "true,true,true,true,true,true,true,true 3 TriangleList 3 0,1,2");
    let m = mesh(&app, ref_of(&app, "attributes "));
    let formats: Vec<(&str, VertexFormat)> = m.attributes().map(|(a, v)| (a.name, VertexFormat::from(v))).collect();
    for (attr, format) in [
        (Mesh::ATTRIBUTE_POSITION, VertexFormat::Float32x3),
        (Mesh::ATTRIBUTE_NORMAL, VertexFormat::Float32x3),
        (Mesh::ATTRIBUTE_UV_0, VertexFormat::Float32x2),
        (Mesh::ATTRIBUTE_UV_1, VertexFormat::Float32x2),
        (Mesh::ATTRIBUTE_TANGENT, VertexFormat::Float32x4),
        (Mesh::ATTRIBUTE_COLOR, VertexFormat::Float32x4),
        (Mesh::ATTRIBUTE_JOINT_WEIGHT, VertexFormat::Float32x4),
        (Mesh::ATTRIBUTE_JOINT_INDEX, VertexFormat::Uint16x4),
    ] {
        assert!(formats.contains(&(attr.name, format)), "{} {format:?} in {formats:?}", attr.name);
    }
    let VertexAttributeValues::Uint16x4(j) = m.attribute(Mesh::ATTRIBUTE_JOINT_INDEX).unwrap() else { panic!() };
    assert_eq!(j[2], [708, 709, 710, 711]);
}

/// A bumpy indexed grid: positions, UVs and indices, the same in TS and Rust.
fn grid() -> (Vec<[f32; 3]>, Vec<[f32; 2]>, Vec<u32>) {
    let n = 4;
    let mut p = Vec::new();
    let mut uv = Vec::new();
    for z in 0..=n {
        for x in 0..=n {
            p.push([x as f32, ((x * 7 + z * 3) % 5) as f32 * 0.25, z as f32]);
            uv.push([x as f32 / n as f32, z as f32 / n as f32]);
        }
    }
    let mut i = Vec::new();
    for z in 0..n {
        for x in 0..n {
            let a = z * (n + 1) + x;
            i.extend([a, a + n + 1, a + 1, a + 1, a + n + 1, a + n + 2]);
        }
    }
    (p, uv, i)
}

#[test]
fn computed_normals_match_rust() {
    let (p, uv, i) = grid();
    let flat = |v: &[f32]| v.iter().map(|f| f.to_string()).collect::<Vec<_>>().join(",");
    let body = format!(
        r#"
const P = new Float32Array([{}]);
const UV = new Float32Array([{}]);
const I = new Uint32Array([{}]);
const make = () => Mesh.new(PrimitiveTopology.TriangleList).insertAttribute(Mesh.ATTRIBUTE_POSITION, P).insertAttribute(Mesh.ATTRIBUTE_UV_0, UV).insertIndices(Indices.U32(I));
export function build(meshes: ResMut<Assets<Mesh>>) {{
  console.log(`normals ${{meshes.add(make().computeNormals()).$asset}}`);
  console.log(`smooth ${{meshes.add(make().computeSmoothNormals()).$asset}}`);
  console.log(`area ${{meshes.add(make().computeAreaWeightedNormals()).$asset}}`);
  console.log(`flat ${{meshes.add(make().duplicateVertices().computeFlatNormals()).$asset}}`);
  console.log(`tangents ${{meshes.add(make().computeSmoothNormals().generateTangents()).$asset}}`);
  // The same through a live mesh, one call per operation.
  const r = meshes.add(make());
  meshes.getMut(r)!.computeSmoothNormals().generateTangents();
  console.log(`live ${{r.$asset}} ${{meshes.get(r)!.countVertices()}}`);
  const d = meshes.add(make());
  meshes.getMut(d)!.duplicateVertices();
  console.log(`duplicated ${{d.$asset}} ${{meshes.get(d)!.countVertices()}} ${{meshes.get(d)!.indices()}}`);
}}
app.addSystems(Startup, build);
"#,
        flat(p.as_flattened()),
        flat(uv.as_flattened()),
        i.iter().map(|v| v.to_string()).collect::<Vec<_>>().join(",")
    );
    let mut app = mesh_app("normals", &body);
    updates(&mut app, 2);
    let make = || {
        Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::default())
            .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, p.clone())
            .with_inserted_attribute(Mesh::ATTRIBUTE_UV_0, uv.clone())
            .with_inserted_indices(Indices::U32(i.clone()))
    };
    let mut normals = make();
    normals.compute_normals();
    let mut smooth = make();
    smooth.compute_smooth_normals();
    let mut area = make();
    area.compute_area_weighted_normals();
    let mut flat_ = make();
    flat_.duplicate_vertices();
    flat_.compute_flat_normals();
    let mut tangents = make();
    tangents.compute_smooth_normals();
    tangents.generate_tangents().unwrap();
    let mut duplicated = make();
    duplicated.duplicate_vertices();
    for (prefix, rust) in [("normals ", &normals), ("smooth ", &smooth), ("area ", &area), ("flat ", &flat_), ("tangents ", &tangents)] {
        assert_same(&mesh(&app, ref_of(&app, prefix)), rust);
    }
    let live = logged(&app, "live ");
    let (n, count) = live.split_once(' ').unwrap();
    assert_same(&mesh(&app, n.parse().unwrap()), &tangents);
    assert_eq!(count, "25");
    let dup = logged(&app, "duplicated ");
    let parts: Vec<&str> = dup.split(' ').collect();
    assert_same(&mesh(&app, parts[0].parse().unwrap()), &duplicated);
    assert_eq!(parts[1..], ["96", "undefined"]);
}

#[test]
fn tangents_need_normals_and_uvs() {
    let mut app = mesh_app(
        "tangents",
        r#"
export function build(meshes: ResMut<Assets<Mesh>>) {
  const mesh = Mesh.new(PrimitiveTopology.TriangleList)
    .insertAttribute(Mesh.ATTRIBUTE_POSITION, new Float32Array([0, 0, 0, 1, 0, 0, 0, 1, 0]))
    .insertIndices(Indices.U16(new Uint16Array([0, 1, 2])));
  try {
    meshes.add(mesh.generateTangents());
  } catch (e) {
    console.log(`add threw ${(e as Error).message}`);
  }
  const r = meshes.add(Mesh.new(PrimitiveTopology.TriangleList).insertAttribute(Mesh.ATTRIBUTE_POSITION, new Float32Array([0, 0, 0, 1, 0, 0, 0, 1, 0])).insertIndices(Indices.U16(new Uint16Array([0, 1, 2]))));
  try {
    meshes.getMut(r)!.generateTangents();
  } catch (e) {
    console.log(`live threw ${(e as Error).message}`);
  }
  try {
    meshes.getMut(r)!.computeFlatNormals();
  } catch (e) {
    console.log(`flat threw ${(e as Error).message}`);
  }
  try {
    Mesh.new(PrimitiveTopology.LineList).insertAttribute(Mesh.ATTRIBUTE_POSITION, new Float32Array(6)).computeNormals();
    meshes.add(Mesh.new(PrimitiveTopology.LineList).insertAttribute(Mesh.ATTRIBUTE_POSITION, new Float32Array(6)).computeNormals());
  } catch (e) {
    console.log(`lines threw ${(e as Error).message}`);
  }
  console.log(`kept ${r.$asset}`);
}
app.addSystems(Startup, build);
"#,
    );
    updates(&mut app, 2);
    // Bevy's own `GenerateTangentsError` text, and the precondition Bevy asserts, as errors.
    assert!(logged(&app, "add threw ").contains("generateTangents: missing vertex attributes 'Vertex_Normal'"), "{}", logged(&app, "add threw "));
    assert!(logged(&app, "live threw ").contains("generateTangents: missing vertex attributes 'Vertex_Normal'"));
    assert!(logged(&app, "flat threw ").contains("computeFlatNormals: needs a mesh without indices"));
    assert!(logged(&app, "lines threw ").contains("computeNormals: needs PrimitiveTopology.TriangleList, not LineList"));
    // The failed adds added nothing; the live mesh is unchanged.
    assert_eq!(app.world().resource::<Assets<Mesh>>().len(), 1);
    let m = mesh(&app, ref_of(&app, "kept "));
    assert!(m.attribute(Mesh::ATTRIBUTE_TANGENT).is_none() && m.attribute(Mesh::ATTRIBUTE_NORMAL).is_none());
}

#[test]
fn a_wrong_array_throws() {
    let mut app = mesh_app(
        "wrong",
        r#"
const t = (what: string, f: () => void) => { try { f(); console.log(`${what} ok`); } catch (e) { console.log(`${what} threw ${(e as Error).message}`); } };
export function build(meshes: ResMut<Assets<Mesh>>) {
  t("ten", () => Mesh.new(PrimitiveTopology.TriangleList).insertAttribute(Mesh.ATTRIBUTE_POSITION, new Float32Array(10)));
  t("joint", () => Mesh.new(PrimitiveTopology.TriangleList).insertAttribute(Mesh.ATTRIBUTE_JOINT_INDEX, new Float32Array(4) as any));
  t("uv", () => Mesh.new(PrimitiveTopology.TriangleList).insertAttribute(Mesh.ATTRIBUTE_UV_0, new Uint16Array(4) as any));
  t("indices", () => Mesh.new(PrimitiveTopology.TriangleList).insertIndices({ U32: [0, 1, 2] } as any));
  const r = meshes.add(Mesh.new(PrimitiveTopology.TriangleList).insertAttribute(Mesh.ATTRIBUTE_POSITION, new Float32Array(9)));
  t("live ten", () => meshes.getMut(r)!.insertAttribute(Mesh.ATTRIBUTE_POSITION, new Float32Array(10)));
  t("read only", () => meshes.get(r)!.insertAttribute(Mesh.ATTRIBUTE_POSITION, new Float32Array(9)));
  t("out of range", () => meshes.getMut(r)!.insertIndices(Indices.U32(new Uint32Array([0, 1, 7]))).computeSmoothNormals());
}
app.addSystems(Startup, build);
"#,
    );
    updates(&mut app, 2);
    assert_eq!(logged(&app, "ten "), "threw insertAttribute: Mesh.ATTRIBUTE_POSITION takes a Float32Array of 3 per vertex; 10 is not a whole number of vertices");
    assert_eq!(logged(&app, "joint "), "threw insertAttribute: Mesh.ATTRIBUTE_JOINT_INDEX takes a Uint16Array of 4 per vertex, not Float32Array");
    assert_eq!(logged(&app, "uv "), "threw insertAttribute: Mesh.ATTRIBUTE_UV_0 takes a Float32Array of 2 per vertex, not Uint16Array");
    assert_eq!(logged(&app, "indices "), "threw insertIndices takes Indices.U16(Uint16Array) or Indices.U32(Uint32Array)");
    assert!(logged(&app, "live ten ").contains("Mesh.ATTRIBUTE_POSITION takes a Float32Array of 3 per vertex"));
    assert_eq!(logged(&app, "read only "), "threw insertAttribute: meshes.get gives a read-only mesh; use meshes.getMut");
    assert!(logged(&app, "out of range ").contains("computeSmoothNormals: an index is past the last vertex (3 vertices)"), "{}", logged(&app, "out of range "));
}

#[test]
fn render_world_only_data_cannot_be_edited() {
    let mut app = mesh_app(
        "render-world",
        r#"
let r: AssetRef<MeshData> | undefined;
export function build(meshes: ResMut<Assets<Mesh>>) {
  if (!r) {
    r = meshes.add(Mesh.new(PrimitiveTopology.TriangleList, RenderAssetUsages.RENDER_WORLD).insertAttribute(Mesh.ATTRIBUTE_POSITION, new Float32Array(9)));
    console.log(`added ${r.$asset}`);
    return;
  }
  try {
    meshes.getMut(r)!.insertAttribute(Mesh.ATTRIBUTE_POSITION, new Float32Array(9));
  } catch (e) {
    console.log(`threw ${(e as Error).message}`);
  }
}
app.addSystems(Update, build);
"#,
    );
    updates(&mut app, 2);
    // What the render world's extraction does to a RENDER_WORLD-only mesh.
    let h = app.world().resource::<ScriptAssets>().handle(ref_of(&app, "added ")).unwrap().typed::<Mesh>();
    app.world_mut().resource_mut::<Assets<Mesh>>().get_mut_untracked(&h).unwrap().take_gpu_data().unwrap();
    updates(&mut app, 2);
    assert!(logged(&app, "threw ").contains("insertAttribute: The mesh vertex/index data has been extracted to the RenderWorld"), "{}", logged(&app, "threw "));
}

const CHUNK: &str = r#"
// A chunk surface: `n` vertices of positions, normals and UVs, and `n * 1.5` indices.
const n = 24000;
const P = new Float32Array(n * 3), N = new Float32Array(n * 3), UV = new Float32Array(n * 2), I = new Uint32Array(n * 1.5);
for (let i = 0; i < I.length; i++) I[i] = i % n;
let r: AssetRef<MeshData> | undefined;
let frame = 0;
export function remesh(meshes: ResMut<Assets<Mesh>>) {
  frame++;
  if (!r) {
    r = meshes.add(Mesh.new(PrimitiveTopology.TriangleList).insertAttribute(Mesh.ATTRIBUTE_POSITION, P).insertAttribute(Mesh.ATTRIBUTE_NORMAL, N).insertAttribute(Mesh.ATTRIBUTE_UV_0, UV).insertIndices(Indices.U32(I)));
    console.log(`chunk ${r.$asset}`);
    return;
  }
  P[0] = frame;
  const m = meshes.getMut(r)!;
  m.insertAttribute(Mesh.ATTRIBUTE_POSITION, P).insertAttribute(Mesh.ATTRIBUTE_NORMAL, N).insertAttribute(Mesh.ATTRIBUTE_UV_0, UV).insertAttribute(Mesh.ATTRIBUTE_COLOR, new Float32Array(n * 4));
  m.insertIndices(Indices.U32(I));
}
app.addSystems(Update, remesh);
"#;

#[test]
fn a_mesh_update_is_seen_as_modified() {
    let mut app = mesh_app("modified", CHUNK);
    // Frame 1 adds the chunk, frame 2 remeshes it and pumps the log.
    updates(&mut app, 2);
    let h = app.world().resource::<ScriptAssets>().handle(ref_of(&app, "chunk ")).unwrap().typed::<Mesh>();
    let pos = |app: &App| app.world().resource::<Assets<Mesh>>().get(&h).unwrap().attribute(Mesh::ATTRIBUTE_POSITION).unwrap().get_bytes()[..4].to_vec();
    let ptr = |app: &App| match app.world().resource::<Assets<Mesh>>().get(&h).unwrap().attribute(Mesh::ATTRIBUTE_POSITION).unwrap() {
        VertexAttributeValues::Float32x3(v) => v.as_ptr() as usize,
        _ => unreachable!(),
    };
    let before = ptr(&app);
    let mut cursor = app.world().resource::<Messages<AssetEvent<Mesh>>>().get_cursor_current();
    app.update();
    // Five writes in one run, one `Modified`; the same-length position data went into the same `Vec`.
    let modified = cursor.read(app.world().resource::<Messages<AssetEvent<Mesh>>>()).filter(|e| e.is_modified(&h)).count();
    assert_eq!(modified, 1);
    assert_eq!(pos(&app), 3f32.to_le_bytes());
    assert_eq!(ptr(&app), before, "same length and format reuses the Vec");
    app.update();
    let modified = cursor.read(app.world().resource::<Messages<AssetEvent<Mesh>>>()).filter(|e| e.is_modified(&h)).count();
    assert_eq!(modified, 1, "and one again the next frame");
}

#[test]
fn a_chunk_remesh_is_six_host_calls() {
    let mut app = mesh_app("six-calls", CHUNK);
    app.update();
    app.update();
    let before = app.world().resource::<HostParams>().calls;
    app.update();
    // `getMut` (one `contains`), four `insertAttribute`, one `insertIndices`.
    assert_eq!(app.world().resource::<HostParams>().calls - before, 6);
}

/// A native crate's export that makes a mesh and reads one, through `ScriptRef<Mesh>`.
#[derive(bevy::ecs::system::SystemParam)]
pub struct Shapes<'w> {
    meshes: ResMut<'w, Assets<Mesh>>,
    refs: Res<'w, ScriptAssets>,
}

#[bevy_ts_native::export(ts = "Shapes", module = "bevy-ts/shapes-test")]
impl Shapes<'_> {
    pub fn triangle(&mut self) -> bevy_ts::host::ScriptRef<Mesh> {
        let mesh = Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::default()).with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, vec![[0.0f32; 3]; 3]);
        bevy_ts::host::ScriptRef::new(self.refs.insert_strong(self.meshes.add(mesh).untyped()))
    }

    pub fn vertices(&self, mesh: bevy_ts::host::ScriptRef<Mesh>) -> Result<u32, String> {
        let h = mesh.handle(&self.refs)?;
        Ok(self.meshes.get(&h).ok_or("gone")?.count_vertices() as u32)
    }
}

#[test]
fn a_native_crate_takes_and_returns_mesh_refs() {
    let mut app = mesh_app_with(
        "native-refs",
        r#"
import type { Shapes } from "bevy-ts/shapes-test";
export function build(shapes: Shapes, meshes: ResMut<Assets<Mesh>>) {
  const t = shapes.triangle();
  const own = meshes.add(Mesh.new(PrimitiveTopology.TriangleList).insertAttribute(Mesh.ATTRIBUTE_POSITION, new Float32Array(12)));
  console.log(`refs ${t.type} ${Object.isFrozen(t)} ${shapes.vertices(t)} ${shapes.vertices(own)} ${meshes.get(t)!.countVertices()}`);
  try {
    shapes.vertices({ $asset: t.$asset, type: "bevy_image::image::Image" } as any);
  } catch (e) {
    console.log(`wrong ${(e as Error).message}`);
  }
}
app.addSystems(Startup, build);
"#,
        |app| {
            app.ts_param::<Shapes<'static>>();
        },
    );
    updates(&mut app, 2);
    assert_eq!(logged(&app, "refs "), "bevy_mesh::mesh::Mesh true 3 4 3");
    assert_eq!(logged(&app, "wrong "), "Shapes.vertices: expected an AssetRef<bevy_mesh::mesh::Mesh>, got an AssetRef<bevy_image::image::Image>");
}
