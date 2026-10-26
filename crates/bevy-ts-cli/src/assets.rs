//! The engine's own asset params (`bevy-ts/assets`): `ScriptAssetServer` for `Res<AssetServer>`
//! (loads and their load state, below the meshes) and `Meshes` for `ResMut<Assets<Mesh>>`.
//!
//! `Meshes`, for `ResMut<Assets<Mesh>>`: Bevy's
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

use bevy::asset::{AssetId, AssetPath, DependencyLoadState, LoadState, RecursiveDependencyLoadState, RenderAssetUsages, UntypedAssetId, UntypedHandle};
use bevy::diagnostic::FrameCount;
use bevy::ecs::system::SystemParam;
use bevy::mesh::{Indices, MeshVertexAttribute, PrimitiveTopology, VertexAttributeValues, VertexFormat};
use bevy::platform::collections::HashSet;
use bevy_ts::host::{HostJson, ScriptRef, UntypedScriptRef};
use bevy_ts::mesh_data::{MeshOp, access, check, run};
use bevy_ts::reflect_asset::ScriptAssets;
use bevy_ts_native::prelude::*;

/// Exports `ScriptAssetServer` and `Meshes`. Every engine adds it.
pub struct ScriptAssetsPlugin;

impl Plugin for ScriptAssetsPlugin {
    fn build(&self, app: &mut App) {
        app.ts_param::<ScriptAssetServer<'static>>()
            .ts_param::<Meshes<'static, 'static>>().ts_module("bevy-ts/assets", include_str!("../ts/assets.js"), include_str!("../ts/assets.d.ts"));
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

fn attribute(index: u32) -> Result<(MeshVertexAttribute, &'static str), String> {
    let i = index as usize;
    ATTRIBUTES.get(i).map(|a| (a.clone(), ATTRIBUTE_NAMES[i])).ok_or_else(|| format!("{index} is not a Mesh.ATTRIBUTE_*"))
}

/// Numbers per vertex of a built-in attribute's format.
fn components(a: &MeshVertexAttribute) -> usize {
    match a.format {
        VertexFormat::Float32x2 => 2,
        VertexFormat::Float32x3 => 3,
        _ => 4,
    }
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

/// One TS system's `Meshes` state (a `Local`): the mesh `meshes.add` is building, and which
/// meshes this system has marked `Modified` this frame. Two systems that change one mesh in a
/// frame send two, as two `get_mut` calls in Rust do.
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

    /// Runs `ops`, codes of `MeshOp` in order (`computeNormals`, `generateTangents`, ...). Each is
    /// checked before it runs; the first that fails stops the rest.
    pub fn apply(&mut self, mesh: ScriptRef<Mesh>, ops: &[u8]) -> Result<(), String> {
        let ops: Vec<MeshOp> = ops.iter().map(|&c| MeshOp::ALL.get(c as usize).copied().ok_or_else(|| format!("{c} is not a mesh operation"))).collect::<Result<_, _>>()?;
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

/// A load state as a code: `NotLoaded` 0, `Loading` 1, `Loaded` 2, `Failed` 3.
fn state_code(state: &LoadState) -> u32 {
    match state {
        LoadState::NotLoaded => 0,
        LoadState::Loading => 1,
        LoadState::Loaded => 2,
        LoadState::Failed(_) => 3,
    }
}

/// Copies `text`'s UTF-8 into `out`, as much as fits; returns its whole length in bytes.
fn put_text(text: &str, out: &mut [u8]) -> u32 {
    let n = text.len().min(out.len());
    out[..n].copy_from_slice(&text.as_bytes()[..n]);
    text.len() as u32
}

fn utf8<'a>(bytes: &'a [u8], what: &str) -> Result<&'a str, String> {
    std::str::from_utf8(bytes).map_err(|e| format!("{what} is not UTF-8: {e}"))
}

/// `Res<AssetServer>` for TS: Bevy's loads, held by the script as strong refs.
#[derive(SystemParam)]
pub struct ScriptAssetServer<'w> {
    server: Res<'w, AssetServer>,
    refs: Res<'w, ScriptAssets>,
    registry: Res<'w, AppTypeRegistry>,
}

impl ScriptAssetServer<'_> {
    fn id(&self, r: UntypedScriptRef) -> Result<bevy::asset::UntypedAssetId, String> {
        Ok(r.handle(&self.refs)?.id())
    }

