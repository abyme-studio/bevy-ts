//! `Mesh` as asset data: its hand-mapped entry (change script-assets, design decision 6), and
//! Bevy's mesh operations checked so none panics, for `bevy-ts/assets` and `<mesh positions>`.
//!
//! Bevy reflects a mesh's topology and attributes as `ignore`, so `reflect_value` cannot reach
//! them. This entry reads and writes Bevy-named JSON instead:
//!
//! ```json
//! { "primitive_topology": "TriangleList", "asset_usage": "MAIN_WORLD | RENDER_WORLD",
//!   "attributes": { "Vertex_Position": [[0, 0, 0], ...], "Vertex_Uv": [[0, 1], ...] },
//!   "indices": { "U32": [0, 1, 2] } }
//! ```
//!
//! Topology is its variant name (wgpu's enum is not reflected); usage takes its reflected shape. An attribute is keyed by Bevy's name; its
//! values are one number per vertex for a one-component format, else one array per vertex.
//! Indices are `{ "U16": [...] }`, `{ "U32": [...] }` or `null`. Written whole, a mesh is an empty
//! `TriangleList` with both usages under the keys given; written over a mesh, a key given
//! replaces that part whole and the rest is kept.

use crate::reflect_asset::Slots;
use crate::reflect_value;
use bevy::asset::RenderAssetUsages;
use bevy::mesh::{Indices, Mesh, MeshVertexAttribute, PrimitiveTopology, VertexAttributeValues, VertexFormat};
use bevy::reflect::Reflect;
use serde_json::{Map, Value, json};

/// The attributes Bevy names, for a key the base mesh does not already have.
const KNOWN: [MeshVertexAttribute; 8] = [
    Mesh::ATTRIBUTE_POSITION,
    Mesh::ATTRIBUTE_NORMAL,
    Mesh::ATTRIBUTE_UV_0,
    Mesh::ATTRIBUTE_UV_1,
    Mesh::ATTRIBUTE_TANGENT,
    Mesh::ATTRIBUTE_COLOR,
    Mesh::ATTRIBUTE_JOINT_WEIGHT,
    Mesh::ATTRIBUTE_JOINT_INDEX,
];

const RENDER_ONLY: &str = "the mesh's data is in the render world only (asset_usage RENDER_WORLD)";

/// `Mesh`'s hand-map read, registered by [`crate::reflect_asset`] under `pbr`.
pub fn read(slots: &Slots, _: Option<&Value>, mesh: &Mesh) -> Result<Value, String> {
    let mut attributes = Map::new();
    for (attr, values) in mesh.try_attributes().map_err(|_| RENDER_ONLY)? {
        attributes.insert(attr.name.to_string(), values_json(values));
    }
    let indices = match mesh.try_indices_option().map_err(|_| RENDER_ONLY)? {
        Some(Indices::U16(v)) => json!({ "U16": v }),
        Some(Indices::U32(v)) => json!({ "U32": v }),
        None => Value::Null,
    };
    Ok(json!({
        "primitive_topology": topology_name(mesh.primitive_topology()),
        "asset_usage": reflect_value::read(slots.registry, &mesh.asset_usage)?,
        "attributes": attributes,
        "indices": indices,
    }))
}

