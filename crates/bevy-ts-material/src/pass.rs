//! Script passes (openspec change `script-fullscreen-pass`): a fullscreen fragment over a
//! `Camera2d`'s image, its WGSL in a `.wgsl` file and its params declared in TS (`pass()` in
//! `bevy-ts/material`). Bevy's `FullscreenMaterial` draws it, on four slot types: a live kind takes
//! a slot by its `order`, then its name, its WGSL is written into that slot's shader, and a
//! camera's `<pass>` puts the kind's bytes in that slot's component on the camera. Slot `n` runs
//! after slot `n - 1`, so slot order is run order. The `passes` section is checked and applied as
//! `kinds.rs` does `materials`; a saved `.wgsl` replaces a slot's shader in place.

use crate::kinds::ParamDecl;
use crate::layout::{PASS_BYTES, PassLayout};
use crate::wgsl;
use bevy::asset::uuid_handle;
use bevy::camera::CameraUpdateSystems;
use bevy::core_pipeline::fullscreen_material::{FullscreenMaterial, FullscreenMaterialPlugin, fullscreen_material_system};
use bevy::core_pipeline::tonemapping::tonemapping;
use bevy::core_pipeline::{Core2d, Core2dSystems};
use bevy::ecs::schedule::{ScheduleConfigs, ScheduleLabel};
use bevy::ecs::system::BoxedSystem;
use bevy::prelude::*;
use bevy::render::RenderApp;
use bevy::render::extract_component::ExtractComponent;
use bevy::render::render_resource::ShaderType;
use bevy::shader::ShaderRef;
use bevy_ts::backend::{ScriptError, SourceLocation};
use bevy_ts::log::{LogLevel, ScriptLog};
use serde::Deserialize;
use serde_json::{Map, Value};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

/// The manifest section `pass()` writes.
pub const SECTION: &str = "passes";

/// Pass kinds live at once: one per slot type. Raising it is this constant, a slot type in
/// [`slots!`] and a shader handle in [`SLOT_SHADERS`].
pub const SLOTS: usize = 4;

/// A pass on a camera, in slot `SLOT`: its params' bytes, frame fields first ([`PASS_BYTES`]).
/// The WGSL struct of the kind in the slot is this size or smaller. Not reflected: levels never
/// save it, a pass is TSX.
#[derive(Component, ExtractComponent, ShaderType, Clone, Copy, Default, Debug, PartialEq)]
#[extract_app(RenderApp)]
pub struct ScriptPass<const SLOT: u8> {
    pub data: [UVec4; 16],
}

impl<const SLOT: u8> ScriptPass<SLOT> {
    pub fn from_bytes(bytes: &[u8]) -> Self {
        let mut data = [UVec4::ZERO; 16];
        for (i, w) in bytes.chunks(4).take(PASS_BYTES / 4).enumerate() {
            let mut b = [0u8; 4];
            b[..w.len()].copy_from_slice(w);
            data[i / 4][i % 4] = u32::from_le_bytes(b);
        }
        Self { data }
    }

    pub fn bytes(&self) -> Vec<u8> {
        self.data.iter().flat_map(|v| v.to_array()).flat_map(u32::to_le_bytes).collect()
    }
}

/// The shader each slot draws: a fixed handle the kind in the slot is written into.
pub const SLOT_SHADERS: [Handle<Shader>; SLOTS] = [
    uuid_handle!("6f1d3c0e-2b8a-4c55-9d0e-1a7b3f5c9e01"),
    uuid_handle!("6f1d3c0e-2b8a-4c55-9d0e-1a7b3f5c9e02"),
    uuid_handle!("6f1d3c0e-2b8a-4c55-9d0e-1a7b3f5c9e03"),
    uuid_handle!("6f1d3c0e-2b8a-4c55-9d0e-1a7b3f5c9e04"),
];

impl<const SLOT: u8> FullscreenMaterial for ScriptPass<SLOT> {
    fn fragment_shader() -> ShaderRef {
        SLOT_SHADERS[SLOT as usize].clone().into()
    }

    fn schedule() -> impl ScheduleLabel + Clone {
        Core2d
    }