    /// `id`, an `AssetId` as JSON, of the asset type at `type_path`: a registered type, else the
    /// argument of a registered `AssetEvent<A>` (`Gltf` is not registered itself).
    fn asset_id(&self, id: &HostJson, type_path: &[u8]) -> Result<UntypedAssetId, String> {
        let type_path = utf8(type_path, "the asset type")?;
        let registry = self.registry.read();
        let type_id = match registry.get_with_type_path(type_path) {
            Some(r) => r.type_id(),
            None => registry
                .get_with_type_path(&format!("bevy_asset::event::AssetEvent<{type_path}>"))
                .and_then(|e| e.type_info().generics().iter().next().map(|g| g.type_id()))
                .ok_or_else(|| format!("{type_path} is not a registered asset type"))?,
        };
        bevy_ts::reflect_asset::id_of_json(&registry, type_id, &id.0)
    }

    /// A state of `id` as a code: `which` 0 load, 1 dependency, 2 recursive (`NotLoaded` 0,
    /// `Loading` 1, `Loaded` 2, `Failed` 3); 3 `is_loaded`, 4 with direct dependencies, 5 with
    /// all (1 for true).
    fn code(&self, id: UntypedAssetId, which: u32) -> u32 {
        let s = &self.server;
        match which {
            0 => state_code(&s.load_state(id)),
            1 => match s.dependency_load_state(id) {
                DependencyLoadState::NotLoaded => 0,
                DependencyLoadState::Loading => 1,
                DependencyLoadState::Loaded => 2,
                DependencyLoadState::Failed(_) => 3,
            },
            2 => match s.recursive_dependency_load_state(id) {
                RecursiveDependencyLoadState::NotLoaded => 0,
                RecursiveDependencyLoadState::Loading => 1,
                RecursiveDependencyLoadState::Loaded => 2,
                RecursiveDependencyLoadState::Failed(_) => 3,
            },
            3 => s.is_loaded(id) as u32,
            4 => s.is_loaded_with_direct_dependencies(id) as u32,
            _ => s.is_loaded_with_dependencies(id) as u32,
        }
    }

    /// The error of `id`'s failed state (`which`: 0 load, 1 dependency, 2 recursive).
    fn failed(&self, id: UntypedAssetId, which: u32) -> Option<String> {
        match which {
            0 => match self.server.load_state(id) {
                LoadState::Failed(e) => Some(e.to_string()),
                _ => None,
            },
            1 => match self.server.dependency_load_state(id) {
                DependencyLoadState::Failed(e) => Some(e.to_string()),
                _ => None,
            },
            _ => match self.server.recursive_dependency_load_state(id) {
                RecursiveDependencyLoadState::Failed(e) => Some(e.to_string()),
                _ => None,
            },
        }
    }
}