/// `Mesh`'s hand-map write.
pub fn write(slots: &Slots, base: Option<&Mesh>, json: &Value) -> Result<Mesh, String> {
    let o = json.as_object().ok_or_else(|| format!("a Mesh is an object, not {json}"))?;
    if let Some(k) = o.keys().find(|k| !matches!(k.as_str(), "primitive_topology" | "asset_usage" | "attributes" | "indices")) {
        return Err(format!("a Mesh has no {k:?}; it has primitive_topology, asset_usage, attributes and indices"));
    }
    let topology = match o.get("primitive_topology") {
        Some(j) => topology_from(j)?,
        None => base.map_or(PrimitiveTopology::TriangleList, Mesh::primitive_topology),
    };
    let usage = match o.get("asset_usage") {
        Some(j) => field(slots, RenderAssetUsages::default(), j, "asset_usage")?,
        None => base.map_or_else(RenderAssetUsages::default, |b| b.asset_usage),
    };
    // Kept from the base: what this entry does not map (morph targets), unless the topology changes.
    let mut mesh = match base {
        Some(b) if b.primitive_topology() == topology => b.clone(),
        _ => {
            let mut m = Mesh::new(topology, usage);
            if let Some(b) = base {
                m.enable_raytracing = b.enable_raytracing;
                copy_data(b, &mut m)?;
            }
            m
        }
    };
    mesh.asset_usage = usage;
    if let Some(attrs) = o.get("attributes") {
        let attrs = attrs.as_object().ok_or_else(|| format!("a Mesh's attributes are an object by Bevy name, not {attrs}"))?;
        let old: Vec<MeshVertexAttribute> = match base {
            Some(b) => b.try_attributes().map_err(|_| RENDER_ONLY)?.map(|(a, _)| a.clone()).collect(),
            None => Vec::new(),
        };
        for a in &old {
            mesh.try_remove_attribute(a.id).map_err(|_| RENDER_ONLY)?;
        }
        let mut count: Option<(&str, usize)> = None;
        for (name, data) in attrs {
            let attr = old
                .iter()
                .chain(&KNOWN)
                .find(|a| a.name == name)
                .cloned()
                .ok_or_else(|| format!("{name:?} is not a vertex attribute Bevy names ({})", KNOWN.map(|a| a.name).join(", ")))?;
            let values = values_from(attr.format, data).map_err(|e| format!("attributes.{name}: {e}"))?;
            match count {
                Some((first, n)) if n != values.len() => {
                    return Err(format!("attributes differ in length: {first} has {n} vertices, {name} has {}", values.len()));
                }
                None => count = Some((name, values.len())),
                _ => {}
            }
            mesh.try_insert_attribute(attr, values).map_err(|_| RENDER_ONLY)?;
        }
    }
    if let Some(ix) = o.get("indices") {
        match indices_from(ix)? {
            Some(ix) => mesh.try_insert_indices(ix).map_err(|_| RENDER_ONLY)?,
            None => {
                mesh.try_remove_indices().map_err(|_| RENDER_ONLY)?;
            }
        }
    }
    check_indices(&mesh)?;
    Ok(mesh)
}

/// wgpu's topologies, by the variant names Bevy's API spells (`PrimitiveTopology` is not reflected).
const TOPOLOGIES: [(PrimitiveTopology, &str); 5] = [
    (PrimitiveTopology::PointList, "PointList"),
    (PrimitiveTopology::LineList, "LineList"),
    (PrimitiveTopology::LineStrip, "LineStrip"),
    (PrimitiveTopology::TriangleList, "TriangleList"),
    (PrimitiveTopology::TriangleStrip, "TriangleStrip"),
];

fn topology_name(t: PrimitiveTopology) -> &'static str {
    TOPOLOGIES.iter().find(|(v, _)| *v == t).map_or("TriangleList", |(_, n)| n)
}

fn topology_from(json: &Value) -> Result<PrimitiveTopology, String> {
    let name = json.as_str().unwrap_or_default();
    TOPOLOGIES
        .iter()
        .find(|(_, n)| *n == name)
        .map(|(v, _)| *v)
        .ok_or_else(|| format!("primitive_topology is one of {}, not {json}", TOPOLOGIES.map(|(_, n)| n).join(", ")))
}

/// `json` written over `default` by its reflected shape.
fn field<T: Reflect>(slots: &Slots, mut default: T, json: &Value, name: &str) -> Result<T, String> {
    reflect_value::write_with(slots.cx(), &mut default, None, json).map_err(|e| format!("{name}: {e}"))?;
    Ok(default)
}