    /// After tonemapping, so a 2D camera grades the colours it shows, and after the slot before.
    fn schedule_configs(system: ScheduleConfigs<BoxedSystem>) -> ScheduleConfigs<BoxedSystem> {
        let system = system.in_set(Core2dSystems::PostProcess).after(tonemapping);
        match SLOT {
            0 => system,
            1 => system.after(fullscreen_material_system::<ScriptPass<0>>),
            2 => system.after(fullscreen_material_system::<ScriptPass<1>>),
            _ => system.after(fullscreen_material_system::<ScriptPass<2>>),
        }
    }
}

/// Runs `$body` with `$t` the slot type of `$slot`.
macro_rules! slots {
    ($slot:expr, $t:ident => $body:expr) => {
        match $slot {
            0 => {
                type $t = ScriptPass<0>;
                $body
            }
            1 => {
                type $t = ScriptPass<1>;
                $body
            }
            2 => {
                type $t = ScriptPass<2>;
                $body
            }
            _ => {
                type $t = ScriptPass<3>;
                $body
            }
        }
    };
}

/// One `pass(name, {...})`.
#[derive(Clone, Debug, PartialEq, Deserialize)]
pub struct PassDecl {
    pub name: String,
    pub params: Vec<ParamDecl>,
    pub fragment: PathBuf,
    #[serde(default)]
    pub order: f64,
    #[serde(default)]
    pub location: Option<SourceLocation>,
}

impl PassDecl {
    pub fn layout(&self) -> Result<PassLayout, String> {
        PassLayout::new(&self.name, self.params.iter().map(|p| (p.name.as_str(), &p.ty)))
    }

    /// What fails the load at the `pass()` call: a schema that does not lay out, a missing file.
    pub fn problems(&self) -> Vec<String> {
        let mut out = Vec::new();
        if let Err(e) = self.layout() {
            out.push(e);
        }
        if !self.fragment.is_file() {
            out.push(format!("no shader file {}", bevy_ts::backend::display_path(&self.fragment)));
        }
        out
    }

    /// The pass's location, file shown relative to the working directory.
    pub fn at(&self) -> Option<SourceLocation> {
        self.location.clone().map(|mut l| {
            l.file = bevy_ts::backend::display_file(&l.file);
            l
        })
    }

    fn subject(&self, layout: &PassLayout) -> wgsl::Subject {
        wgsl::Subject { label: format!("pass {}", self.name), at: self.at(), generated: layout.generated_names() }
    }
}

#[derive(Deserialize)]
struct Section {
    items: Vec<PassDecl>,
}

/// The kinds a generation declares, read from its `passes` section, each with its layout, or
/// `None` for a declaration the section's own check rejects. `None` whole for a malformed
/// section, which that check reports too.
pub fn section_kinds(sections: &[bevy_ts::manifest::Section]) -> Option<BTreeMap<String, Option<PassLayout>>> {
    let Some(s) = sections.iter().find(|s| s.name == SECTION) else { return Some(BTreeMap::new()) };
    let section: Section = serde_json::from_slice(&s.bytes).ok()?;
    Some(section.items.into_iter().map(|d| (d.name.clone(), d.problems().is_empty().then(|| d.layout().ok()).flatten())).collect())
}

/// One live kind.
pub struct PassKind {
    pub decl: PassDecl,
    pub layout: PassLayout,
    /// A newer declaration whose shader did not check out: the kind keeps drawing as `decl`, and a
    /// later `.wgsl` save retries this one.
    pub wanted: Option<PassDecl>,
    pub slot: u8,
    /// The combined source last installed; `None` until the file is read.
    pub source: Option<String>,
    /// The source version whose renderer failure was last reported, so each is told once.
    pub reported: Option<u64>,
}

impl PassKind {
    pub fn handle(&self) -> &Handle<Shader> {
        &SLOT_SHADERS[self.slot as usize]
    }
}

/// Every live pass kind, and the kinds the generation being applied declares.
#[derive(Resource, Default)]
pub struct PassKinds {
    pub live: BTreeMap<String, PassKind>,
    pending: Option<(u64, BTreeMap<String, PassDecl>)>,
    /// Shader sources installed, for tests.
    pub shaders_built: usize,
}

impl PassKinds {
    /// The live kind in `slot`.
    pub fn in_slot(&self, slot: u8) -> Option<&PassKind> {
        self.live.values().find(|k| k.slot == slot)
    }
}

