//! Every `<mesh shape>` built headless (no GPU) and its geometry checked: vertices, a normal per
//! vertex, triangles whose winding agrees with their normals, and closed solids facing outward.

use bevy::mesh::{Indices, PrimitiveTopology, VertexAttributeValues};
use bevy::prelude::*;
use bevy_ts::BevyTsPlugin;
use bevy_ts::backend::ModuleSet;
use bevy_ts::fake::{FakeHandle, FakeProgram};
use bevy_ts::generation::{self, ReloadReport, ReloadStatus};
use bevy_ts::manifest::Section;
use bevy_ts_tsx::{TsxNode, TsxPlugin};
use serde_json::{Value, json};
use std::path::PathBuf;

fn app() -> (App, FakeHandle) {
    let handle = FakeHandle::default();
    let h = handle.clone();
    let mut app = App::new();
    app.add_plugins((MinimalPlugins, AssetPlugin::default()))
        .init_asset::<Mesh>()
        .init_asset::<StandardMaterial>()
        .add_plugins(BevyTsPlugin::new(move || Box::new(h.backend())))
        .add_plugins(TsxPlugin);
    (app, handle)
}

/// One entity, keyed `key`, with `<mesh {props} />` on line `line` of shapes.tsx.
fn node(key: &str, line: u32, props: Value) -> Value {
    json!({
        "key": key,
        "type": "entity",
        "source": [{ "file": "shapes.tsx", "line": line, "column": 3 }],
        "components": [
            { "type": "mesh", "kind": "intrinsic", "props": props, "source": { "file": "shapes.tsx", "line": line, "column": 5 } }
        ],
        "children": []
    })
}

fn load(app: &mut App, fake: &FakeHandle, nodes: Vec<Value>) -> ReloadReport {
    let section = json!({ "roots": [{ "name": "shapes", "nodes": nodes }] });
    fake.push(FakeProgram {
        sections: vec![Section { name: "tsx".into(), bytes: serde_json::to_vec(&section).unwrap(), location: None }],
        ..Default::default()
    });
    generation::apply(app.world_mut(), ModuleSet { entry: PathBuf::from("/game.tsx"), modules: vec![] }, false)
}

fn mesh_of(app: &mut App, key: &str) -> Mesh {
    let world = app.world_mut();
    let e = world
        .query::<(Entity, &TsxNode)>()
        .iter(world)
        .find(|(_, n)| n.key_path.last().map(String::as_str) == Some(key))
        .map(|(e, _)| e)
        .unwrap_or_else(|| panic!("no entity {key}"));
    let h = world.get::<Mesh3d>(e).unwrap_or_else(|| panic!("{key} has no Mesh3d")).0.clone();
    world.resource::<Assets<Mesh>>().get(&h).unwrap().clone()
}

const SOLIDS_3D: &[&str] = &["cuboid", "sphere", "cylinder", "capsule3d", "cone", "conicalFrustum", "torus", "tetrahedron"];
const OPEN_3D: &[&str] = &["plane", "triangle3d"];
const SHAPES_2D: &[&str] = &[
    "circle",
    "circularSector",
    "circularSegment",
    "ellipse",
    "annulus",
    "rectangle",
    "rhombus",
    "regularPolygon",
    "triangle2d",
    "capsule2d",
    "convexPolygon",
];

/// A 2D shape's props: `convexPolygon` has no default vertices.
fn flat(shape: &str) -> Value {
    match shape {
        "convexPolygon" => json!({ "shape": shape, "vertices": [[0, 0], [1, 0], [1.5, 1], [0, 1.5]] }),
        _ => json!({ "shape": shape }),
    }
}

