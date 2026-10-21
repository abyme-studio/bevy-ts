//! Material kinds as the live generation declares them, and their instances. The `materials`
//! section is checked with the generation (a bad schema fails it), then applied: a new or changed
//! kind gets its shaders rebuilt in place and its instances re-encoded by param name. A saved
//! `.wgsl` file replaces its shaders in place with no generation at all.

use crate::layout::{Bindings, ParamType, ParamsLayout, struct_name};
use crate::render::{Kind, ScriptMaterial};
use crate::wgsl;
use bevy::prelude::*;
use bevy_ts::backend::{ScriptError, SourceLocation};
use bevy_ts::log::{LogLevel, ScriptLog};
use serde::Deserialize;
use serde_json::{Map, Value};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

/// The manifest section `material()` writes.
pub const SECTION: &str = "materials";

#[derive(Clone, Debug, PartialEq, Deserialize)]
pub struct ParamDecl {
    pub name: String,
    #[serde(rename = "type")]
    pub ty: ParamType,
}

#[derive(Clone, Debug, PartialEq, Deserialize)]
#[serde(untagged)]
pub enum AlphaDecl {
    Named(String),
    Mask { mask: f32 },
}

/// One `material(name, {...})`.
#[derive(Clone, Debug, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MaterialDecl {
    pub name: String,
    pub params: Vec<ParamDecl>,
    #[serde(default)]
    pub textures: Vec<String>,
    pub fragment: PathBuf,
    #[serde(default)]
    pub vertex: Option<PathBuf>,
    pub alpha_mode: AlphaDecl,
    #[serde(default)]
    pub location: Option<SourceLocation>,
}

impl MaterialDecl {
    pub fn bindings(&self) -> Result<Bindings, String> {
        let params = ParamsLayout::new(self.params.iter().map(|p| (p.name.as_str(), &p.ty)))?;
        Ok(Bindings { struct_name: struct_name(&self.name), params, textures: self.textures.clone() })
    }

    pub fn alpha_mode(&self) -> Result<AlphaMode, String> {
        Ok(match &self.alpha_mode {
            AlphaDecl::Mask { mask } => AlphaMode::Mask(*mask),
            AlphaDecl::Named(n) => match n.as_str() {
                "opaque" => AlphaMode::Opaque,
                "blend" => AlphaMode::Blend,
                "premultiplied" => AlphaMode::Premultiplied,
                "add" => AlphaMode::Add,
                "multiply" => AlphaMode::Multiply,
                "alphaToCoverage" => AlphaMode::AlphaToCoverage,
                other => return Err(format!("unknown alphaMode {other:?}")),
            },
        })
    }

    /// The material's location, file shown relative to the working directory.
    pub fn at(&self) -> Option<SourceLocation> {
        self.location.clone().map(|mut l| {
            l.file = display(Path::new(l.file.strip_prefix("file://").unwrap_or(&l.file)));
            l
        })
    }
}

#[derive(Deserialize)]
struct Section {
    items: Vec<MaterialDecl>,
}

pub fn display(path: &Path) -> String {
    match std::env::current_dir().ok().and_then(|cwd| path.strip_prefix(cwd).ok().map(Path::to_path_buf)) {
        Some(p) => p.display().to_string(),
        None => path.display().to_string(),
    }
}

/// One shader stage of a kind: its file, the shader asset every instance draws with (replaced in
/// place on change), and the last source that compiled.
#[derive(Debug)]
pub struct Stage {
    pub file: PathBuf,
    pub handle: Handle<Shader>,
    /// The combined source the asset holds now; `None` until one compiles.
    pub source: Option<String>,
}

/// One live kind.
pub struct KindState {
    pub decl: MaterialDecl,
    pub kind: Arc<Kind>,
    pub fragment: Stage,
    pub vertex: Option<Stage>,
    /// Instances by key, each with the props it was made from.
    pub instances: BTreeMap<String, (Handle<ScriptMaterial>, Map<String, Value>)>,
}