/// The `<pass>` elements on a camera: each kind's props as the element gives them, and where.
/// Its slot components follow from it and the live kinds ([`sync_camera`]).
#[derive(Component, Clone, Debug, Default)]
pub struct CameraPasses {
    pub props: BTreeMap<String, (Map<String, Value>, Option<SourceLocation>)>,
}

/// The section's check: every declaration parses and lays out, its file is there, and at most
/// [`SLOTS`] kinds. WGSL problems do not fail a generation; they are reported when the shader is
/// built.
pub fn check(world: &mut World, generation: u64, bytes: &[u8], location: Option<SourceLocation>) -> Result<(), Vec<ScriptError>> {
    let section: Section = serde_json::from_slice(bytes).map_err(|e| vec![ScriptError::at(format!("passes section: {e}"), location.clone())])?;
    let mut errors = Vec::new();
    let mut decls = BTreeMap::new();
    for (i, d) in section.items.into_iter().enumerate() {
        let at = d.at();
        if i == SLOTS {
            errors.push(ScriptError::at(format!("pass {}: at most {SLOTS} pass kinds may be live at once", d.name), at.clone()));
        }
        errors.extend(d.problems().into_iter().map(|m| ScriptError::at(format!("pass {}: {m}", d.name), at.clone())));
        decls.insert(d.name.clone(), d);
    }
    if !errors.is_empty() {
        return Err(errors);
    }
    world.resource_mut::<PassKinds>().pending = Some((generation, decls));
    Ok(())
}

/// Slots for `decls`: by `order`, then by name.
fn slots_of<'a>(decls: impl IntoIterator<Item = &'a PassDecl>) -> BTreeMap<String, u8> {
    let mut v: Vec<&PassDecl> = decls.into_iter().collect();
    v.sort_by(|a, b| a.order.total_cmp(&b.order).then_with(|| a.name.cmp(&b.name)));
    v.into_iter().enumerate().map(|(i, d)| (d.name.clone(), i as u8)).collect()
}

/// The section's apply: kinds made, rebuilt, moved or dropped to match the generation, every
/// moved kind's shader written into its new slot, and every camera's slot components following
/// their kinds, all in this one world update.
pub fn apply(world: &mut World, generation: u64, present: bool) {
    let decls = match world.resource_mut::<PassKinds>().pending.take() {
        Some((g, d)) if g == generation && present => d,
        _ if present => return,
        _ => BTreeMap::new(),
    };
    let slots = slots_of(decls.values());
    let mut kinds = std::mem::take(&mut world.resource_mut::<PassKinds>().live);
    kinds.retain(|name, _| decls.contains_key(name));
    let mut out = BTreeMap::new();
    for (name, decl) in decls {
        let slot = slots[&name];
        let k = match kinds.remove(&name) {
            Some(mut k) if k.decl == decl => {
                k.wanted = None;
                let moved = k.slot != slot;
                k.slot = slot;
                // A failure already reported was the old slot's shader's.
                if moved {
                    k.reported = None;
                }
                if moved && let Some(source) = k.source.clone() {
                    install(world, &mut k, source);
                }
                // The file may have changed since the last generation.
                let errors = refresh(world, &mut k, !moved);
                log_errors(world, generation, errors);
                k
            }
            Some(k) => rebuild(world, generation, decl, Some(k), slot),
            None => rebuild(world, generation, decl, None, slot),
        };
        out.insert(name, k);
    }
    world.resource_mut::<PassKinds>().live = out;
    sync_cameras(world);
}