/// Every shape at its defaults, every 2D shape extruded, and a few builder settings. Each case:
/// its key, its props, and whether its mesh is closed (`Some(true)`), open, or flat 2D (`None`).
fn cases() -> Vec<(String, Value, Option<bool>)> {
    let mut out = vec![];
    for s in SOLIDS_3D {
        out.push((s.to_string(), json!({ "shape": s }), Some(true)));
    }
    for s in OPEN_3D {
        out.push((s.to_string(), json!({ "shape": s }), Some(false)));
    }
    for s in SHAPES_2D {
        out.push((s.to_string(), flat(s), None));
        let mut p = flat(s);
        p["depth"] = json!(1);
        out.push((format!("{s}-extruded"), p, Some(true)));
    }
    // A clockwise polygon and triangle face +Z too.
    let cw = json!({ "shape": "convexPolygon", "vertices": [[0, 1.5], [1.5, 1], [1, 0], [0, 0]] });
    out.push(("convexPolygon-cw".into(), cw.clone(), None));
    let mut cw_deep = cw;
    cw_deep["depth"] = json!(0.5);
    cw_deep["segments"] = json!(3);
    out.push(("convexPolygon-cw-extruded".into(), cw_deep, Some(true)));
    out.push(("triangle2d-cw".into(), json!({ "shape": "triangle2d", "vertices": [[0, 0], [0, 1], [1, 0]] }), None));
    out.push(("cylinder-open".into(), json!({ "shape": "cylinder", "caps": false }), Some(false)));
    out.push(("sphere-uv".into(), json!({ "shape": "sphere", "kind": "uv" }), Some(true)));
    out.push(("capsule3d-rings".into(), json!({ "shape": "capsule3d", "rings": 3 }), Some(true)));
    out.push(("cylinder-bottom".into(), json!({ "shape": "cylinder", "anchor": "bottom", "segments": 3 }), Some(true)));
    out.push(("cone-tip".into(), json!({ "shape": "cone", "anchor": "tip", "resolution": 3 }), Some(true)));
    out.push(("plane-tilted".into(), json!({ "shape": "plane", "normal": [1, 1, 0], "subdivisions": 2 }), Some(false)));
    out.push(("annulus-thin".into(), json!({ "shape": "annulus", "innerRadius": 0.9, "depth": 0.1 }), Some(true)));
    out
}

fn vec3s(m: &Mesh, attr: bevy::mesh::MeshVertexAttribute, key: &str) -> Vec<Vec3> {
    match m.attribute(attr) {
        Some(VertexAttributeValues::Float32x3(v)) => v.iter().map(|&p| Vec3::from(p)).collect(),
        other => panic!("{key}: {} is {other:?}", attr.name),
    }
}

fn triangles(m: &Mesh) -> Vec<[usize; 3]> {
    let idx: Vec<usize> = match m.indices() {
        Some(Indices::U16(v)) => v.iter().map(|&i| i as usize).collect(),
        Some(Indices::U32(v)) => v.iter().map(|&i| i as usize).collect(),
        None => (0..m.count_vertices()).collect(),
    };
    assert_eq!(idx.len() % 3, 0);
    idx.chunks(3).map(|t| [t[0], t[1], t[2]]).collect()
}

/// Checks `m`; `closed` as in `cases`.
fn check_mesh(key: &str, m: &Mesh, closed: Option<bool>) {
    assert_eq!(m.primitive_topology(), PrimitiveTopology::TriangleList, "{key}");
    let n = m.count_vertices();
    assert!(n > 0, "{key}: no vertices");
    let pos = vec3s(m, Mesh::ATTRIBUTE_POSITION, key);
    let nor = vec3s(m, Mesh::ATTRIBUTE_NORMAL, key);
    assert_eq!(nor.len(), n, "{key}: one normal per vertex");
    if let Some(idx) = m.indices() {
        assert_eq!(idx.len() % 3, 0, "{key}: index count");
    }
    let tris = triangles(m);
    assert!(!tris.is_empty(), "{key}: no triangles");
    let mut volume = 0.0;
    for [a, b, c] in &tris {
        let (pa, pb, pc) = (pos[*a], pos[*b], pos[*c]);
        let face = (pb - pa).cross(pc - pa);
        volume += pa.dot(pb.cross(pc)) / 6.0;
        // A degenerate triangle has no winding to check.
        if face.length() < 1e-7 {
            continue;
        }
        let along = face.dot(nor[*a] + nor[*b] + nor[*c]);
        assert!(along > 0.0, "{key}: triangle {a},{b},{c} winds against its normals ({along})");
    }
    match closed {
        Some(true) => assert!(volume > 0.0, "{key}: signed volume {volume} (normals inward)"),
        Some(false) => {}
        None => {
            for v in &nor {
                assert_eq!(*v, Vec3::Z, "{key}: a flat 2D mesh faces +Z");
            }
            assert!(pos.iter().all(|p| p.z == 0.0), "{key}: a flat 2D mesh lies in XY");
        }
    }
}