/// `from`'s attributes and indices into `to`, a mesh of another topology.
fn copy_data(from: &Mesh, to: &mut Mesh) -> Result<(), String> {
    for (attr, values) in from.try_attributes().map_err(|_| RENDER_ONLY)? {
        to.try_insert_attribute(attr.clone(), values.clone()).map_err(|_| RENDER_ONLY)?;
    }
    if let Some(ix) = from.try_indices_option().map_err(|_| RENDER_ONLY)? {
        to.try_insert_indices(ix.clone()).map_err(|_| RENDER_ONLY)?;
    }
    Ok(())
}

/// Every index names a vertex, when the mesh has any attributes.
fn check_indices(mesh: &Mesh) -> Result<(), String> {
    let Some(n) = mesh.try_attributes().map_err(|_| RENDER_ONLY)?.map(|(_, v)| v.len()).next() else {
        return Ok(());
    };
    let Some(ix) = mesh.try_indices_option().map_err(|_| RENDER_ONLY)? else {
        return Ok(());
    };
    match ix.iter().enumerate().find(|(_, i)| *i >= n) {
        Some((at, i)) => Err(format!("indices[{at}] is {i}, but the mesh has {n} vertices")),
        None => Ok(()),
    }
}

fn indices_from(json: &Value) -> Result<Option<Indices>, String> {
    if json.is_null() {
        return Ok(None);
    }
    let o = json.as_object().filter(|o| o.len() == 1).ok_or_else(|| format!("indices are {{ U16: [...] }}, {{ U32: [...] }} or null, not {json}"))?;
    match o.iter().next() {
        Some((k, v)) if k == "U16" => Ok(Some(Indices::U16(scalars(v).map_err(|e| format!("indices.U16: {e}"))?))),
        Some((k, v)) if k == "U32" => Ok(Some(Indices::U32(scalars(v).map_err(|e| format!("indices.U32: {e}"))?))),
        _ => Err(format!("indices are {{ U16: [...] }}, {{ U32: [...] }} or null, not {json}")),
    }
}

fn values_json(v: &VertexAttributeValues) -> Value {
    use VertexAttributeValues as V;
    fn one<T: Copy + Into<f64>>(x: &[T]) -> Vec<Value> {
        x.iter().map(|c| json!((*c).into())).collect()
    }
    fn many<T: Copy + Into<f64>, const N: usize>(x: &[[T; N]]) -> Vec<Value> {
        x.iter().map(|a| Value::Array(a.iter().map(|c| json!((*c).into())).collect())).collect()
    }
    Value::Array(match v {
        V::Uint8(x) | V::Unorm8(x) => one(x),
        V::Uint8x2(x) | V::Unorm8x2(x) => many(x),
        V::Uint8x4(x) | V::Unorm8x4(x) | V::Unorm8x4Bgra(x) => many(x),
        V::Sint8(x) | V::Snorm8(x) => one(x),
        V::Sint8x2(x) | V::Snorm8x2(x) => many(x),
        V::Sint8x4(x) | V::Snorm8x4(x) => many(x),
        V::Uint16(x) | V::Unorm16(x) => one(x),
        V::Uint16x2(x) | V::Unorm16x2(x) => many(x),
        V::Uint16x4(x) | V::Unorm16x4(x) => many(x),
        V::Sint16(x) | V::Snorm16(x) => one(x),
        V::Sint16x2(x) | V::Snorm16x2(x) => many(x),
        V::Sint16x4(x) | V::Snorm16x4(x) => many(x),
        V::Float16(x) => x.iter().map(|c| json!(c.to_f32())).collect(),
        V::Float16x2(x) => x.iter().map(|a| json!(a.map(|c| c.to_f32()))).collect(),
        V::Float16x4(x) => x.iter().map(|a| json!(a.map(|c| c.to_f32()))).collect(),
        V::Float32(x) => one(x),
        V::Float32x2(x) => many(x),
        V::Float32x3(x) => many(x),
        V::Float32x4(x) => many(x),
        V::Uint32(x) | V::Unorm10_10_10_2(x) => one(x),
        V::Uint32x2(x) => many(x),
        V::Uint32x3(x) => many(x),
        V::Uint32x4(x) => many(x),
        V::Sint32(x) => one(x),
        V::Sint32x2(x) => many(x),
        V::Sint32x3(x) => many(x),
        V::Sint32x4(x) => many(x),
        V::Float64(x) => one(x),
        V::Float64x2(x) => many(x),
        V::Float64x3(x) => many(x),
        V::Float64x4(x) => many(x),
    })
}

