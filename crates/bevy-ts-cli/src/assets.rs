//! `Meshes`, the engine's own exported param for `ResMut<Assets<Mesh>>` (`bevy-ts/assets`): Bevy's
//! `Mesh` API from TS. Vertex data crosses as typed-array views read in place, one call per
//! attribute, and is copied once into the mesh's own `Vec`; an attribute or index buffer of the
//! same length and format is overwritten in place, with no allocation. A mesh made with `Mesh.new`
//! stays in JS until `meshes.add`, which builds it here (ref 0, the mesh being built) and adds it
//! in one finish call.
//!
//! The runtime (`crates/bevy-ts-v8/js/core.js`, `meshesClass`) gives these calls Bevy's names:
//! `meshes.add`, `get`, `getMut`, `remove`, and `insertAttribute` and the rest on a mesh. Every
//! precondition Bevy asserts is checked first, so no call panics and a failed call leaves the mesh
//! as it was. A changed mesh sends one `Modified` per frame however many calls change it.

use bevy::asset::{AssetId, RenderAssetUsages};
use bevy::diagnostic::FrameCount;
use bevy::ecs::system::SystemParam;
use bevy::mesh::{Indices, MeshVertexAttribute, PrimitiveTopology, VertexAttributeValues};
use bevy::platform::collections::HashSet;
use bevy_ts::host::ScriptRef;
use bevy_ts::reflect_asset::ScriptAssets;
use bevy_ts_native::prelude::*;

/// Exports `Meshes`. Every engine adds it.
pub struct ScriptAssetsPlugin;

impl Plugin for ScriptAssetsPlugin {
    fn build(&self, app: &mut App) {
        app.ts_param::<Meshes<'static, 'static>>().ts_module("bevy-ts/assets", include_str!("../ts/assets.js"), include_str!("../ts/assets.d.ts"));
    }
}

/// The eight built-in attributes, by the index `Mesh.ATTRIBUTE_*` carries in JS.
const ATTRIBUTES: [MeshVertexAttribute; 8] = [
    Mesh::ATTRIBUTE_POSITION,
    Mesh::ATTRIBUTE_NORMAL,
    Mesh::ATTRIBUTE_UV_0,
    Mesh::ATTRIBUTE_UV_1,
    Mesh::ATTRIBUTE_TANGENT,
    Mesh::ATTRIBUTE_COLOR,
    Mesh::ATTRIBUTE_JOINT_WEIGHT,
    Mesh::ATTRIBUTE_JOINT_INDEX,
];

/// The JS name of each of [`ATTRIBUTES`], for messages.
const ATTRIBUTE_NAMES: [&str; 8] =
    ["ATTRIBUTE_POSITION", "ATTRIBUTE_NORMAL", "ATTRIBUTE_UV_0", "ATTRIBUTE_UV_1", "ATTRIBUTE_TANGENT", "ATTRIBUTE_COLOR", "ATTRIBUTE_JOINT_WEIGHT", "ATTRIBUTE_JOINT_INDEX"];

const TOPOLOGIES: [PrimitiveTopology; 5] =
    [PrimitiveTopology::PointList, PrimitiveTopology::LineList, PrimitiveTopology::LineStrip, PrimitiveTopology::TriangleList, PrimitiveTopology::TriangleStrip];

/// The mesh operations `apply` runs, by code.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Op {
    ComputeNormals = 0,
    ComputeFlatNormals = 1,
    ComputeSmoothNormals = 2,
    ComputeAreaWeightedNormals = 3,
    GenerateTangents = 4,
    DuplicateVertices = 5,
    RemoveIndices = 6,
}

impl Op {
    const ALL: [Op; 7] = [
        Op::ComputeNormals,
        Op::ComputeFlatNormals,
        Op::ComputeSmoothNormals,
        Op::ComputeAreaWeightedNormals,
        Op::GenerateTangents,
        Op::DuplicateVertices,
        Op::RemoveIndices,
    ];

    fn name(self) -> &'static str {
        match self {
            Op::ComputeNormals => "computeNormals",
            Op::ComputeFlatNormals => "computeFlatNormals",
            Op::ComputeSmoothNormals => "computeSmoothNormals",
            Op::ComputeAreaWeightedNormals => "computeAreaWeightedNormals",
            Op::GenerateTangents => "generateTangents",
            Op::DuplicateVertices => "duplicateVertices",
            Op::RemoveIndices => "removeIndices",
        }
    }
}