#[export(ts = "ScriptAssetServer", module = "bevy-ts/assets", stands_for = "Res<AssetServer>")]
impl ScriptAssetServer<'_> {
    /// `AssetServer::load`: the asset at `path` (UTF-8), of the asset type at `type_path` when
    /// given, else of the type the path's extension loads. Returns a strong ref; the same path
    /// gives the same ref. The runtime gives it the asset's type.
    pub fn load_path(&self, path: &[u8], type_path: &[u8]) -> Result<UntypedScriptRef, String> {
        let path = utf8(path, "load: the path")?;
        let type_path = utf8(type_path, "load: the type")?;
        let parsed = AssetPath::try_parse(path).map_err(|e| format!("load: {path:?} is not an asset path: {e}"))?;
        let type_id = if type_path.is_empty() {
            if parsed.label().is_some() {
                return Err(format!("load: {path:?} names a labelled asset; pass its type, server.load({path:?}, Mesh)"));
            }
            let ext = parsed.get_full_extension().ok_or_else(|| format!("load: {path:?} has no extension; pass the asset type"))?;
            let loader = bevy::tasks::futures::now_or_never(self.server.get_asset_loader_with_extension(&ext))
                .ok_or_else(|| format!("load: the loader for .{ext} is not ready yet; pass the asset type"))?
                .map_err(|e| format!("load: {e}"))?;
            loader.asset_type_id()
        } else {
            let registry = self.registry.read();
            registry.get_with_type_path(type_path).ok_or_else(|| format!("load: {type_path} is not a registered type"))?.type_id()
        };
        let handle: UntypedHandle = self.server.load_builder().load_erased(type_id, parsed);
        Ok(UntypedScriptRef(self.refs.insert_strong(handle)))
    }

    /// Copies the type path of ref `r`'s asset into `out`; returns its length.
    pub fn type_path(&self, r: UntypedScriptRef, out: &mut [u8]) -> Result<u32, String> {
        let id = r.handle(&self.refs)?.type_id();
        let registered = self.registry.read().get(id).map(|t| t.type_info().type_path());
        // An asset type with no reflection (glTF's `Gltf`) is named by its loader.
        let path = match registered {
            Some(path) => path,
            None => bevy::tasks::futures::now_or_never(self.server.get_asset_loader_with_asset_type_id(id))
                .and_then(Result::ok)
                .map(|l| l.asset_type_name())
                .ok_or("type_path: the asset type is neither registered nor loaded by a loader")?,
        };
        Ok(put_text(path, out))
    }

    /// `load_state` as a code.
    pub fn load_state_code(&self, r: UntypedScriptRef) -> Result<u32, String> {
        Ok(self.code(self.id(r)?, 0))
    }

    pub fn dependency_load_state_code(&self, r: UntypedScriptRef) -> Result<u32, String> {
        Ok(self.code(self.id(r)?, 1))
    }

    pub fn recursive_dependency_load_state_code(&self, r: UntypedScriptRef) -> Result<u32, String> {
        Ok(self.code(self.id(r)?, 2))
    }

    /// The error of a failed state (`which`: 0 load, 1 dependency, 2 recursive), copied into
    /// `out`; returns its whole length, 0 when that state is not failed.
    pub fn failed_text(&self, r: UntypedScriptRef, which: u32, out: &mut [u8]) -> Result<u32, String> {
        Ok(self.failed(self.id(r)?, which).map_or(0, |e| put_text(&e, out)))
    }

    pub fn is_loaded(&self, r: UntypedScriptRef) -> Result<bool, String> {
        Ok(self.code(self.id(r)?, 3) == 1)
    }

    pub fn is_loaded_with_direct_dependencies(&self, r: UntypedScriptRef) -> Result<bool, String> {
        Ok(self.code(self.id(r)?, 4) == 1)
    }

    pub fn is_loaded_with_dependencies(&self, r: UntypedScriptRef) -> Result<bool, String> {
        Ok(self.code(self.id(r)?, 5) == 1)
    }

    /// The load-state methods for an `AssetId<A>` (`id`, with `A` at `type_path`, UTF-8), as
    /// `ref.id` and an `AssetEvent<A>` give it: `which` 0 to 2 the state codes, 3 to 5 the
    /// `is_loaded` forms (1 for true).
    pub fn id_code(&self, id: HostJson, type_path: &[u8], which: u32) -> Result<u32, String> {
        Ok(self.code(self.asset_id(&id, type_path)?, which))
    }

    /// [`Self::failed_text`] for an `AssetId<A>`.
    pub fn id_failed_text(&self, id: HostJson, type_path: &[u8], which: u32, out: &mut [u8]) -> Result<u32, String> {
        Ok(self.failed(self.asset_id(&id, type_path)?, which).map_or(0, |e| put_text(&e, out)))
    }
}