/// A kind built from `decl` in `slot`, from `old` (in its old slot) when it was live: its new
/// source checked first when the old one draws, so a kind that draws keeps drawing as it was until
/// the new one checks out, keeping `decl` as its wanted declaration.
fn rebuild(world: &mut World, generation: u64, decl: PassDecl, old: Option<PassKind>, slot: u8) -> PassKind {
    let layout = decl.layout().expect("checked");
    let moved = old.as_ref().is_some_and(|o| o.slot != slot);
    let current = old.as_ref().and_then(|o| o.source.clone());
    match next_source(world, &decl, &layout, slot, current.as_deref(), old.is_some() && !moved) {
        Ok(source) => {
            let mut k = PassKind { decl, layout, wanted: None, slot, source: current, reported: old.and_then(|o| o.reported).filter(|_| !moved) };
            // The same source, in a new slot, still needs writing there.
            if let Some(s) = source.or_else(|| k.source.clone().filter(|_| moved)) {
                install(world, &mut k, s);
            }
            k
        }
        Err(errors) => match old {
            Some(mut old) => {
                let suffix = if draws(world, old.handle()) { "; the previous pass keeps drawing" } else { "" };
                log_errors(world, generation, with_suffix(errors, suffix));
                old.slot = slot;
                if moved && let Some(s) = old.source.clone() {
                    install(world, &mut old, s);
                }
                old.wanted = Some(decl);
                old
            }
            None => {
                log_errors(world, generation, errors);
                PassKind { decl, layout, wanted: None, slot, source: None, reported: None }
            }
        },
    }
}

/// The combined source `decl`'s file gives under `layout`: `None` when it is `current`. Checked
/// first only when `keep` and the slot's pipeline draws, so there is a shader to keep and Bevy's
/// modules are loaded; otherwise the renderer's errors are mapped instead.
fn next_source(world: &World, decl: &PassDecl, layout: &PassLayout, slot: u8, current: Option<&str>, keep: bool) -> Result<Option<String>, Vec<ScriptError>> {
    let file = &decl.fragment;
    let text = std::fs::read_to_string(file)
        .map_err(|e| vec![ScriptError::at(format!("pass {}: cannot read {}: {e}", decl.name, bevy_ts::backend::display_path(file)), decl.at())])?;
    let combined = wgsl::combine_pass(&text, layout, &decl.name);
    if current == Some(combined.as_str()) {
        return Ok(None);
    }
    let handle = &SLOT_SHADERS[slot as usize];
    if keep && let Some(defs) = live_defs(world, handle) {
        match wgsl::check(world.resource::<Assets<Shader>>(), &combined, file, &decl.subject(layout), &defs) {
            wgsl::Checked::Errors(errors) => return Err(errors),
            wgsl::Checked::Unverified(why) => debug!("pass {}: not checked before replacing its shader: {why}", decl.name),
            wgsl::Checked::Ok => {}
        }
    }
    Ok(Some(combined))
}

/// Writes a checked source into the kind's slot shader, in place.
fn install(world: &mut World, k: &mut PassKind, source: String) {
    let id = k.handle().id();
    if let Some(w) = world.get_resource::<wgsl::PipelineWatch>() {
        w.0.lock().unwrap().replaced(id);
    }
    let path = format!("bevy_ts/passes/slot{}.wesl", k.slot);
    world.resource_mut::<Assets<Shader>>().insert(id, Shader::from_wesl(source.clone(), path)).expect("a fixed shader handle");
    world.resource_mut::<PassKinds>().shaders_built += 1;
    k.source = Some(source);
}

/// Re-reads the kind's file under its live layout and installs it if it changed and checks out.
/// Returns the errors that kept the old one.
fn refresh(world: &mut World, k: &mut PassKind, keep: bool) -> Vec<ScriptError> {
    match next_source(world, &k.decl, &k.layout, k.slot, k.source.as_deref(), keep && k.source.is_some()) {
        Ok(Some(source)) => {
            install(world, k, source);
            Vec::new()
        }
        Ok(None) => Vec::new(),
        Err(errors) if draws(world, k.handle()) => with_suffix(errors, "; the previous shader keeps drawing"),
        Err(errors) => errors,
    }
}

fn draws(world: &World, handle: &Handle<Shader>) -> bool {
    world.get_resource::<wgsl::PipelineWatch>().is_some_and(|w| w.0.lock().unwrap().drawing.contains(&handle.id()))
}

fn live_defs(world: &World, handle: &Handle<Shader>) -> Option<Vec<bevy::shader::ShaderDefVal>> {
    world.get_resource::<wgsl::PipelineWatch>()?.0.lock().unwrap().defs.get(&handle.id()).cloned()
}

fn with_suffix(errors: Vec<ScriptError>, suffix: &str) -> Vec<ScriptError> {
    errors.into_iter().map(|e| ScriptError::at(format!("{}{suffix}", e.message), e.location)).collect()
}