#[test]
fn every_shape_builds_a_valid_mesh() {
    let (mut app, fake) = app();
    let cases = cases();
    let nodes = cases.iter().enumerate().map(|(i, (k, p, _))| node(k, i as u32 + 1, p.clone())).collect();
    let r = load(&mut app, &fake, nodes);
    assert_eq!(r.status, ReloadStatus::Applied, "{:?}", r.errors);
    let shapes: std::collections::BTreeSet<&str> = cases.iter().map(|(_, p, _)| p["shape"].as_str().unwrap()).collect();
    assert_eq!(shapes.len(), 21, "every shape");
    for (key, _, closed) in &cases {
        let m = mesh_of(&mut app, key);
        check_mesh(key, &m, *closed);
    }
}

#[test]
fn builder_settings_reach_bevy() {
    let (mut app, fake) = app();
    let props = json!({ "shape": "cylinder", "radius": 0.5, "height": 2, "resolution": 8 });
    let r = load(&mut app, &fake, vec![node("c", 1, props)]);
    assert_eq!(r.status, ReloadStatus::Applied, "{:?}", r.errors);
    let got = mesh_of(&mut app, "c");
    let want = Cylinder::new(0.5, 2.0).mesh().resolution(8).build();
    assert_eq!(vec3s(&got, Mesh::ATTRIBUTE_POSITION, "c"), vec3s(&want, Mesh::ATTRIBUTE_POSITION, "want"));
    assert_eq!(triangles(&got), triangles(&want));
}

#[test]
fn a_value_bevy_would_panic_on_fails_the_load() {
    let (mut app, fake) = app();
    let r = load(&mut app, &fake, vec![node("ok", 1, json!({ "shape": "circle" })), node("bad", 7, json!({ "shape": "regularPolygon", "sides": 2 }))]);
    assert_eq!(r.status, ReloadStatus::Failed);
    let e = &r.errors[0];
    assert!(e.message.contains("regularPolygon") && e.message.contains("sides"), "{}", e.message);
    let at = e.location.as_ref().unwrap();
    assert_eq!((at.file.as_str(), at.line), ("shapes.tsx", 7));
    // The engine keeps running, and the next good load applies.
    app.update();
    app.update();
    let r = load(&mut app, &fake, vec![node("ok", 1, json!({ "shape": "regularPolygon", "sides": 3 }))]);
    assert_eq!(r.status, ReloadStatus::Applied, "{:?}", r.errors);
    check_mesh("ok", &mesh_of(&mut app, "ok"), None);
}

#[test]
fn a_finer_uv_sphere_has_more_vertices() {
    let (mut app, fake) = app();
    let nodes = vec![
        node("default", 1, json!({ "shape": "sphere" })),
        node("uv", 2, json!({ "shape": "sphere", "kind": "uv" })),
        node("fine", 3, json!({ "shape": "sphere", "kind": "uv", "sectors": 64, "stacks": 32 })),
    ];
    let r = load(&mut app, &fake, nodes);
    assert_eq!(r.status, ReloadStatus::Applied, "{:?}", r.errors);
    let fine = mesh_of(&mut app, "fine");
    check_mesh("fine", &fine, Some(true));
    for k in ["default", "uv"] {
        assert!(fine.count_vertices() > mesh_of(&mut app, k).count_vertices(), "fine vs {k}");
    }
}
