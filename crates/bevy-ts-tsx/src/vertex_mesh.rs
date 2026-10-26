//! `<mesh positions>`: a mesh from vertex data (change script-assets, design decision 7). Each prop
//! is one of Bevy's `Mesh` calls: `positions`, `normals`, `uvs`, `uvs1`, `tangents`, `colors`,
//! `jointIndices` and `jointWeights` insert `Mesh::ATTRIBUTE_*`; `indices` inserts `Indices`;
//! `topology` and `assetUsage` are `Mesh::new`'s. `normals` or `tangents` given as a word runs
//! Bevy's computation instead. `check` builds the mesh, so every refusal is found at load and
//! `mesh` never panics.

use bevy::asset::RenderAssetUsages;
use bevy::mesh::{Indices, Mesh, MeshVertexAttribute, PrimitiveTopology, VertexAttributeValues};
use bevy_ts::mesh_data::{self, MeshOp};
use serde::Deserialize;
use serde_json::{Map, Value};

type V2 = [f32; 2];
type V3 = [f32; 3];
type V4 = [f32; 4];

/// The vertex props of a `<mesh>` with `positions`.
#[derive(Clone, Debug, PartialEq, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct VertexMesh {
    positions: Vec<V3>,
    #[serde(default)]
    normals: Option<Normals>,
    #[serde(default)]
    uvs: Option<Vec<V2>>,
    #[serde(default)]
    uvs1: Option<Vec<V2>>,
    #[serde(default)]
    tangents: Option<Tangents>,
    #[serde(default)]
    colors: Option<Vec<V4>>,
    #[serde(default)]
    joint_indices: Option<Vec<[u16; 4]>>,
    #[serde(default)]
    joint_weights: Option<Vec<V4>>,
    #[serde(default)]
    indices: Option<IndexProp>,
    #[serde(default)]
    topology: Option<String>,
    #[serde(default)]
    asset_usage: Option<RenderAssetUsages>,
}

/// `normals`: one per vertex, or the computation that makes them.
#[derive(Clone, Debug, PartialEq, Deserialize)]
#[serde(untagged)]
enum Normals {
    Given(Vec<V3>),
    Computed(Computed),
}

#[derive(Clone, Copy, Debug, PartialEq, Deserialize)]
#[allow(clippy::enum_variant_names)] // "computed" is a wire value
#[serde(rename_all = "camelCase")]
enum Computed {
    Computed,
    Flat,
    Smooth,
    AreaWeighted,
}

/// `tangents`: one per vertex, or `"generated"`.
#[derive(Clone, Debug, PartialEq, Deserialize)]
#[serde(untagged)]
enum Tangents {
    Given(Vec<V4>),
    Generated(Generated),
}

#[derive(Clone, Copy, Debug, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
enum Generated {
    Generated,
}

/// `indices`: a plain list is `Indices::U32`.
#[derive(Clone, Debug, PartialEq, Deserialize)]
#[serde(untagged)]
enum IndexProp {
    U32(Vec<u32>),
    Tagged(Tagged),
}

#[derive(Clone, Debug, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
enum Tagged {
    U16(Vec<u16>),
    U32(Vec<u32>),
}

/// Bevy's topologies by the variant names its API spells.
const TOPOLOGIES: [(&str, PrimitiveTopology); 5] = [
    ("PointList", PrimitiveTopology::PointList),
    ("LineList", PrimitiveTopology::LineList),
    ("LineStrip", PrimitiveTopology::LineStrip),
    ("TriangleList", PrimitiveTopology::TriangleList),
    ("TriangleStrip", PrimitiveTopology::TriangleStrip),
];

impl VertexMesh {
    /// Parses a `<mesh>`'s props other than `color`. An error names the prop.
    pub fn from_props(props: Map<String, Value>) -> Result<Self, String> {
        let e = match serde_json::from_value(Value::Object(props.clone())) {
            Ok(m) => return Ok(m),
            Err(e) => e,
        };
        // Find the prop at fault: parse each with empty positions.
        for (k, v) in &props {
            let mut alone = Map::new();
            alone.insert("positions".into(), Value::Array(vec![]));
            alone.insert(k.clone(), v.clone());
            if let Err(e) = serde_json::from_value::<VertexMesh>(Value::Object(alone)) {
                return Err(format!("{k}: {}", plain(&e.to_string())));
            }
        }
        Err(e.to_string())
    }