fn attribute(index: u32) -> Result<(MeshVertexAttribute, &'static str), String> {
    let i = index as usize;
    ATTRIBUTES.get(i).map(|a| (a.clone(), ATTRIBUTE_NAMES[i])).ok_or_else(|| format!("{index} is not a Mesh.ATTRIBUTE_*"))
}

/// Components per vertex of a built-in attribute's format.
fn components(a: &MeshVertexAttribute) -> usize {
    a.format.size() as usize / 4
}

/// Fails with Bevy's text when the mesh's data went to the render world.
fn access(mesh: &Mesh, method: &str) -> Result<(), String> {
    mesh.try_attributes().map(|_| ()).map_err(|e| format!("{method}: {e}"))
}

/// Writes `data` as `attr`, into the present `Vec` when its length and format match.
fn put_f32(mesh: &mut Mesh, attr: MeshVertexAttribute, name: &str, data: &[f32]) -> Result<(), String> {
    let n = components(&attr);
    if !data.len().is_multiple_of(n) {
        return Err(format!("insertAttribute: Mesh.{name} takes a Float32Array of {n} floats per vertex; {} floats is not a whole number of vertices", data.len()));
    }
    if let Ok(Some(present)) = mesh.try_attribute_mut_option(attr.id) {
        let same = match present {
            VertexAttributeValues::Float32x2(v) if n == 2 && v.len() * 2 == data.len() => Some(v.as_flattened_mut()),
            VertexAttributeValues::Float32x3(v) if n == 3 && v.len() * 3 == data.len() => Some(v.as_flattened_mut()),
            VertexAttributeValues::Float32x4(v) if n == 4 && v.len() * 4 == data.len() => Some(v.as_flattened_mut()),
            _ => None,
        };
        if let Some(dst) = same {
            dst.copy_from_slice(data);
            return Ok(());
        }
    }
    let values = match n {
        2 => VertexAttributeValues::Float32x2(data.as_chunks::<2>().0.to_vec()),
        3 => VertexAttributeValues::Float32x3(data.as_chunks::<3>().0.to_vec()),
        _ => VertexAttributeValues::Float32x4(data.as_chunks::<4>().0.to_vec()),
    };
    mesh.try_insert_attribute(attr, values).map_err(|e| format!("insertAttribute: {e}"))
}

fn put_u16(mesh: &mut Mesh, attr: MeshVertexAttribute, name: &str, data: &[u16]) -> Result<(), String> {
    if !data.len().is_multiple_of(4) {
        return Err(format!("insertAttribute: Mesh.{name} takes a Uint16Array of 4 per vertex; {} is not a whole number of vertices", data.len()));
    }
    if let Ok(Some(VertexAttributeValues::Uint16x4(v))) = mesh.try_attribute_mut_option(attr.id)
        && v.len() * 4 == data.len()
    {
        v.as_flattened_mut().copy_from_slice(data);
        return Ok(());
    }
    mesh.try_insert_attribute(attr, VertexAttributeValues::Uint16x4(data.as_chunks::<4>().0.to_vec())).map_err(|e| format!("insertAttribute: {e}"))
}

fn put_indices_u16(mesh: &mut Mesh, data: &[u16]) -> Result<(), String> {
    if let Ok(Some(Indices::U16(v))) = mesh.try_indices_mut_option()
        && v.len() == data.len()
    {
        v.copy_from_slice(data);
        return Ok(());
    }
    mesh.try_insert_indices(Indices::U16(data.to_vec())).map_err(|e| format!("insertIndices: {e}"))
}

fn put_indices_u32(mesh: &mut Mesh, data: &[u32]) -> Result<(), String> {
    if let Ok(Some(Indices::U32(v))) = mesh.try_indices_mut_option()
        && v.len() == data.len()
    {
        v.copy_from_slice(data);
        return Ok(());
    }
    mesh.try_insert_indices(Indices::U32(data.to_vec())).map_err(|e| format!("insertIndices: {e}"))
}

