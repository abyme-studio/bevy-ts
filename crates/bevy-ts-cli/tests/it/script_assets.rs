//! script-assets: loads and their load state through `Res<AssetServer>`, strong refs dropped and
//! removed, and meshes built and updated from TS through `ResMut<Assets<Mesh>>` and Bevy's `Mesh`
//! API, compared with the same calls in Rust. Headless; only the glTF test needs the GPU.

use crate::common;

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

const IMPORTS: &str = r#"import { app, asset, assets, Startup, Update } from "bevy-ts";
import type { AssetServer, Assets, AssetRef, MeshData, Res, ResMut } from "bevy-ts";
import { Indices, Mesh, PrimitiveTopology, RenderAssetUsages } from "bevy-ts/assets";
"#;

/// A headless app with meshes and `Meshes`, its script `body` after [`IMPORTS`], loaded.
fn mesh_app(name: &str, body: &str) -> App {
    mesh_app_with(name, body, |_| {})
}

/// As [`mesh_app`], with `extra` added before the load.
fn mesh_app_with(name: &str, body: &str, extra: impl FnOnce(&mut App)) -> App {
    assets_app(name, body, &[], extra).1
}

/// The project folder, with `files` beside `game.ts` (the asset root), and its loaded app.
fn assets_app(name: &str, body: &str, files: &[(&str, &[u8])], extra: impl FnOnce(&mut App)) -> (std::path::PathBuf, App) {
    let dir = project(name, &[("game.ts", &format!("{IMPORTS}{body}"))]);
    for (f, bytes) in files {
        std::fs::write(dir.join(f), bytes).unwrap();
    }
    let mut app = App::new();
    let root = AssetPlugin { file_path: dir.to_string_lossy().into_owned(), ..default() };
    app.add_plugins((MinimalPlugins, root, MeshPlugin, bevy::image::ImagePlugin::default()))
        .add_plugins(BevyTsPlugin::new(bevy_ts_v8::factory(V8Options::default())).dev(true))
        .add_plugins(bevy_ts_cli::assets::ScriptAssetsPlugin);
    extra(&mut app);
    app.insert_resource(DevSource(Arc::new(Compiler::new(&dir.join("game.ts"), true).unwrap())));
    let r = reload(&mut app, false);
    assert!(r.errors.is_empty(), "load failed: {:?}", r.errors);
    (dir, app)
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

// ---------- handles and load state ----------

/// A 2x2 RGBA PNG: zlib with one stored block, so no encoder is needed.
fn png() -> Vec<u8> {
    fn crc(bytes: &[u8]) -> u32 {
        let mut c = 0xffff_ffffu32;
        for &b in bytes {
            c ^= b as u32;
            for _ in 0..8 {
                c = if c & 1 != 0 { 0xedb8_8320 ^ (c >> 1) } else { c >> 1 };
            }
        }
        !c
    }
    fn chunk(out: &mut Vec<u8>, kind: &[u8], data: &[u8]) {
        out.extend((data.len() as u32).to_be_bytes());
        let mut body = kind.to_vec();
        body.extend(data);
        out.extend(&body);
        out.extend(crc(&body).to_be_bytes());
    }
    let raw: Vec<u8> = (0..2).flat_map(|_| [0u8, 255, 0, 0, 255, 0, 255, 0, 255]).collect();
    let (mut a, mut b) = (1u32, 0u32);
    for &x in &raw {
        a = (a + x as u32) % 65521;
        b = (b + a) % 65521;
    }
    let mut z = vec![0x78, 0x01, 0x01];
    z.extend((raw.len() as u16).to_le_bytes());
    z.extend((!(raw.len() as u16)).to_le_bytes());
    z.extend(&raw);
    z.extend(((b << 16) | a).to_be_bytes());
    let mut out = b"\x89PNG\r\n\x1a\n".to_vec();
    let mut ihdr = Vec::new();
    ihdr.extend(2u32.to_be_bytes());
    ihdr.extend(2u32.to_be_bytes());
    ihdr.extend([8, 6, 0, 0, 0]);
    chunk(&mut out, b"IHDR", &ihdr);
    chunk(&mut out, b"IDAT", &z);
    chunk(&mut out, b"IEND", &[]);
    out
}

/// Runs frames until the script logs `prefix`, up to 200.
fn until_logged(app: &mut App, prefix: &str) -> String {
    for _ in 0..200 {
        app.update();
        if let Some(l) = messages(app).iter().find_map(|l| l.strip_prefix(prefix)) {
            return l.to_string();
        }
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
    panic!("no {prefix:?} in {:?}", messages(app));
}

#[test]
fn a_texture_loads_and_reaches_loaded() {
    let png = png();
    let (_, mut app) = assets_app(
        "load-texture",
        r#"
let r: AssetRef<unknown> | undefined;
let first = true;
export function watch(server: Res<AssetServer>) {
  if (!r) {
    r = server.load("grid.png");
    const again = server.load("grid.png");
    console.log(`loading ${r.$asset} ${r.type} ${again.$asset === r.$asset}`);
  }
  const state = server.loadState(r);
  if (first) {
    first = false;
    console.log(`first ${JSON.stringify(state)} ${server.isLoadedWithDependencies(r)}`);
  }
  if (state === "Loaded") console.log(`loaded ${server.isLoaded(r)} ${server.isLoadedWithDependencies(r)} ${JSON.stringify(server.recursiveDependencyLoadState(r))}`);
}
app.addSystems(Update, watch);
"#,
        &[("grid.png", &png)],
        |_| {},
    );
    let done = until_logged(&mut app, "loaded ");
    assert_eq!(done, "true true \"Loaded\"");
    let loading = logged(&app, "loading ");
    let parts: Vec<&str> = loading.split(' ').collect();
    assert_eq!(parts[1..], ["bevy_image::image::Image", "true"]);
    assert_eq!(logged(&app, "first "), "\"Loading\" false");
    let h = app.world().resource::<ScriptAssets>().handle(parts[0].parse().unwrap()).unwrap().typed::<Image>();
    assert_eq!(app.world().resource::<Assets<Image>>().get(&h).unwrap().size(), UVec2::new(2, 2));
}

#[test]
fn a_missing_file_fails_with_bevys_error() {
    let (_, mut app) = assets_app(
        "load-missing",
        r#"
let r: AssetRef<unknown> | undefined;
export function watch(server: Res<AssetServer>) {
  r ??= server.load("missing.png");
  const s = server.loadState(r);
  if (typeof s === "object") console.log(`failed ${s.Failed} | ${JSON.stringify(server.recursiveDependencyLoadState(r))}`);
}
app.addSystems(Update, watch);
"#,
        &[],
        |_| {},
    );
    let failed = until_logged(&mut app, "failed ");
    assert!(failed.contains("missing.png"), "{failed}");
    assert!(failed.contains("| {\"Failed\":"), "{failed}");
    app.update();
}

#[test]
fn a_labelled_path_needs_a_type() {
    let mut app = mesh_app(
        "load-labelled",
        r#"
export function load(server: Res<AssetServer>) {
  try {
    server.load("models/fox.glb#Mesh0/Primitive0");
  } catch (e) {
    console.log(`threw ${(e as Error).message}`);
  }
  const r = server.load("models/fox.glb#Mesh0/Primitive0", Mesh);
  console.log(`typed ${r.type}`);
}
app.addSystems(Startup, load);
"#,
    );
    updates(&mut app, 2);
    assert!(logged(&app, "threw ").contains("load: \"models/fox.glb#Mesh0/Primitive0\" names a labelled asset; pass its type"), "{}", logged(&app, "threw "));
    assert_eq!(logged(&app, "typed "), "bevy_mesh::mesh::Mesh");
}

/// `AssetEvent<Mesh>`s sent since `cursor`, for `id`, by kind.
fn mesh_events(app: &App, cursor: &mut bevy::ecs::message::MessageCursor<AssetEvent<Mesh>>, id: AssetId<Mesh>) -> Vec<&'static str> {
    cursor
        .read(app.world().resource::<Messages<AssetEvent<Mesh>>>())
        .filter_map(|e| match e {
            AssetEvent::Added { id: i } if *i == id => Some("Added"),
            AssetEvent::Modified { id: i } if *i == id => Some("Modified"),
            AssetEvent::Removed { id: i } if *i == id => Some("Removed"),
            AssetEvent::Unused { id: i } if *i == id => Some("Unused"),
            AssetEvent::LoadedWithDependencies { id: i } if *i == id => Some("LoadedWithDependencies"),
            _ => None,
        })
        .collect()
}

const TRIANGLE: &str = r#"
const tri = () => Mesh.new(PrimitiveTopology.TriangleList).insertAttribute(Mesh.ATTRIBUTE_POSITION, new Float32Array(9));
"#;

#[test]
fn a_dropped_mesh_nothing_holds_is_freed() {
    let mut app = mesh_app(
        "drop-free",
        &format!(
            r#"{TRIANGLE}
let r: AssetRef<MeshData> | undefined;
let frame = 0;
export function run(meshes: ResMut<Assets<Mesh>>) {{
  frame++;
  if (frame === 1) {{ r = meshes.add(tri()); console.log(`added ${{r.$asset}}`); }}
  if (frame === 3) assets.drop(r!);
  if (frame === 4) {{ try {{ meshes.get(r!); }} catch (e) {{ console.log(`stale ${{(e as Error).message}}`); }} }}
}}
app.addSystems(Update, run);
"#
        ),
    );
    updates(&mut app, 2);
    let n = ref_of(&app, "added ");
    let id = app.world().resource::<ScriptAssets>().handle(n).unwrap().typed::<Mesh>().id();
    let mut cursor = app.world().resource::<Messages<AssetEvent<Mesh>>>().get_cursor_current();
    updates(&mut app, 4);
    assert!(app.world().resource::<Assets<Mesh>>().get(id).is_none(), "freed");
    assert_eq!(mesh_events(&app, &mut cursor, id), ["Unused", "Removed"]);
    assert!(logged(&app, "stale ").contains(&format!("stale asset ref {n}")), "{}", logged(&app, "stale "));
}

#[test]
fn a_dropped_mesh_an_entity_draws_stays() {
    let mut app = mesh_app(
        "drop-held",
        &format!(
            r#"{TRIANGLE}
let r: AssetRef<MeshData> | undefined;
let frame = 0;
export function run(meshes: ResMut<Assets<Mesh>>) {{
  frame++;
  if (frame === 1) {{ r = meshes.add(tri()); console.log(`added ${{r.$asset}}`); }}
  if (frame === 3) assets.drop(r!);
}}
app.addSystems(Update, run);
"#
        ),
    );
    updates(&mut app, 2);
    // What a `<Mesh3d value={ref} />` holds: a clone of the strong handle.
    let h = app.world().resource::<ScriptAssets>().handle(ref_of(&app, "added ")).unwrap().typed::<Mesh>();
    let e = app.world_mut().spawn(Mesh3d(h.clone())).id();
    let id = h.id();
    drop(h);
    updates(&mut app, 4);
    assert!(app.world().resource::<Assets<Mesh>>().get(id).is_some(), "kept while the entity holds it");
    app.world_mut().despawn(e);
    updates(&mut app, 2);
    assert!(app.world().resource::<Assets<Mesh>>().get(id).is_none(), "freed once the entity is gone");
}

#[test]
fn remove_frees_at_once() {
    let mut app = mesh_app(
        "remove",
        &format!(
            r#"{TRIANGLE}
let r: AssetRef<MeshData> | undefined;
let r2: AssetRef<MeshData> | undefined;
let frame = 0;
export function run(meshes: ResMut<Assets<Mesh>>) {{
  frame++;
  if (frame === 1) {{ r = meshes.add(tri()); r2 = meshes.add(tri()); console.log(`added ${{r.$asset}} ${{r2.$asset}}`); }}
  if (frame === 3) {{ assets.remove(r!); console.log(`removed ${{meshes.remove(r2!)}}`); }}
}}
app.addSystems(Update, run);
"#
        ),
    );
    updates(&mut app, 2);
    let added = logged(&app, "added ");
    let refs = app.world().resource::<ScriptAssets>().clone();
    let held: Vec<Handle<Mesh>> = added.split(' ').map(|n| refs.handle(n.parse().unwrap()).unwrap().typed()).collect();
    let mut cursor = app.world().resource::<Messages<AssetEvent<Mesh>>>().get_cursor_current();
    // An entity holds both: `remove` frees them anyway, the frame it runs.
    app.world_mut().spawn((Mesh3d(held[0].clone()),));
    app.world_mut().spawn((Mesh3d(held[1].clone()),));
    app.update();
    for h in &held {
        assert!(app.world().resource::<Assets<Mesh>>().get(h).is_none(), "removed the same frame");
    }
    app.update();
    assert_eq!(mesh_events(&app, &mut cursor, held[0].id()), ["Removed"]);
    assert_eq!(logged(&app, "removed "), "true");
}

#[test]
fn a_stale_ref_throws() {
    let mut app = mesh_app(
        "stale",
        &format!(
            r#"{TRIANGLE}
export function run(meshes: ResMut<Assets<Mesh>>) {{
  const r = meshes.add(tri());
  assets.drop(r);
  for (const [what, f] of [["getMut", () => meshes.getMut(r)], ["drop", () => assets.drop(r)], ["remove", () => meshes.remove(r)]] as const) {{
    try {{ f(); console.log(`${{what}} ok`); }} catch (e) {{ console.log(`${{what}} threw ${{(e as Error).message}}`); }}
  }}
}}
app.addSystems(Startup, run);
"#
        ),
    );
    updates(&mut app, 2);
    for what in ["getMut ", "drop ", "remove "] {
        assert!(logged(&app, what).contains("threw") && logged(&app, what).contains("stale asset ref 1"), "{what}{}", logged(&app, what));
    }
}

#[test]
fn hot_reload_frees_what_only_the_old_script_held() {
    let body = format!(
        r#"{TRIANGLE}
export function run(meshes: ResMut<Assets<Mesh>>) {{
  const a = meshes.add(tri());
  const b = meshes.add(tri());
  console.log(`added ${{a.$asset}} ${{b.$asset}}`);
}}
app.addSystems(Startup, run);
"#
    );
    let (dir, mut app) = assets_app("reload-free", &body, &[], |_| {});
    updates(&mut app, 2);
    let added = logged(&app, "added ");
    let refs = app.world().resource::<ScriptAssets>().clone();
    let ids: Vec<Handle<Mesh>> = added.split(' ').map(|n| refs.handle(n.parse().unwrap()).unwrap().typed()).collect();
    // The first goes on an entity, as a slot would hold it.
    app.world_mut().spawn(Mesh3d(ids[0].clone()));
    let (on_entity, only_script) = (ids[0].id(), ids[1].id());
    drop(ids);
    write(&dir, "game.ts", &format!("{IMPORTS}// edited\n{body}"));
    let r = reload(&mut app, false);
    assert!(r.errors.is_empty(), "{:?}", r.errors);
    updates(&mut app, 3);
    let meshes = app.world().resource::<Assets<Mesh>>();
    assert!(meshes.get(on_entity).is_some(), "the entity's mesh stays");
    assert!(meshes.get(only_script).is_none(), "the one only the old script held is freed");
}

#[test]
fn a_uuid_ref_keeps_its_asset_after_drop() {
    let mut app = mesh_app(
        "uuid-drop",
        r#"
const Image = asset("bevy_image::image::Image");
export function run() {
  const r = assets.add(Image, {});
  console.log(`made ${r.$asset}`);
  assets.drop(r);
}
app.addSystems(Startup, run);
"#,
    );
    let handle = {
        updates(&mut app, 2);
        let n = ref_of(&app, "made ");
        // The ref is stale, but the UUID handle is derived from its number.
        let uuid = bevy::asset::uuid::Uuid::from_u64_pair(0x6265_7679_2d74_7341, n);
        Handle::<Image>::Uuid(uuid, std::marker::PhantomData)
    };
    updates(&mut app, 2);
    assert!(app.world().resource::<Assets<Image>>().get(&handle).is_some(), "a UUID asset stays after drop");
}

const GLTF: &str = r#"{"asset":{"version":"2.0"},
"buffers":[{"byteLength":36,"uri":"data:application/octet-stream;base64,AAAAAAAAAAAAAAAAAACAPwAAAAAAAAAAAAAAAAAAgD8AAAAA"}],
"bufferViews":[{"buffer":0,"byteLength":36}],
"accessors":[{"bufferView":0,"componentType":5126,"count":3,"type":"VEC3","min":[0,0,0],"max":[1,1,0]}],
"images":[{"uri":"tex.png"}],"textures":[{"source":0}],
"materials":[{"pbrMetallicRoughness":{"baseColorTexture":{"index":0}}}],
"meshes":[{"primitives":[{"attributes":{"POSITION":0},"material":0}]}],
"nodes":[{"mesh":0}],"scenes":[{"nodes":[0]}],"scene":0}"#;