/// A number of a vertex or index component type, from JSON.
trait Component: Sized + Copy {
    const NAME: &'static str;
    fn from_f64(x: f64) -> Option<Self>;
}

macro_rules! int_component {
    ($($t:ty),*) => {$(
        impl Component for $t {
            const NAME: &'static str = stringify!($t);
            fn from_f64(x: f64) -> Option<Self> {
                (x.fract() == 0.0 && x >= <$t>::MIN as f64 && x <= <$t>::MAX as f64).then_some(x as $t)
            }
        }
    )*};
}
int_component!(u8, i8, u16, i16, u32, i32);

impl Component for f32 {
    const NAME: &'static str = "f32";
    fn from_f64(x: f64) -> Option<Self> {
        Some(x as f32)
    }
}

impl Component for f64 {
    const NAME: &'static str = "f64";
    fn from_f64(x: f64) -> Option<Self> {
        Some(x)
    }
}

fn component<T: Component>(v: &Value) -> Result<T, String> {
    v.as_f64().and_then(T::from_f64).ok_or_else(|| format!("expected a {}, got {v}", T::NAME))
}

fn list(json: &Value) -> Result<&Vec<Value>, String> {
    json.as_array().ok_or_else(|| format!("expected an array, got {json}"))
}

fn scalars<T: Component>(json: &Value) -> Result<Vec<T>, String> {
    list(json)?.iter().enumerate().map(|(i, v)| component(v).map_err(|e| format!("[{i}]: {e}"))).collect()
}

fn tuples<T: Component + Default, const N: usize>(json: &Value) -> Result<Vec<[T; N]>, String> {
    list(json)?
        .iter()
        .enumerate()
        .map(|(i, v)| {
            let a = v.as_array().filter(|a| a.len() == N).ok_or_else(|| format!("[{i}]: expected {N} numbers, got {v}"))?;
            let mut out = [T::default(); N];
            for (o, c) in out.iter_mut().zip(a) {
                *o = component(c).map_err(|e| format!("[{i}]: {e}"))?;
            }
            Ok(out)
        })
        .collect()
}