/// Checks what Bevy asserts before `op`, so the op cannot panic.
fn check(mesh: &Mesh, op: Op) -> Result<(), String> {
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
        Op::ComputeNormals => {
            triangles()?;
            let n = positions()?;
            in_range(n)
        }
        Op::ComputeFlatNormals => {
            triangles()?;
            positions()?;
            if indices.is_some() {
                return Err(format!("{method}: needs a mesh without indices; call duplicateVertices first, or computeSmoothNormals"));
            }
            Ok(())
        }
        Op::ComputeSmoothNormals | Op::ComputeAreaWeightedNormals => {
            triangles()?;
            let n = positions()?;
            if indices.is_none() {
                return Err(format!("{method}: needs an indexed mesh"));
            }
            in_range(n)
        }
        // Bevy's `generate_tangents` returns its own errors.
        Op::GenerateTangents => Ok(()),
        Op::DuplicateVertices => in_range(mesh.try_attributes().map_or(0, |a| a.map(|(_, v)| v.len()).min().unwrap_or(0))),
        Op::RemoveIndices => Ok(()),
    }
}

fn run(mesh: &mut Mesh, op: Op) -> Result<(), String> {
    check(mesh, op)?;
    let method = op.name();
    let e = |e: bevy::mesh::MeshAccessError| format!("{method}: {e}");
    match op {
        Op::ComputeNormals => mesh.try_compute_normals().map_err(e),
        Op::ComputeFlatNormals => mesh.try_compute_flat_normals().map_err(e),
        Op::ComputeSmoothNormals => mesh.try_compute_smooth_normals().map_err(e),
        Op::ComputeAreaWeightedNormals => mesh.try_compute_area_weighted_normals().map_err(e),
        Op::GenerateTangents => mesh.generate_tangents().map_err(|e| format!("{method}: {e}")),
        Op::DuplicateVertices => mesh.try_duplicate_vertices().map_err(e),
        Op::RemoveIndices => mesh.try_remove_indices().map(|_| ()).map_err(e),
    }
}

/// What every TS system taking `Meshes` shares: the mesh `meshes.add` is building, and which
/// meshes have sent `Modified` this frame.
#[derive(Default)]
pub struct MeshesState {
    building: Option<Mesh>,
    frame: u32,
    modified: HashSet<AssetId<Mesh>>,
}

/// `ResMut<Assets<Mesh>>` for TS: Bevy's meshes, and the script's refs to them.
#[derive(SystemParam)]
pub struct Meshes<'w, 's> {
    meshes: ResMut<'w, Assets<Mesh>>,
    refs: Res<'w, ScriptAssets>,
    frame: Res<'w, FrameCount>,
    state: Local<'s, MeshesState>,
}

impl Meshes<'_, '_> {
    /// The mesh `r` names, to read; ref 0 is the mesh being built.
    fn mesh(&self, r: ScriptRef<Mesh>) -> Result<&Mesh, String> {
        if r.0 == 0 {
            return self.state.building.as_ref().ok_or_else(|| "no mesh is being built".to_string());
        }
        let h = r.handle(&self.refs)?;
        self.meshes.get(&h).ok_or_else(|| format!("asset ref {}: the mesh is gone", r.0))
    }

    /// The mesh `r` names, to change: the first change in a frame marks it `Modified`.
    fn mesh_mut(&mut self, r: ScriptRef<Mesh>) -> Result<&mut Mesh, String> {
        if r.0 == 0 {
            return self.state.building.as_mut().ok_or_else(|| "no mesh is being built".to_string());
        }
        let id = r.handle(&self.refs)?.id();
        let state = &mut *self.state;
        if state.frame != self.frame.0 {
            state.frame = self.frame.0;
            state.modified.clear();
        }
        let gone = || format!("asset ref {}: the mesh is gone", r.0);
        if state.modified.insert(id) {
            self.meshes.get_mut(id).map(|m| m.into_inner()).ok_or_else(gone)
        } else {
            self.meshes.get_mut_untracked(id).ok_or_else(gone)
        }
    }

    /// Checks with `check`, then changes the mesh with `f`.
    fn change(&mut self, r: ScriptRef<Mesh>, check: impl FnOnce(&Mesh) -> Result<(), String>, f: impl FnOnce(&mut Mesh) -> Result<(), String>) -> Result<(), String> {
        check(self.mesh(r)?)?;
        f(self.mesh_mut(r)?)
    }
}

#[export(ts = "Meshes", module = "bevy-ts/assets", stands_for = "ResMut<Assets<Mesh>>")]
impl Meshes<'_, '_> {
    /// Starts building a mesh for `meshes.add`: `Mesh::new(topology, usage)`, as ref 0.
    pub fn begin(&mut self, topology: u32, usage: u32) -> Result<(), String> {
        let topology = *TOPOLOGIES.get(topology as usize).ok_or_else(|| format!("{topology} is not a PrimitiveTopology"))?;
        let usage = RenderAssetUsages::from_bits(usage as u8).filter(|u| !u.is_empty()).ok_or_else(|| format!("{usage} is not a RenderAssetUsages"))?;
        self.state.building = Some(Mesh::new(topology, usage));
        Ok(())
    }