    /// Builds the mesh, refusing what Bevy would panic or error on; the error names the prop.
    pub fn build(&self) -> Result<Mesh, String> {
        let topology = match &self.topology {
            None => PrimitiveTopology::TriangleList,
            Some(t) => TOPOLOGIES.iter().find(|(n, _)| n == t).map(|(_, p)| *p).ok_or_else(|| {
                format!("topology: {t:?} is not one of Bevy's ({})", TOPOLOGIES.map(|(n, _)| n).join(", "))
            })?,
        };
        let usage = self.asset_usage.unwrap_or_default();
        if usage.is_empty() {
            return Err("assetUsage: a mesh in neither world is never drawn; give MAIN_WORLD, RENDER_WORLD or both".into());
        }
        let n = self.positions.len();
        let mut mesh = Mesh::new(topology, usage);
        mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, self.positions.clone());
        let mut put = |prop: &str, attr: MeshVertexAttribute, values: VertexAttributeValues| {
            if values.len() != n {
                return Err(format!("{prop}: has {} vertices, positions has {n}", values.len()));
            }
            mesh.insert_attribute(attr, values);
            Ok(())
        };
        if let Some(Normals::Given(v)) = &self.normals {
            put("normals", Mesh::ATTRIBUTE_NORMAL, v.clone().into())?;
        }
        if let Some(v) = &self.uvs {
            put("uvs", Mesh::ATTRIBUTE_UV_0, v.clone().into())?;
        }
        if let Some(v) = &self.uvs1 {
            put("uvs1", Mesh::ATTRIBUTE_UV_1, v.clone().into())?;
        }
        if let Some(Tangents::Given(v)) = &self.tangents {
            put("tangents", Mesh::ATTRIBUTE_TANGENT, v.clone().into())?;
        }
        if let Some(v) = &self.colors {
            put("colors", Mesh::ATTRIBUTE_COLOR, v.clone().into())?;
        }
        if let Some(v) = &self.joint_indices {
            put("jointIndices", Mesh::ATTRIBUTE_JOINT_INDEX, VertexAttributeValues::Uint16x4(v.clone()))?;
        }
        if let Some(v) = &self.joint_weights {
            put("jointWeights", Mesh::ATTRIBUTE_JOINT_WEIGHT, v.clone().into())?;
        }
        if let Some(ix) = &self.indices {
            let ix = match ix {
                IndexProp::U32(v) | IndexProp::Tagged(Tagged::U32(v)) => Indices::U32(v.clone()),
                IndexProp::Tagged(Tagged::U16(v)) => Indices::U16(v.clone()),
            };
            if let Some((at, i)) = ix.iter().enumerate().find(|(_, i)| *i >= n) {
                return Err(format!("indices: indices[{at}] is {i}, but positions has {n} vertices"));
            }
            mesh.insert_indices(ix);
        }
        if let Some(Normals::Computed(c)) = self.normals {
            let op = match c {
                Computed::Computed => MeshOp::ComputeNormals,
                Computed::Flat => MeshOp::ComputeFlatNormals,
                Computed::Smooth => MeshOp::ComputeSmoothNormals,
                Computed::AreaWeighted => MeshOp::ComputeAreaWeightedNormals,
            };
            mesh_data::run(&mut mesh, op).map_err(|e| format!("normals: {e}"))?;
        }
        if let Some(Tangents::Generated(_)) = self.tangents {
            mesh_data::run(&mut mesh, MeshOp::GenerateTangents).map_err(|e| format!("tangents: {e}"))?;
        }
        Ok(mesh)
    }
}

/// serde's untagged miss, said plainly.
fn plain(e: &str) -> String {
    if e.starts_with("data did not match any variant of untagged enum Normals") {
        "a Vec3 per vertex, or \"computed\", \"flat\", \"smooth\" or \"areaWeighted\"".into()
    } else if e.starts_with("data did not match any variant of untagged enum Tangents") {
        "a Vec4 per vertex, or \"generated\"".into()
    } else if e.starts_with("data did not match any variant of untagged enum IndexProp") {
        "a list of vertex numbers, { U16: [...] } or { U32: [...] }".into()
    } else {
        e.into()
    }
}