fn log_errors(world: &mut World, generation: u64, errors: Vec<ScriptError>) {
    let mut log = world.resource_mut::<ScriptLog>();
    for e in errors {
        log.push(LogLevel::Error, e.message, e.location, generation);
    }
}

/// `.wgsl` files saved with no module changed: every kind naming one re-reads it. A kind waiting
/// on a newer declaration retries it whole.
pub fn files_changed(world: &mut World, paths: &[PathBuf]) {
    let generation = bevy_ts::runtime::current_generation(world);
    let mut kinds = std::mem::take(&mut world.resource_mut::<PassKinds>().live);
    let mut resync = false;
    for k in kinds.values_mut() {
        let touched = |f: &Path| paths.iter().any(|p| p == f);
        if let Some(wanted) = k.wanted.clone()
            && (touched(&wanted.fragment) || touched(&k.decl.fragment))
        {
            let old = std::mem::replace(k, placeholder(k));
            let slot = old.slot;
            *k = rebuild(world, generation, wanted, Some(old), slot);
            resync |= k.wanted.is_none();
        } else if touched(&k.decl.fragment) {
            let errors = refresh(world, k, true);
            log_errors(world, generation, errors);
        }
    }
    world.resource_mut::<PassKinds>().live = kinds;
    if resync {
        sync_cameras(world);
    }
}

/// A stand-in while a kind is rebuilt in place.
fn placeholder(k: &PassKind) -> PassKind {
    PassKind { decl: k.decl.clone(), layout: k.layout.clone(), wanted: None, slot: k.slot, source: None, reported: None }
}

/// Renderer failures of pass slot shaders, reported against the author's file by re-checking the
/// current source, or with the renderer's own message. Each version is reported once. Called by
/// the materials' report with the failures that are not theirs.
pub fn report_failures(world: &mut World, failed: Vec<wgsl::Failure>, versions: &std::collections::HashMap<AssetId<Shader>, u64>) {
    if failed.is_empty() || !world.contains_resource::<PassKinds>() {
        return;
    }
    let generation = bevy_ts::runtime::current_generation(world);
    let mut kinds = std::mem::take(&mut world.resource_mut::<PassKinds>().live);
    let mut out = Vec::new();
    for f in failed {
        if f.version < versions.get(&f.shader).copied().unwrap_or_default() {
            continue;
        }
        let Some(k) = kinds.values_mut().find(|k| k.handle().id() == f.shader) else { continue };
        let Some(source) = k.source.clone() else { continue };
        if k.reported == Some(f.version) {
            continue;
        }
        k.reported = Some(f.version);
        match wgsl::check(world.resource::<Assets<Shader>>(), &source, &k.decl.fragment, &k.decl.subject(&k.layout), &f.defs) {
            wgsl::Checked::Errors(errors) => out.extend(errors),
            _ => out.push(ScriptError::at(
                format!("pass {}: the renderer could not build {}: {}", k.decl.name, bevy_ts::backend::display_path(&k.decl.fragment), f.message),
                k.decl.at(),
            )),
        }
    }
    world.resource_mut::<PassKinds>().live = kinds;
    log_errors(world, generation, out);
}

/// `props` encoded by `layout` over its defaults (zeros, colours white), frame fields left zero.
/// Returns the props that did not encode.
pub fn encode(layout: &PassLayout, props: &Map<String, Value>) -> (Vec<u8>, Vec<String>) {
    let mut data = layout.params().defaults();
    data.resize(PASS_BYTES, 0);
    let mut dropped = Vec::new();
    for (p, v) in props {
        if v.is_null() {
            continue;
        }
        if !layout.has(p) || layout.params().encode(p, v, &mut data).is_err() {
            dropped.push(p.clone());
        }
    }
    (data, dropped)
}