/// A glTF whose material reads a texture file: the file's load and its dependencies' are seen
/// apart. Bevy's glTF loader needs the full engine, so this runs on the GPU (macOS, as the other
/// GPU tests).
#[cfg(target_os = "macos")]
#[test]
fn a_gltf_waits_for_its_dependencies() {
    use bevy::window::ExitCondition;
    use bevy::winit::WinitPlugin;
    let png = png();
    let dir = project(
        "load-gltf",
        &[(
            "game.ts",
            &format!(
                r#"{IMPORTS}
let r: AssetRef<unknown> | undefined;
const seen: string[] = [];
export function watch(server: Res<AssetServer>) {{
  r ??= server.load("tri.gltf");
  const line = `${{server.isLoaded(r)}} ${{server.isLoadedWithDirectDependencies(r)}} ${{server.isLoadedWithDependencies(r)}} ${{JSON.stringify(server.dependencyLoadState(r))}} ${{JSON.stringify(server.recursiveDependencyLoadState(r))}}`;
  if (seen[seen.length - 1] !== line) {{ seen.push(line); console.log(`state ${{line}}`); }}
  if (server.isLoadedWithDependencies(r)) console.log(`done ${{r.type}}`);
}}
app.addSystems(Update, watch);
"#
            ),
        )],
    );
    std::fs::write(dir.join("tri.gltf"), GLTF).unwrap();
    std::fs::write(dir.join("tex.png"), &png).unwrap();
    let mut app = App::new();
    app.add_plugins(
        DefaultPlugins
            .build()
            .disable::<WinitPlugin>()
            .set(WindowPlugin { primary_window: None, exit_condition: ExitCondition::DontExit, ..default() })
            .set(AssetPlugin { file_path: dir.to_string_lossy().into_owned(), ..default() }),
    )
    .add_plugins(BevyTsPlugin::new(bevy_ts_v8::factory(V8Options::default())).dev(true))
    .add_plugins(bevy_ts_cli::assets::ScriptAssetsPlugin);
    app.insert_resource(DevSource(Arc::new(Compiler::new(&dir.join("game.ts"), true).unwrap())));
    app.finish();
    app.cleanup();
    let r = reload(&mut app, false);
    assert!(r.errors.is_empty(), "{:?}", r.errors);
    assert_eq!(until_logged(&mut app, "done "), "bevy_gltf::Gltf");
    app.update();
    let states: Vec<String> = messages(&app).iter().filter_map(|l| l.strip_prefix("state ")).map(str::to_string).collect();
    for line in &states {
        let f: Vec<&str> = line.split(' ').collect();
        let (loaded, all, recursive) = (f[0] == "true", f[2] == "true", f[4]);
        // `isLoaded` comes no later than `isLoadedWithDependencies`, and the recursive state is
        // `Loaded` exactly when it is true.
        assert!(!all || loaded, "{states:?}");
        assert_eq!(recursive == "\"Loaded\"", all, "{states:?}");
    }
    assert!(states.last().unwrap().starts_with("true true true"), "{states:?}");
}

#[test]
#[ignore = "needs rust-events: MessageReader<AssetEvent<A>> in TS"]
fn a_load_is_seen_as_an_event() {}