/// Every live kind, and the kinds the generation being checked declares.
#[derive(Resource, Default)]
pub struct MaterialKinds {
    pub live: BTreeMap<String, KindState>,
    /// Set by the section's check, so `<material kind>` checks against the generation's kinds.
    pub pending: Option<(u64, BTreeMap<String, MaterialDecl>)>,
    /// Shader assets made, for tests.
    pub shaders_built: usize,
}

impl MaterialKinds {
    /// A kind's declaration, as the generation being checked declares it, else as live.
    pub fn decl(&self, generation: Option<u64>, name: &str) -> Option<&MaterialDecl> {
        match (&self.pending, generation) {
            (Some((g, decls)), Some(n)) if *g == n => decls.get(name),
            _ => self.live.get(name).map(|k| &k.decl),
        }
    }
}

/// The section's check: every declaration parses and lays out. WGSL problems do not fail a
/// generation; they are reported when the shader is built, and the old shader keeps drawing.
pub fn check(world: &mut World, generation: u64, bytes: &[u8], location: Option<SourceLocation>) -> Result<(), Vec<ScriptError>> {
    let section: Section = serde_json::from_slice(bytes).map_err(|e| vec![ScriptError::at(format!("materials section: {e}"), location.clone())])?;
    let mut errors = Vec::new();
    let mut decls = BTreeMap::new();
    for d in section.items {
        let at = d.at();
        if let Err(e) = d.bindings().and_then(|_| d.alpha_mode().map(|_| ())) {
            errors.push(ScriptError::at(format!("material {}: {e}", d.name), at.clone()));
        }
        for f in std::iter::once(&d.fragment).chain(d.vertex.as_ref()) {
            if !f.is_file() {
                errors.push(ScriptError::at(format!("material {}: no shader file {}", d.name, display(f)), at.clone()));
            }
        }
        decls.insert(d.name.clone(), d);
    }
    if !errors.is_empty() {
        return Err(errors);
    }
    world.resource_mut::<MaterialKinds>().pending = Some((generation, decls));
    Ok(())
}

/// The section's apply: kinds made, rebuilt or dropped to match the generation.
pub fn apply(world: &mut World, generation: u64, present: bool) {
    let decls = match world.resource_mut::<MaterialKinds>().pending.take() {
        Some((g, d)) if g == generation && present => d,
        _ if present => return,
        _ => BTreeMap::new(),
    };
    let mut kinds = std::mem::take(&mut world.resource_mut::<MaterialKinds>().live);
    kinds.retain(|name, _| decls.contains_key(name));
    for (name, decl) in decls {
        match kinds.remove(&name) {
            Some(mut k) if k.decl == decl => {
                // Same declaration: the files may still have changed since the last generation.
                refresh_stage(world, generation, &k.decl, &k.kind.bindings, &mut k.fragment);
                if let Some(v) = &mut k.vertex {
                    refresh_stage(world, generation, &k.decl, &k.kind.bindings, v);
                }
                kinds.insert(name, k);
            }
            Some(old) => {
                let k = rebuild(world, generation, decl, Some(old));
                kinds.insert(name, k);
            }
            None => {
                let k = rebuild(world, generation, decl, None);
                kinds.insert(name, k);
            }
        }
    }
    world.resource_mut::<MaterialKinds>().live = kinds;
}

fn new_stage(world: &mut World, file: &Path) -> Stage {
    // An empty placeholder until a source compiles: materials of the kind draw nothing.
    let handle = world.resource_mut::<Assets<Shader>>().reserve_handle();
    Stage { file: file.to_path_buf(), handle, source: None }
}