/// An attribute's values from JSON, in its format.
fn values_from(format: VertexFormat, json: &Value) -> Result<VertexAttributeValues, String> {
    use VertexAttributeValues as V;
    use VertexFormat as F;
    Ok(match format {
        F::Uint8 => V::Uint8(scalars(json)?),
        F::Uint8x2 => V::Uint8x2(tuples(json)?),
        F::Uint8x4 => V::Uint8x4(tuples(json)?),
        F::Sint8 => V::Sint8(scalars(json)?),
        F::Sint8x2 => V::Sint8x2(tuples(json)?),
        F::Sint8x4 => V::Sint8x4(tuples(json)?),
        F::Unorm8 => V::Unorm8(scalars(json)?),
        F::Unorm8x2 => V::Unorm8x2(tuples(json)?),
        F::Unorm8x4 => V::Unorm8x4(tuples(json)?),
        F::Snorm8 => V::Snorm8(scalars(json)?),
        F::Snorm8x2 => V::Snorm8x2(tuples(json)?),
        F::Snorm8x4 => V::Snorm8x4(tuples(json)?),
        F::Uint16 => V::Uint16(scalars(json)?),
        F::Uint16x2 => V::Uint16x2(tuples(json)?),
        F::Uint16x4 => V::Uint16x4(tuples(json)?),
        F::Sint16 => V::Sint16(scalars(json)?),
        F::Sint16x2 => V::Sint16x2(tuples(json)?),
        F::Sint16x4 => V::Sint16x4(tuples(json)?),
        F::Unorm16 => V::Unorm16(scalars(json)?),
        F::Unorm16x2 => V::Unorm16x2(tuples(json)?),
        F::Unorm16x4 => V::Unorm16x4(tuples(json)?),
        F::Snorm16 => V::Snorm16(scalars(json)?),
        F::Snorm16x2 => V::Snorm16x2(tuples(json)?),
        F::Snorm16x4 => V::Snorm16x4(tuples(json)?),
        F::Float32 => V::Float32(scalars(json)?),
        F::Float32x2 => V::Float32x2(tuples(json)?),
        F::Float32x3 => V::Float32x3(tuples(json)?),
        F::Float32x4 => V::Float32x4(tuples(json)?),
        F::Uint32 => V::Uint32(scalars(json)?),
        F::Uint32x2 => V::Uint32x2(tuples(json)?),
        F::Uint32x3 => V::Uint32x3(tuples(json)?),
        F::Uint32x4 => V::Uint32x4(tuples(json)?),
        F::Sint32 => V::Sint32(scalars(json)?),
        F::Sint32x2 => V::Sint32x2(tuples(json)?),
        F::Sint32x3 => V::Sint32x3(tuples(json)?),
        F::Sint32x4 => V::Sint32x4(tuples(json)?),
        F::Float64 => V::Float64(scalars(json)?),
        F::Float64x2 => V::Float64x2(tuples(json)?),
        F::Float64x3 => V::Float64x3(tuples(json)?),
        F::Float64x4 => V::Float64x4(tuples(json)?),
        F::Unorm10_10_10_2 => V::Unorm10_10_10_2(scalars(json)?),
        F::Unorm8x4Bgra => V::Unorm8x4Bgra(tuples(json)?),
        other => return Err(format!("{other:?} attributes are read-only from a script")),
    })
}

/// Bevy's mesh operations, by the code the TS runtime sends (`meshes` in `bevy-ts/assets`), each
/// named as TS calls it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MeshOp {
    ComputeNormals = 0,
    ComputeFlatNormals = 1,
    ComputeSmoothNormals = 2,
    ComputeAreaWeightedNormals = 3,
    GenerateTangents = 4,
    DuplicateVertices = 5,
    RemoveIndices = 6,
}

impl MeshOp {
    pub const ALL: [MeshOp; 7] = [
        MeshOp::ComputeNormals,
        MeshOp::ComputeFlatNormals,
        MeshOp::ComputeSmoothNormals,
        MeshOp::ComputeAreaWeightedNormals,
        MeshOp::GenerateTangents,
        MeshOp::DuplicateVertices,
        MeshOp::RemoveIndices,
    ];

    pub fn name(self) -> &'static str {
        match self {
            MeshOp::ComputeNormals => "computeNormals",
            MeshOp::ComputeFlatNormals => "computeFlatNormals",
            MeshOp::ComputeSmoothNormals => "computeSmoothNormals",
            MeshOp::ComputeAreaWeightedNormals => "computeAreaWeightedNormals",
            MeshOp::GenerateTangents => "generateTangents",
            MeshOp::DuplicateVertices => "duplicateVertices",
            MeshOp::RemoveIndices => "removeIndices",
        }
    }
}

/// Fails with Bevy's text when the mesh's data went to the render world.
pub fn access(mesh: &Mesh, method: &str) -> Result<(), String> {
    mesh.try_attributes().map(|_| ()).map_err(|e| format!("{method}: {e}"))
}