/// Checks `<pass kind={K} .../>`'s props, and the names of its `bound` ones, against the kinds a
/// generation declares ([`section_kinds`]). A kind whose declaration is invalid is not checked
/// here; a bound value is checked as it arrives.
pub fn check_props(kinds: &BTreeMap<String, Option<PassLayout>>, kind: &str, props: &Map<String, Value>, bound: &[String]) -> Result<(), String> {
    let Some(layout) = kinds.get(kind).ok_or_else(|| format!("no pass {kind}; declare it with pass({kind:?}, ...) from bevy-ts/material"))? else {
        return Ok(());
    };
    let mut scratch = layout.params().defaults();
    for p in props.keys().chain(bound) {
        if !layout.has(p) {
            let names: Vec<&str> = layout.own().map(|f| f.name.as_str()).collect();
            let has = if names.is_empty() { "no params".to_string() } else { format!("params {}", names.join(", ")) };
            return Err(format!("{kind} has no param {p}; it has {has}"));
        }
    }
    for (p, v) in props {
        layout.params().encode(p, v, &mut scratch).map_err(|e| format!("{kind}: {e}"))?;
    }
    Ok(())
}

/// `<pass kind>` on camera `e`: its props for the kind, then the camera's slots.
pub fn set(world: &mut World, e: Entity, kind: &str, props: Map<String, Value>, at: Option<SourceLocation>) {
    let Ok(mut em) = world.get_entity_mut(e) else { return };
    match em.get_mut::<CameraPasses>() {
        Some(mut passes) => {
            passes.props.insert(kind.to_string(), (props, at));
        }
        None => {
            em.insert(CameraPasses { props: BTreeMap::from([(kind.to_string(), (props, at))]) });
        }
    }
    sync_camera(world, e);
}

/// The `<pass>` of `kind` came off camera `e`: the pass stops the next frame.
pub fn remove(world: &mut World, e: Entity, kind: &str) {
    let Ok(mut em) = world.get_entity_mut(e) else { return };
    let Some(mut passes) = em.take::<CameraPasses>() else { return };
    passes.props.remove(kind);
    if !passes.props.is_empty() {
        em.insert(passes);
    }
    sync_camera(world, e);
}

/// Every camera with a `<pass>`, synced.
fn sync_cameras(world: &mut World) {
    let mut q = world.query_filtered::<Entity, With<CameraPasses>>();
    let cameras: Vec<Entity> = q.iter(world).collect();
    for e in cameras {
        sync_camera(world, e);
    }
    // A camera whose last `<pass>` went keeps no slot component.
    for slot in 0..SLOTS as u8 {
        slots!(slot, T => {
            let mut q = world.query_filtered::<Entity, (With<T>, Without<CameraPasses>)>();
            let stale: Vec<Entity> = q.iter(world).collect();
            for e in stale {
                world.entity_mut(e).remove::<T>();
            }
        });
    }
}

/// Camera `e`'s slot components from its `<pass>` props and the live kinds: each kind's bytes in
/// its slot, frame fields kept, and no component in a slot none of its kinds holds. A prop the
/// kind no longer has, or whose value no longer fits, is dropped with a warning.
pub fn sync_camera(world: &mut World, e: Entity) {
    let passes = world.get::<CameraPasses>(e).cloned().unwrap_or_default();
    let mut want: [Option<Vec<u8>>; SLOTS] = Default::default();
    let mut warnings = Vec::new();
    {
        let kinds = world.resource::<PassKinds>();
        for (kind, (props, at)) in &passes.props {
            let Some(k) = kinds.live.get(kind) else { continue };
            let (bytes, dropped) = encode(&k.layout, props);
            warnings.extend(dropped.into_iter().map(|p| (format!("<pass kind={{{kind}}}>: param {p} is gone or no longer fits; it is dropped"), at.clone())));
            want[k.slot as usize] = Some(bytes);
        }
    }
    let generation = bevy_ts::runtime::current_generation(world);
    if let Some(mut log) = world.get_resource_mut::<ScriptLog>() {
        for (m, at) in warnings {
            log.push(LogLevel::Warn, m, at, generation);
        }
    }
    let Ok(mut em) = world.get_entity_mut(e) else { return };
    for (slot, want) in want.into_iter().enumerate() {
        slots!(slot, T => match want {
            Some(mut bytes) => {
                if let Some(old) = em.get::<T>() {
                    let frame = crate::layout::FRAME_BYTES;
                    bytes[..frame].copy_from_slice(&old.bytes()[..frame]);
                }
                let new = T::from_bytes(&bytes);
                if em.get::<T>() != Some(&new) {
                    em.insert(new);
                }
            }
            None => {
                em.remove::<T>();
            }
        });
    }
}