/// A kind built from `decl`, reusing the shader assets of `old` (replaced in place) and
/// re-encoding its instances by param name.
fn rebuild(world: &mut World, generation: u64, decl: MaterialDecl, old: Option<KindState>) -> KindState {
    let bindings = decl.bindings().expect("checked");
    let alpha = decl.alpha_mode().expect("checked");
    let (mut fragment, mut vertex, instances) = match old {
        Some(o) => (o.fragment, o.vertex, o.instances),
        None => (new_stage(world, &decl.fragment), None, BTreeMap::new()),
    };
    fragment.file = decl.fragment.clone();
    vertex = match (&decl.vertex, vertex) {
        (Some(f), Some(mut v)) => {
            v.file = f.clone();
            Some(v)
        }
        (Some(f), None) => Some(new_stage(world, f)),
        (None, _) => None,
    };
    // The generated declarations changed with the schema, so every stage rebuilds.
    fragment.source = None;
    if let Some(v) = &mut vertex {
        v.source = None;
    }
    refresh_stage(world, generation, &decl, &bindings, &mut fragment);
    if let Some(v) = &mut vertex {
        refresh_stage(world, generation, &decl, &bindings, v);
    }
    let kind = Arc::new(Kind::new(&decl.name, bindings, Some(fragment.handle.clone()), vertex.as_ref().map(|v| v.handle.clone()), alpha));
    let mut state = KindState { decl, kind, fragment, vertex, instances };
    let keys: Vec<String> = state.instances.keys().cloned().collect();
    for key in keys {
        let (handle, props) = state.instances[&key].clone();
        let (material, dropped) = encode(&state, &props);
        for p in dropped {
            world.resource_mut::<ScriptLog>().push(
                LogLevel::Warn,
                format!("material {}: param {p} is gone; its value is dropped", state.decl.name),
                state.decl.at(),
                generation,
            );
        }
        let material = material.with_textures(world_textures(world, &state, &props));
        if let Some(mut m) = world.resource_mut::<Assets<ScriptMaterial>>().get_mut(&handle) {
            *m = material;
        }
    }
    state.instances.retain(|_, (h, _)| world.resource::<Assets<ScriptMaterial>>().contains(h));
    state
}

/// Reads a stage's file and, if its combined source differs from what the asset holds, checks it
/// and replaces the asset in place. On an error the old shader stays.
fn refresh_stage(world: &mut World, generation: u64, decl: &MaterialDecl, bindings: &Bindings, stage: &mut Stage) {
    let text = match std::fs::read_to_string(&stage.file) {
        Ok(t) => t,
        Err(e) => {
            let msg = format!("material {}: cannot read {}: {e}", decl.name, display(&stage.file));
            world.resource_mut::<ScriptLog>().push(LogLevel::Error, msg, decl.at(), generation);
            return;
        }
    };
    let combined = wgsl::combine(&text, bindings, &decl.name);
    if stage.source.as_deref() == Some(combined.as_str()) {
        return;
    }
    if let Err(errors) = wgsl::check(world, &combined, &text, &stage.file, decl) {
        let mut log = world.resource_mut::<ScriptLog>();
        for e in errors {
            log.push(LogLevel::Error, e.message, e.location, generation);
        }
        return;
    }
    let path = format!("bevy_ts/materials/{}.wesl", shader_path(&decl.name, &stage.file));
    world.resource_mut::<Assets<Shader>>().insert(stage.handle.id(), Shader::from_wesl(combined.clone(), path)).expect("a reserved shader handle");
    world.resource_mut::<MaterialKinds>().shaders_built += 1;
    stage.source = Some(combined);
}

/// A shader asset's path: the kind and the file stem, as WESL module path segments.
fn shader_path(kind: &str, file: &Path) -> String {
    let seg = |s: &str| s.chars().map(|c| if c.is_ascii_alphanumeric() { c.to_ascii_lowercase() } else { '_' }).collect::<String>();
    let stem = file.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_default();
    format!("{}/{}", kind.split("::").map(seg).collect::<Vec<_>>().join("/"), seg(&stem))
}