/// Checks what Bevy asserts before `op`, so the op cannot panic.
pub fn check(mesh: &Mesh, op: MeshOp) -> Result<(), String> {
    let method = op.name();
    access(mesh, method)?;
    let triangles = || {
        if mesh.primitive_topology() == PrimitiveTopology::TriangleList {
            Ok(())
        } else {
            Err(format!("{method}: needs PrimitiveTopology.TriangleList, not {:?}", mesh.primitive_topology()))
        }
    };
    let positions = || match mesh.try_attribute_option(Mesh::ATTRIBUTE_POSITION) {
        Ok(Some(VertexAttributeValues::Float32x3(p))) => Ok(p.len()),
        _ => Err(format!("{method}: needs Mesh.ATTRIBUTE_POSITION")),
    };
    let indices = mesh.try_indices_option().ok().flatten();
    // Every index must name a vertex: Bevy indexes the attributes with them.
    let in_range = |count: usize| match indices {
        Some(i) if i.iter().any(|i| i >= count) => Err(format!("{method}: an index is past the last vertex ({count} vertices)")),
        _ => Ok(()),
    };
    match op {
        MeshOp::ComputeNormals => {
            triangles()?;
            let n = positions()?;
            in_range(n)
        }
        MeshOp::ComputeFlatNormals => {
            triangles()?;
            positions()?;
            if indices.is_some() {
                return Err(format!("{method}: needs a mesh without indices; call duplicateVertices first, or computeSmoothNormals"));
            }
            Ok(())
        }
        MeshOp::ComputeSmoothNormals | MeshOp::ComputeAreaWeightedNormals => {
            triangles()?;
            let n = positions()?;
            if indices.is_none() {
                return Err(format!("{method}: needs an indexed mesh"));
            }
            in_range(n)
        }
        // Bevy's `generate_tangents` returns its own errors for a missing attribute or another
        // topology, but indexes normals and UVs by position: short ones, or an index past any of
        // the three, would panic.
        MeshOp::GenerateTangents => {
            let len = |a: MeshVertexAttribute| mesh.try_attribute_option(a).ok().flatten().map(VertexAttributeValues::len);
            let (Some(p), Some(n), Some(uv)) = (len(Mesh::ATTRIBUTE_POSITION), len(Mesh::ATTRIBUTE_NORMAL), len(Mesh::ATTRIBUTE_UV_0)) else {
                return Ok(());
            };
            if n < p || uv < p {
                return Err(format!("{method}: needs a normal and a UV per position ({p} positions, {n} normals, {uv} UVs)"));
            }
            in_range(p)
        }
        MeshOp::DuplicateVertices => in_range(mesh.try_attributes().map_or(0, |a| a.map(|(_, v)| v.len()).min().unwrap_or(0))),
        MeshOp::RemoveIndices => Ok(()),
    }
}

/// Runs `op` after [`check`]; a refused op leaves the mesh as it was.
pub fn run(mesh: &mut Mesh, op: MeshOp) -> Result<(), String> {
    check(mesh, op)?;
    let method = op.name();
    let e = |e: bevy::mesh::MeshAccessError| format!("{method}: {e}");
    match op {
        MeshOp::ComputeNormals => mesh.try_compute_normals().map_err(e),
        MeshOp::ComputeFlatNormals => mesh.try_compute_flat_normals().map_err(e),
        MeshOp::ComputeSmoothNormals => mesh.try_compute_smooth_normals().map_err(e),
        MeshOp::ComputeAreaWeightedNormals => mesh.try_compute_area_weighted_normals().map_err(e),
        MeshOp::GenerateTangents => mesh.generate_tangents().map_err(|e| format!("{method}: {e}")),
        MeshOp::DuplicateVertices => mesh.try_duplicate_vertices().map_err(e),
        MeshOp::RemoveIndices => mesh.try_remove_indices().map(|_| ()).map_err(e),
    }
}
