//! Material kinds as the live generation declares them, and their instances. The `materials`
//! section is checked with the generation (a bad schema fails it), then applied: a new or changed
//! kind gets its shaders rebuilt in place and its instances re-encoded by param name. A saved
//! `.wgsl` file replaces its shaders in place with no generation at all.

use crate::layout::{Bindings, ParamType};
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
    /// Bevy's `Material::enable_shadows`; unset takes `Kind`'s default.
    #[serde(default)]
    pub shadows: Option<bool>,
    /// Bevy's `Material::enable_prepass`; unset takes `Kind`'s default.
    #[serde(default)]
    pub prepass: Option<bool>,
    #[serde(default)]
    pub location: Option<SourceLocation>,
}

impl MaterialDecl {
    pub fn bindings(&self) -> Result<Bindings, String> {
        Bindings::new(&self.name, self.params.iter().map(|p| (p.name.as_str(), &p.ty)), &self.textures)
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

    /// What fails the load at the `material()` call: a schema that does not lay out, an unknown
    /// alpha mode, a shader file that is not there.
    pub fn problems(&self) -> Vec<String> {
        let mut out = Vec::new();
        if let Err(e) = self.bindings().and_then(|_| self.alpha_mode().map(|_| ())) {
            out.push(e);
        }
        for f in std::iter::once(&self.fragment).chain(self.vertex.as_ref()) {
            if !f.is_file() {
                out.push(format!("no shader file {}", bevy_ts::backend::display_path(f)));
            }
        }
        out
    }

    /// The material's location, file shown relative to the working directory.
    pub fn at(&self) -> Option<SourceLocation> {
        self.location.clone().map(|mut l| {
            l.file = bevy_ts::backend::display_file(&l.file);
            l
        })
    }
}

#[derive(Deserialize)]
struct Section {
    items: Vec<MaterialDecl>,
}

/// The kinds a generation declares, read from its `materials` section, each with its bindings, or
/// `None` for a declaration the section's own check rejects at its `material()` call. `None`
/// whole for a malformed section, which that check reports too.
pub fn section_kinds(sections: &[bevy_ts::manifest::Section]) -> Option<BTreeMap<String, Option<Bindings>>> {
    let Some(s) = sections.iter().find(|s| s.name == SECTION) else { return Some(BTreeMap::new()) };
    let section: Section = serde_json::from_slice(&s.bytes).ok()?;
    Some(section.items.into_iter().map(|d| (d.name.clone(), d.problems().is_empty().then(|| d.bindings().ok()).flatten())).collect())
}

/// One shader stage of a kind: its file, the shader asset every instance draws with (replaced in
/// place on change), and the last source that compiled.
#[derive(Debug)]
pub struct Stage {
    /// `fragment` or `vertex`.
    pub label: &'static str,
    pub file: PathBuf,
    pub handle: Handle<Shader>,
    /// The combined source the asset holds now; `None` until one compiles.
    pub source: Option<String>,
    /// The source version whose renderer failure was last reported, so each is told once.
    pub reported: Option<u64>,
}

/// One live kind.
pub struct KindState {
    pub decl: MaterialDecl,
    /// A newer declaration whose shaders did not check out: the kind keeps drawing as `decl`, and
    /// a later `.wgsl` save retries this one.
    pub wanted: Option<MaterialDecl>,
    pub kind: Arc<Kind>,
    pub fragment: Stage,
    pub vertex: Option<Stage>,
    /// Instances by key, each with the props it was made from.
    pub instances: BTreeMap<String, (Handle<ScriptMaterial>, Map<String, Value>)>,
}

/// Every live kind, and the kinds the generation being applied declares.
#[derive(Resource, Default)]
pub struct MaterialKinds {
    pub live: BTreeMap<String, KindState>,
    /// Set by the section's check for its apply step.
    pub pending: Option<(u64, BTreeMap<String, MaterialDecl>)>,
    /// Shader assets made, for tests.
    pub shaders_built: usize,
}

/// The section's check: every declaration parses and lays out. WGSL problems do not fail a
/// generation; they are reported when the shader is built, and the old shader keeps drawing.
pub fn check(world: &mut World, generation: u64, bytes: &[u8], location: Option<SourceLocation>) -> Result<(), Vec<ScriptError>> {
    let section: Section = serde_json::from_slice(bytes).map_err(|e| vec![ScriptError::at(format!("materials section: {e}"), location.clone())])?;
    let mut errors = Vec::new();
    let mut decls = BTreeMap::new();
    for d in section.items {
        let at = d.at();
        errors.extend(d.problems().into_iter().map(|m| ScriptError::at(format!("material {}: {m}", d.name), at.clone())));
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
                k.wanted = None;
                let files: Vec<PathBuf> = std::iter::once(&k.fragment).chain(k.vertex.as_ref()).map(|s| s.file.clone()).collect();
                let errors = refresh_touched(world, &mut k, &files);
                log_errors(world, generation, errors, "");
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

fn new_stage(world: &mut World, label: &'static str, file: &Path) -> Stage {
    // An empty placeholder until a source compiles: materials of the kind draw nothing.
    let handle = world.resource_mut::<Assets<Shader>>().reserve_handle();
    Stage { label, file: file.to_path_buf(), handle, source: None, reported: None }
}

/// A kind built from `decl`, reusing the shader assets of `old` (replaced in place) and
/// re-encoding its instances by param name. Every stage the new declaration has is checked before
/// anything is replaced: a kind that draws keeps drawing as it was until they all check out, and
/// keeps `decl` as its wanted declaration.
fn rebuild(world: &mut World, generation: u64, decl: MaterialDecl, old: Option<KindState>) -> KindState {
    let bindings = decl.bindings().expect("checked");
    let alpha = decl.alpha_mode().expect("checked");
    // A stage the declaration adds has no pipeline yet; it joins the fragment's, so it is checked
    // under the fragment's defs.
    let fragment_defs = old.as_ref().and_then(|o| live_defs(world, &o.fragment));
    let mut sources: BTreeMap<&'static str, Option<String>> = BTreeMap::new();
    let mut errors = Vec::new();
    let stages = [
        ("fragment", Some(&decl.fragment), old.as_ref().map(|o| &o.fragment)),
        ("vertex", decl.vertex.as_ref(), old.as_ref().and_then(|o| o.vertex.as_ref())),
    ];
    for (label, file, stage) in stages {
        let Some(file) = file else { continue };
        let current = stage.and_then(|s| s.source.as_deref());
        // A stage with no pipeline of its own yet (added, or never drawn) uses the fragment's.
        let defs = match stage {
            Some(s) if s.source.is_some() => live_defs(world, s).or_else(|| fragment_defs.clone()),
            _ => fragment_defs.clone(),
        };
        match next_source(world, &decl, &bindings, current, defs, file) {
            Ok(source) => {
                sources.insert(label, source);
            }
            Err(e) => errors.extend(e),
        }
    }
    if let Some(mut old) = old {
        if !errors.is_empty() {
            let suffix = if draws(world, &old.fragment) { "; the previous material keeps drawing" } else { "" };
            log_errors(world, generation, errors, suffix);
            old.wanted = Some(decl);
            return old;
        }
        return build(world, generation, decl, bindings, alpha, sources, Some(old));
    }
    log_errors(world, generation, errors, "");
    build(world, generation, decl, bindings, alpha, sources, None)
}

/// The kind from a checked declaration: stages installed from `sources`, instances re-encoded.
fn build(
    world: &mut World,
    generation: u64,
    decl: MaterialDecl,
    bindings: Bindings,
    alpha: AlphaMode,
    mut sources: BTreeMap<&'static str, Option<String>>,
    old: Option<KindState>,
) -> KindState {
    let (mut fragment, vertex, instances) = match old {
        Some(o) => (o.fragment, o.vertex, o.instances),
        None => (new_stage(world, "fragment", &decl.fragment), None, BTreeMap::new()),
    };
    fragment.file = decl.fragment.clone();
    let mut vertex = match (&decl.vertex, vertex) {
        (Some(f), Some(mut v)) => {
            v.file = f.clone();
            Some(v)
        }
        (Some(f), None) => Some(new_stage(world, "vertex", f)),
        (None, _) => None,
    };
    for stage in std::iter::once(&mut fragment).chain(vertex.as_mut()) {
        if let Some(Some(source)) = sources.remove(stage.label) {
            install(world, &decl.name, stage, source);
        }
    }
    let mut kind = Kind::new(&decl.name, bindings, Some(fragment.handle.clone()), vertex.as_ref().map(|v| v.handle.clone()), alpha);
    kind.shadows = decl.shadows.unwrap_or(kind.shadows);
    kind.prepass = decl.prepass.unwrap_or(kind.prepass);
    let kind = Arc::new(kind);
    let mut state = KindState { decl, wanted: None, kind, fragment, vertex, instances };
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

/// Whether a pipeline built from the stage's shader draws now: not merely installed, built.
fn draws(world: &World, stage: &Stage) -> bool {
    world.resource::<wgsl::PipelineWatch>().0.lock().unwrap().drawing.contains(&stage.handle.id())
}

/// The defs a stage's live pipeline was built with. Cloned out, so the render world's per-frame
/// watch never waits on a check.
fn live_defs(world: &World, stage: &Stage) -> Option<Vec<bevy::shader::ShaderDefVal>> {
    world.resource::<wgsl::PipelineWatch>().0.lock().unwrap().defs.get(&stage.handle.id()).cloned()
}

/// The combined source `file` gives under `bindings`, checked under `defs`: `None` when it is
/// `current`. With no defs (nothing draws yet, so nothing to keep) it is not checked here; the
/// renderer's errors are mapped instead.
fn next_source(
    world: &World,
    decl: &MaterialDecl,
    bindings: &Bindings,
    current: Option<&str>,
    defs: Option<Vec<bevy::shader::ShaderDefVal>>,
    file: &Path,
) -> Result<Option<String>, Vec<ScriptError>> {
    let text = std::fs::read_to_string(file)
        .map_err(|e| vec![ScriptError::at(format!("material {}: cannot read {}: {e}", decl.name, bevy_ts::backend::display_path(file)), decl.at())])?;
    let combined = wgsl::combine(&text, bindings, &decl.name);
    if current == Some(combined.as_str()) {
        return Ok(None);
    }
    if let Some(defs) = defs {
        match wgsl::check(world.resource::<Assets<Shader>>(), &combined, file, decl, &defs) {
            wgsl::Checked::Errors(errors) => return Err(errors),
            wgsl::Checked::Unverified(why) => debug!("material {}: not checked before replacing its shader: {why}", decl.name),
            wgsl::Checked::Ok => {}
        }
    }
    Ok(Some(combined))
}

/// Replaces a stage's shader asset in place with a checked source.
fn install(world: &mut World, kind: &str, stage: &mut Stage, source: String) {
    world.resource::<wgsl::PipelineWatch>().0.lock().unwrap().replaced(stage.handle.id());
    let path = format!("bevy_ts/materials/{}.wesl", shader_path(kind, stage.label));
    world.resource_mut::<Assets<Shader>>().insert(stage.handle.id(), Shader::from_wesl(source.clone(), path)).expect("a reserved shader handle");
    world.resource_mut::<MaterialKinds>().shaders_built += 1;
    stage.source = Some(source);
}

/// Re-reads a stage's file and replaces its shader if the source changed and checks out. Returns
/// the errors that kept the old one.
fn refresh_stage(world: &mut World, decl: &MaterialDecl, bindings: &Bindings, stage: &mut Stage) -> Vec<ScriptError> {
    let defs = if stage.source.is_some() { live_defs(world, stage) } else { None };
    match next_source(world, decl, bindings, stage.source.as_deref(), defs, &stage.file) {
        Ok(Some(source)) => {
            install(world, &decl.name, stage, source);
            Vec::new()
        }
        Ok(None) => Vec::new(),
        // Only a stage whose shader built has a previous shader to keep.
        Err(errors) if draws(world, stage) => with_suffix(errors, "; the previous shader keeps drawing"),
        Err(errors) => errors,
    }
}

fn with_suffix(errors: Vec<ScriptError>, suffix: &str) -> Vec<ScriptError> {
    errors.into_iter().map(|e| ScriptError::at(format!("{}{suffix}", e.message), e.location)).collect()
}

fn log_errors(world: &mut World, generation: u64, errors: Vec<ScriptError>, suffix: &str) {
    let mut log = world.resource_mut::<ScriptLog>();
    for e in errors {
        log.push(LogLevel::Error, format!("{}{suffix}", e.message), e.location, generation);
    }
}

/// A shader asset's path, which Bevy keys shader modules by: the kind, encoded exactly (`[a-z0-9]`
/// kept, every other byte `_xx` in hex), then the stage.
fn shader_path(kind: &str, stage: &str) -> String {
    let mut k = String::from("k");
    for b in kind.bytes() {
        if b.is_ascii_lowercase() || b.is_ascii_digit() {
            k.push(b as char);
        } else {
            k.push_str(&format!("_{b:02x}"));
        }
    }
    format!("{k}/{stage}")
}

/// Main world: pipelines the renderer failed to build for a stage. A failure from a source older
/// than the stage's current one is dropped (pipelined rendering can still show it after a fix).
/// Otherwise it is reported against the author's file by re-checking the current source under the
/// failing pipeline's defs, or, when the check cannot reproduce it (a device capability, a WESL
/// feature the check skips), with the renderer's own message. Each version is reported once.
pub fn report_pipeline_errors(world: &mut World) {
    let Some(watch) = world.get_resource::<wgsl::PipelineWatch>().cloned() else { return };
    let (failed, versions) = {
        let mut w = watch.0.lock().unwrap();
        (std::mem::take(&mut w.failed), w.version.clone())
    };
    if failed.is_empty() {
        return;
    }
    let generation = world.resource::<bevy_ts::generation::Generations>().live;
    let mut kinds = std::mem::take(&mut world.resource_mut::<MaterialKinds>().live);
    let mut out = Vec::new();
    for f in failed {
        if f.version < versions.get(&f.shader).copied().unwrap_or_default() {
            continue;
        }
        let Some((decl, stage)) = kinds.values_mut().find_map(|k| {
            let decl = &k.decl;
            std::iter::once(&mut k.fragment).chain(k.vertex.as_mut()).find(|s| s.handle.id() == f.shader).map(|s| (decl, s))
        }) else {
            continue;
        };
        let Some(source) = stage.source.clone() else { continue };
        if stage.reported == Some(f.version) {
            continue;
        }
        stage.reported = Some(f.version);
        match wgsl::check(world.resource::<Assets<Shader>>(), &source, &stage.file, decl, &f.defs) {
            wgsl::Checked::Errors(errors) => out.extend(errors),
            _ => out.push(ScriptError::at(
                format!("material {}: the renderer could not build {}: {}", decl.name, bevy_ts::backend::display_path(&stage.file), f.message),
                decl.at(),
            )),
        }
    }
    world.resource_mut::<MaterialKinds>().live = kinds;
    log_errors(world, generation, out, "");
}

/// `.wgsl` files saved with no module changed: every stage naming one is re-read and replaced. A
/// kind waiting on a newer declaration retries it whole; if that still fails, the save still
/// applies to the kind as it draws, when it checks out under the live schema.
pub fn files_changed(world: &mut World, paths: &[PathBuf]) {
    let generation = world.resource::<bevy_ts::generation::Generations>().live;
    let mut kinds = std::mem::take(&mut world.resource_mut::<MaterialKinds>().live);
    let names: Vec<String> = kinds.keys().cloned().collect();
    for name in names {
        let mut k = kinds.remove(&name).expect("listed");
        let touched = |f: &PathBuf| paths.contains(f);
        if let Some(wanted) = k.wanted.clone()
            && [Some(&wanted.fragment), wanted.vertex.as_ref(), Some(&k.fragment.file), k.vertex.as_ref().map(|v| &v.file)].into_iter().flatten().any(touched)
        {
            k = rebuild(world, generation, wanted, Some(k));
            if k.wanted.is_some() {
                // The retry's errors are logged; this fallback stays quiet.
                refresh_touched(world, &mut k, paths);
            }
        } else {
            let errors = refresh_touched(world, &mut k, paths);
            log_errors(world, generation, errors, "");
        }
        kinds.insert(name, k);
    }
    world.resource_mut::<MaterialKinds>().live = kinds;
}

/// Refreshes the live stages whose file is in `paths`, under the live declaration.
fn refresh_touched(world: &mut World, k: &mut KindState, paths: &[PathBuf]) -> Vec<ScriptError> {
    let bindings = k.kind.bindings.clone();
    let mut errors = Vec::new();
    for stage in std::iter::once(&mut k.fragment).chain(k.vertex.as_mut()) {
        if paths.contains(&stage.file) {
            errors.extend(refresh_stage(world, &k.decl, &bindings, stage));
        }
    }
    errors
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
    let params = &k.kind.bindings.params();
    let mut data = params.defaults();
    let mut dropped = Vec::new();
    for (p, v) in props {
        if params.field(p).is_some() {
            // A value that no longer fits the param's new type keeps the default.
            if params.encode(p, v, &mut data).is_err() {
                dropped.push(p.clone());
            }
        } else if !k.kind.bindings.textures().contains(p) {
            dropped.push(p.clone());
        }
    }
    let textures = vec![None; k.kind.bindings.textures().len()];
    (ScriptMaterial { kind: k.kind.clone(), data, textures }, dropped)
}

fn world_textures(world: &World, k: &KindState, props: &Map<String, Value>) -> Vec<Option<Handle<Image>>> {
    let server = world.get_resource::<AssetServer>();
    k.kind
        .bindings
        .textures()
        .iter()
        .map(|t| match (props.get(t).and_then(Value::as_str), server) {
            (Some(path), Some(s)) => Some(s.load(path.to_string())),
            _ => None,
        })
        .collect()
}

/// Checks `<material kind={K} .../>`'s props against the kinds a generation declares
/// ([`section_kinds`]). A kind whose declaration is invalid is not checked here.
pub fn check_props(kinds: &BTreeMap<String, Option<Bindings>>, kind: &str, props: &Map<String, Value>) -> Result<(), String> {
    let Some(bindings) = kinds.get(kind).ok_or_else(|| format!("no material {kind}; declare it with material({kind:?}, ...) from bevy-ts/material"))? else {
        return Ok(());
    };
    let mut scratch = bindings.params().defaults();
    for (p, v) in props {
        if bindings.params().field(p).is_some() {
            bindings.params().encode(p, v, &mut scratch).map_err(|e| format!("{kind}: {e}"))?;
        } else if bindings.textures().contains(p) {
            if !v.is_string() {
                return Err(format!("{kind}: texture {p} takes an asset path"));
            }
        } else {
            let mut names: Vec<&str> = bindings.params().fields.iter().map(|f| f.name.as_str()).collect();
            names.extend(bindings.textures().iter().map(String::as_str));
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shader_paths_are_exact_per_kind_and_stage() {
        assert_eq!(shader_path("game::Glow", "fragment"), "kgame_3a_3a_47low/fragment");
        assert_ne!(shader_path("game::Glow", "fragment"), shader_path("Game::glow", "fragment"));
        assert_ne!(shader_path("game::Glow", "fragment"), shader_path("game::Glow", "vertex"));
        assert_ne!(shader_path("a_b", "fragment"), shader_path("a::b", "fragment"));
    }
}