/// The bytes of one slot's component on `e`, for tests and tools.
pub fn slot_bytes(world: &World, e: Entity, slot: u8) -> Option<Vec<u8>> {
    slots!(slot, T => world.get::<T>(e).map(T::bytes))
}

/// The frame fields of `e`'s passes: `time`, `texel`, `size` and `view`, for tests and tools.
pub fn frame_fields(bytes: &[u8]) -> (f32, f32, Vec2, Vec2) {
    let f = |i: usize| f32::from_le_bytes(bytes[i..i + 4].try_into().expect("4 bytes"));
    (f(0), f(4), Vec2::new(f(8), f(12)), Vec2::new(f(16), f(20)))
}

/// `PostUpdate`, after the pixel camera's snap and Bevy's camera update: the frame fields of
/// every pass on every camera. `time` is `Time`'s wrapped elapsed seconds, as Bevy's
/// `globals.time`; `size` the physical target; `texel` its pixels per world unit, through the
/// camera's scale; `view` the world point drawn at its centre.
fn write_frame_fields(world: &mut World) {
    let time = world.resource::<Time>().elapsed_secs_wrapped();
    let mut q = world.query_filtered::<(Entity, &Camera, &Projection, &GlobalTransform), With<CameraPasses>>();
    let frames: Vec<(Entity, [u8; crate::layout::FRAME_BYTES])> = q
        .iter(world)
        .map(|(e, camera, projection, at)| {
            let size = camera.physical_target_size().unwrap_or_default().as_vec2();
            let (texel, view) = match projection {
                Projection::Orthographic(o) if o.area.width() > 0.0 => {
                    let world_width = o.area.width() * at.scale().x.abs();
                    (if world_width > 0.0 { size.x / world_width } else { 0.0 }, at.transform_point(o.area.center().extend(0.0)).truncate())
                }
                _ => (0.0, at.translation().truncate()),
            };
            let mut b = [0u8; crate::layout::FRAME_BYTES];
            for (i, v) in [time, texel, size.x, size.y, view.x, view.y].into_iter().enumerate() {
                b[4 * i..4 * i + 4].copy_from_slice(&v.to_le_bytes());
            }
            (e, b)
        })
        .collect();
    for (e, frame) in frames {
        let mut em = world.entity_mut(e);
        for slot in 0..SLOTS as u8 {
            slots!(slot, T => {
                if let Some(mut pass) = em.get_mut::<T>() {
                    let words = frame.chunks(4).map(|w| u32::from_le_bytes(w.try_into().expect("4 bytes")));
                    for (i, w) in words.enumerate() {
                        if pass.data[i / 4][i % 4] != w {
                            pass.data[i / 4][i % 4] = w;
                        }
                    }
                }
            });
        }
    }
}

/// Script passes: the four slot types on Bevy's `FullscreenMaterial`, and the frame fields.
pub struct ScriptPassPlugin;

impl Plugin for ScriptPassPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins((
            FullscreenMaterialPlugin::<ScriptPass<0>>::default(),
            FullscreenMaterialPlugin::<ScriptPass<1>>::default(),
            FullscreenMaterialPlugin::<ScriptPass<2>>::default(),
            FullscreenMaterialPlugin::<ScriptPass<3>>::default(),
        ))
        .init_resource::<PassKinds>()
        .add_systems(PostUpdate, write_frame_fields.after(CameraUpdateSystems).after(bevy_ts::BevyTsSet::PostUpdate));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bytes_round_trip_through_the_slot_component() {
        let bytes: Vec<u8> = (0..PASS_BYTES).map(|i| i as u8).collect();
        assert_eq!(ScriptPass::<2>::from_bytes(&bytes).bytes(), bytes);
    }

    #[test]
    fn slots_follow_order_then_name() {
        let d = |name: &str, order: f64| PassDecl { name: name.into(), params: vec![], fragment: "x.wgsl".into(), order, location: None };
        let s = slots_of(&[d("b", 0.0), d("a", 0.0), d("z", -1.0), d("c", 2.0)]);
        assert_eq!(s, BTreeMap::from([("z".into(), 0), ("a".into(), 1), ("b".into(), 2), ("c".into(), 3)]));
    }
}