    /// Adds the mesh being built and returns a strong ref to it.
    pub fn finish(&mut self) -> Result<ScriptRef<Mesh>, String> {
        let mesh = self.state.building.take().ok_or("no mesh is being built")?;
        let handle = self.meshes.add(mesh);
        Ok(ScriptRef::new(self.refs.insert_strong(handle.untyped())))
    }

    /// Drops the mesh being built, after a failed step.
    pub fn abandon(&mut self) {
        self.state.building = None;
    }

    /// `insertAttribute` with a `Float32Array`.
    pub fn insert_attribute_f32(&mut self, mesh: ScriptRef<Mesh>, attribute: u32, data: &[f32]) -> Result<(), String> {
        let (attr, name) = self::attribute(attribute)?;
        if attr.id == Mesh::ATTRIBUTE_JOINT_INDEX.id {
            return Err(format!("insertAttribute: Mesh.{name} takes a Uint16Array of 4 per vertex, not a Float32Array"));
        }
        self.change(mesh, |m| access(m, "insertAttribute"), |m| put_f32(m, attr, name, data))
    }

    /// `insertAttribute` with a `Uint16Array`.
    pub fn insert_attribute_u16(&mut self, mesh: ScriptRef<Mesh>, attribute: u32, data: &[u16]) -> Result<(), String> {
        let (attr, name) = self::attribute(attribute)?;
        if attr.id != Mesh::ATTRIBUTE_JOINT_INDEX.id {
            return Err(format!("insertAttribute: Mesh.{name} takes a Float32Array of {} floats per vertex, not a Uint16Array", components(&attr)));
        }
        self.change(mesh, |m| access(m, "insertAttribute"), |m| put_u16(m, attr, name, data))
    }

    pub fn remove_attribute(&mut self, mesh: ScriptRef<Mesh>, attribute: u32) -> Result<(), String> {
        let (attr, _) = self::attribute(attribute)?;
        self.change(mesh, |m| access(m, "removeAttribute"), |m| m.try_remove_attribute(attr.id).map(|_| ()).map_err(|e| format!("removeAttribute: {e}")))
    }

    /// `insertIndices(Indices.U16(...))`.
    pub fn insert_indices_u16(&mut self, mesh: ScriptRef<Mesh>, data: &[u16]) -> Result<(), String> {
        self.change(mesh, |m| access(m, "insertIndices"), |m| put_indices_u16(m, data))
    }

    /// `insertIndices(Indices.U32(...))`.
    pub fn insert_indices_u32(&mut self, mesh: ScriptRef<Mesh>, data: &[u32]) -> Result<(), String> {
        self.change(mesh, |m| access(m, "insertIndices"), |m| put_indices_u32(m, data))
    }

    /// Runs `ops`, codes of `Op` in order (`computeNormals`, `generateTangents`, ...). Each is
    /// checked before it runs; the first that fails stops the rest.
    pub fn apply(&mut self, mesh: ScriptRef<Mesh>, ops: &[u8]) -> Result<(), String> {
        let ops: Vec<Op> = ops.iter().map(|&c| Op::ALL.get(c as usize).copied().ok_or_else(|| format!("{c} is not a mesh operation"))).collect::<Result<_, _>>()?;
        let Some(&first) = ops.first() else { return Ok(()) };
        self.change(mesh, |m| check(m, first), |m| ops.into_iter().try_for_each(|op| run(m, op)))
    }

    /// `countVertices()`.
    pub fn count_vertices(&self, mesh: ScriptRef<Mesh>) -> Result<u32, String> {
        let m = self.mesh(mesh)?;
        access(m, "countVertices")?;
        Ok(m.count_vertices() as u32)
    }

    /// The topology's index in `PrimitiveTopology` and the usage bits.
    pub fn topology(&self, mesh: ScriptRef<Mesh>) -> Result<u32, String> {
        let t = self.mesh(mesh)?.primitive_topology();
        Ok(TOPOLOGIES.iter().position(|&x| x == t).unwrap_or(0) as u32)
    }

