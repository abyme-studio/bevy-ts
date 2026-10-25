//! `<mesh positions>`: a mesh from vertex data, built headless and compared with the same calls
//! on Bevy's `Mesh`; refused data fails the load at the element; equal props share one mesh.

use crate::primitives::{app, load, mesh_of, node};
use bevy::asset::RenderAssetUsages;
use bevy::mesh::{Indices, PrimitiveTopology, VertexAttributeValues};
use bevy::prelude::*;
use bevy_ts::generation::ReloadStatus;
use bevy_ts_tsx::TsxNode;
use serde_json::json;

fn quad() -> serde_json::Value {
    json!({
        "positions": [[0, 0, 0], [1, 0, 0], [1, 1, 0], [0, 1, 0]],
        "uvs": [[0, 0], [1, 0], [1, 1], [0, 1]],
        "indices": [0, 1, 2, 0, 2, 3],
        "normals": "computed",
    })
}

fn handle_of(app: &mut App, key: &str) -> Handle<Mesh> {
    let world = app.world_mut();
    let e = world
        .query::<(Entity, &TsxNode)>()
        .iter(world)
        .find(|(_, n)| n.key_path.last().map(String::as_str) == Some(key))
        .map(|(e, _)| e)
        .unwrap_or_else(|| panic!("no entity {key}"));
    world.get::<Mesh3d>(e).unwrap_or_else(|| panic!("{key} has no Mesh3d")).0.clone()
}

/// Every attribute and the indices, by Bevy's name, for comparing two meshes.
fn data(m: &Mesh) -> (PrimitiveTopology, Vec<(String, VertexAttributeValues)>, Option<Indices>) {
    let attrs = m.attributes().map(|(a, v)| (a.name.to_string(), v.clone())).collect();
    (m.primitive_topology(), attrs, m.indices().cloned())
}

#[test]
fn a_quad_from_props_equals_bevys_calls() {
    let (mut app, fake) = app();
    let r = load(&mut app, &fake, vec![node("q", 1, quad())]);
    assert_eq!(r.status, ReloadStatus::Applied, "{:?}", r.errors);
    let got = mesh_of(&mut app, "q");
    let mut want = Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::default())
        .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, vec![[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [1.0, 1.0, 0.0], [0.0, 1.0, 0.0]])
        .with_inserted_attribute(Mesh::ATTRIBUTE_UV_0, vec![[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0]])
        .with_inserted_indices(Indices::U32(vec![0, 1, 2, 0, 2, 3]));
    want.compute_normals();
    assert_eq!(data(&got), data(&want));
}

#[test]
fn every_prop_reaches_bevy() {
    let (mut app, fake) = app();
    let props = json!({
        "positions": [[0, 0, 0], [1, 0, 0], [0, 1, 0]],
        "uvs": [[0, 0], [1, 0], [0, 1]],
        "uvs1": [[0, 1], [1, 1], [0, 0]],
        "colors": [[1, 0, 0, 1], [0, 1, 0, 1], [0, 0, 1, 1]],
        "jointIndices": [[0, 1, 2, 3], [0, 0, 0, 0], [1, 1, 1, 1]],
        "jointWeights": [[1, 0, 0, 0], [0.5, 0.5, 0, 0], [1, 0, 0, 0]],
        "indices": { "U16": [0, 1, 2] },
        "normals": "smooth",
        "tangents": "generated",
    });
    let lines = json!({ "positions": [[0, 0, 0], [1, 0, 0]], "topology": "LineList", "assetUsage": "MAIN_WORLD" });
    let r = load(&mut app, &fake, vec![node("all", 1, props), node("lines", 2, lines)]);
    assert_eq!(r.status, ReloadStatus::Applied, "{:?}", r.errors);
    let got = mesh_of(&mut app, "all");
    let mut want = Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::default())
        .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, vec![[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]])
        .with_inserted_attribute(Mesh::ATTRIBUTE_UV_0, vec![[0.0, 0.0], [1.0, 0.0], [0.0, 1.0]])
        .with_inserted_attribute(Mesh::ATTRIBUTE_UV_1, vec![[0.0, 1.0], [1.0, 1.0], [0.0, 0.0]])
        .with_inserted_attribute(Mesh::ATTRIBUTE_COLOR, vec![[1.0, 0.0, 0.0, 1.0], [0.0, 1.0, 0.0, 1.0], [0.0, 0.0, 1.0, 1.0]])
        .with_inserted_attribute(Mesh::ATTRIBUTE_JOINT_INDEX, VertexAttributeValues::Uint16x4(vec![[0, 1, 2, 3], [0, 0, 0, 0], [1, 1, 1, 1]]))
        .with_inserted_attribute(Mesh::ATTRIBUTE_JOINT_WEIGHT, vec![[1.0, 0.0, 0.0, 0.0], [0.5, 0.5, 0.0, 0.0], [1.0, 0.0, 0.0, 0.0]])
        .with_inserted_indices(Indices::U16(vec![0, 1, 2]));
    want.compute_smooth_normals();
    want.generate_tangents().unwrap();
    assert_eq!(data(&got), data(&want));
    let lines = mesh_of(&mut app, "lines");
    assert_eq!(lines.primitive_topology(), PrimitiveTopology::LineList);
    assert_eq!(lines.asset_usage, RenderAssetUsages::MAIN_WORLD);
}