/// `.wgsl` files saved with no module changed: every stage naming one is re-read and replaced.
pub fn files_changed(world: &mut World, paths: &[PathBuf]) {
    let generation = world.resource::<bevy_ts::generation::Generations>().live;
    let mut kinds = std::mem::take(&mut world.resource_mut::<MaterialKinds>().live);
    for k in kinds.values_mut() {
        let bindings = k.kind.bindings.clone();
        if paths.contains(&k.fragment.file) {
            refresh_stage(world, generation, &k.decl, &bindings, &mut k.fragment);
        }
        if let Some(v) = &mut k.vertex
            && paths.contains(&v.file)
        {
            refresh_stage(world, generation, &k.decl, &bindings, v);
        }
    }
    world.resource_mut::<MaterialKinds>().live = kinds;
}

impl ScriptMaterial {
    fn with_textures(mut self, textures: Vec<Option<Handle<Image>>>) -> Self {
        self.textures = textures;
        self
    }
}

/// An instance's material from its props: defaults, then each param the kind still has. Returns
/// the params it no longer has.
fn encode(k: &KindState, props: &Map<String, Value>) -> (ScriptMaterial, Vec<String>) {
    let params = &k.kind.bindings.params;
    let mut data = params.defaults();
    let mut dropped = Vec::new();
    for (p, v) in props {
        if params.field(p).is_some() {
            // A value that no longer fits the param's new type keeps the default.
            if params.encode(p, v, &mut data).is_err() {
                dropped.push(p.clone());
            }
        } else if !k.kind.bindings.textures.contains(p) {
            dropped.push(p.clone());
        }
    }
    let textures = vec![None; k.kind.bindings.textures.len()];
    (ScriptMaterial { kind: k.kind.clone(), data, textures }, dropped)
}

fn world_textures(world: &World, k: &KindState, props: &Map<String, Value>) -> Vec<Option<Handle<Image>>> {
    let server = world.get_resource::<AssetServer>();
    k.kind
        .bindings
        .textures
        .iter()
        .map(|t| match (props.get(t).and_then(Value::as_str), server) {
            (Some(path), Some(s)) => Some(s.load(path.to_string())),
            _ => None,
        })
        .collect()
}

/// Checks `<material kind={K} .../>`'s props against the kind the generation declares.
pub fn check_props(world: &World, generation: Option<u64>, kind: &str, props: &Map<String, Value>) -> Result<(), String> {
    let kinds = world.get_resource::<MaterialKinds>().ok_or("script materials need the material plugin")?;
    let decl = kinds.decl(generation, kind).ok_or_else(|| format!("no material {kind}; declare it with material({kind:?}, ...) from bevy-ts/material"))?;
    let bindings = decl.bindings()?;
    let mut scratch = bindings.params.defaults();
    for (p, v) in props {
        if bindings.params.field(p).is_some() {
            bindings.params.encode(p, v, &mut scratch).map_err(|e| format!("{kind}: {e}"))?;
        } else if bindings.textures.contains(p) {
            if !v.is_string() {
                return Err(format!("{kind}: texture {p} takes an asset path"));
            }
        } else {
            let mut names: Vec<&str> = bindings.params.fields.iter().map(|f| f.name.as_str()).collect();
            names.extend(bindings.textures.iter().map(String::as_str));
            return Err(format!("{kind} has no param or texture {p}; it has {}", names.join(", ")));
        }
    }
    Ok(())
}

/// The instance for `kind` with `props`: equal props share one asset, as `<material>` does.
pub fn instance(world: &mut World, kind: &str, props: &Map<String, Value>) -> Option<Handle<ScriptMaterial>> {
    let key = serde_json::to_string(props).unwrap_or_default();
    let kinds = world.resource::<MaterialKinds>();
    let k = kinds.live.get(kind)?;
    if let Some((h, _)) = k.instances.get(&key) {
        return Some(h.clone());
    }
    let (material, _) = encode(k, props);
    let material = material.with_textures(world_textures(world, k, props));
    let h = world.resource_mut::<Assets<ScriptMaterial>>().add(material);
    world.resource_mut::<MaterialKinds>().live.get_mut(kind)?.instances.insert(key, (h.clone(), props.clone()));
    Some(h)
}