    pub fn usage(&self, mesh: ScriptRef<Mesh>) -> Result<u32, String> {
        Ok(self.mesh(mesh)?.asset_usage.bits() as u32)
    }

    /// How many numbers `attribute` holds (vertices × components), or `undefined` when unset.
    pub fn attribute_len(&self, mesh: ScriptRef<Mesh>, attribute: u32) -> Result<Option<u32>, String> {
        let (attr, _) = self::attribute(attribute)?;
        let m = self.mesh(mesh)?;
        access(m, "attribute")?;
        Ok(m.try_attribute_option(attr.id).ok().flatten().map(|v| (v.len() * components(&attr)) as u32))
    }

    /// Copies a float attribute into `out`.
    pub fn read_attribute_f32(&self, mesh: ScriptRef<Mesh>, attribute: u32, out: &mut [f32]) -> Result<(), String> {
        let (attr, name) = self::attribute(attribute)?;
        let m = self.mesh(mesh)?;
        let src: &[f32] = match m.try_attribute(attr.id).map_err(|e| format!("attribute: {e}"))? {
            VertexAttributeValues::Float32x2(v) => v.as_flattened(),
            VertexAttributeValues::Float32x3(v) => v.as_flattened(),
            VertexAttributeValues::Float32x4(v) => v.as_flattened(),
            _ => return Err(format!("attribute: Mesh.{name} is not float data")),
        };
        out.get_mut(..src.len()).ok_or("attribute: out is too short")?.copy_from_slice(src);
        Ok(())
    }

    /// Copies `ATTRIBUTE_JOINT_INDEX` into `out`.
    pub fn read_attribute_u16(&self, mesh: ScriptRef<Mesh>, attribute: u32, out: &mut [u16]) -> Result<(), String> {
        let (attr, name) = self::attribute(attribute)?;
        let m = self.mesh(mesh)?;
        let VertexAttributeValues::Uint16x4(v) = m.try_attribute(attr.id).map_err(|e| format!("attribute: {e}"))? else {
            return Err(format!("attribute: Mesh.{name} is not Uint16 data"));
        };
        out.get_mut(..v.len() * 4).ok_or("attribute: out is too short")?.copy_from_slice(v.as_flattened());
        Ok(())
    }

    /// The index buffer's width (16 or 32), or `undefined` when the mesh has none.
    pub fn indices_width(&self, mesh: ScriptRef<Mesh>) -> Result<Option<u32>, String> {
        let m = self.mesh(mesh)?;
        Ok(m.try_indices_option().map_err(|e| format!("indices: {e}"))?.map(|i| match i {
            Indices::U16(_) => 16,
            Indices::U32(_) => 32,
        }))
    }

    pub fn indices_len(&self, mesh: ScriptRef<Mesh>) -> Result<u32, String> {
        Ok(self.mesh(mesh)?.try_indices_option().ok().flatten().map_or(0, |i| i.len()) as u32)
    }

    pub fn read_indices_u16(&self, mesh: ScriptRef<Mesh>, out: &mut [u16]) -> Result<(), String> {
        let Ok(Some(Indices::U16(v))) = self.mesh(mesh)?.try_indices_option() else { return Err("indices: not Indices.U16".into()) };
        out.get_mut(..v.len()).ok_or("indices: out is too short")?.copy_from_slice(v);
        Ok(())
    }

    pub fn read_indices_u32(&self, mesh: ScriptRef<Mesh>, out: &mut [u32]) -> Result<(), String> {
        let Ok(Some(Indices::U32(v))) = self.mesh(mesh)?.try_indices_option() else { return Err("indices: not Indices.U32".into()) };
        out.get_mut(..v.len()).ok_or("indices: out is too short")?.copy_from_slice(v);
        Ok(())
    }

    /// Whether ref `mesh` still names a mesh in `Assets<Mesh>`.
    pub fn contains(&self, mesh: ScriptRef<Mesh>) -> Result<bool, String> {
        let h = mesh.handle(&self.refs)?;
        Ok(self.meshes.contains(&h))
    }

    /// `meshes.remove(ref)`: Bevy's `Assets::remove`, at once; the ref goes stale.
    pub fn remove(&mut self, mesh: ScriptRef<Mesh>) -> Result<bool, String> {
        let h = mesh.handle(&self.refs)?;
        self.refs.drop(mesh.0)?;
        Ok(self.meshes.remove(&h).is_some())
    }
}