#[test]
fn mismatched_lengths_fail_at_load() {
    let (mut app, fake) = app();
    let mut bad = quad();
    bad["uvs"] = json!([[0, 0], [1, 0], [1, 1]]);
    let r = load(&mut app, &fake, vec![node("ok", 1, quad()), node("bad", 9, bad)]);
    assert_eq!(r.status, ReloadStatus::Failed);
    let e = &r.errors[0];
    assert!(e.message.contains("uvs") && e.message.contains('3') && e.message.contains('4'), "{}", e.message);
    let at = e.location.as_ref().unwrap();
    assert_eq!((at.file.as_str(), at.line), ("shapes.tsx", 9));
    // The engine keeps running, and the next good load applies.
    app.update();
    let r = load(&mut app, &fake, vec![node("ok", 1, quad())]);
    assert_eq!(r.status, ReloadStatus::Applied, "{:?}", r.errors);
}

#[test]
fn what_bevy_refuses_fails_at_load_naming_the_prop() {
    let cases = [
        (json!({ "positions": [[0, 0, 0], [1, 0, 0], [0, 1, 0]], "indices": [0, 1, 3] }), "indices"),
        (json!({ "positions": [[0, 0, 0]], "topology": "QuadList" }), "topology"),
        // Bevy asserts flat normals have no indices, and smooth normals need them.
        (json!({ "positions": [[0, 0, 0], [1, 0, 0], [0, 1, 0]], "indices": [0, 1, 2], "normals": "flat" }), "normals"),
        (json!({ "positions": [[0, 0, 0], [1, 0, 0], [0, 1, 0]], "normals": "smooth" }), "normals"),
        (json!({ "positions": [[0, 0, 0], [1, 0], [0, 1, 0]] }), "positions"),
        (json!({ "positions": [[0, 0, 0], [1, 0, 0]], "topology": "LineList", "normals": "computed" }), "normals"),
        (json!({ "positions": [[0, 0, 0], [1, 0, 0], [0, 1, 0]], "tangents": "generated" }), "tangents"),
        (json!({ "positions": [[0, 0, 0]], "normals": "sharp" }), "normals"),
        (json!({ "positions": [[0, 0, 0]], "shape": "cuboid" }), "shape and positions"),
        (json!({ "positions": [[0, 0, 0]], "uv": [[0, 0]] }), "uv"),
        (json!({ "positions": [[0, 0, 0]], "assetUsage": "" }), "assetUsage"),
    ];
    for (props, prop) in cases {
        let (mut app, fake) = app();
        let r = load(&mut app, &fake, vec![node("bad", 4, props.clone())]);
        assert_eq!(r.status, ReloadStatus::Failed, "{props}");
        let e = &r.errors[0];
        assert!(e.message.contains(prop), "{props}: {}", e.message);
        assert_eq!(e.location.as_ref().map(|l| l.line), Some(4), "{props}: {e:?}");
    }
}

#[test]
fn equal_vertex_props_share_one_mesh() {
    let (mut app, fake) = app();
    let mut other = quad();
    other["normals"] = json!("smooth");
    let r = load(&mut app, &fake, vec![node("a", 1, quad()), node("b", 2, quad()), node("c", 3, other)]);
    assert_eq!(r.status, ReloadStatus::Applied, "{:?}", r.errors);
    assert_eq!(handle_of(&mut app, "a"), handle_of(&mut app, "b"));
    assert_ne!(handle_of(&mut app, "a"), handle_of(&mut app, "c"));
}
