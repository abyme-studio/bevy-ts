//! `bevy-ts/diagnostics`, a built-in module every engine ships: Bevy's diagnostics as a TS system
//! reads and writes them. `DiagnosticsStore` reads every registered diagnostic by path,
//! `Diagnostics` adds measurements through Bevy's own deferred buffer, `FrameCount` is Bevy's
//! frame counter, and `app.registerDiagnostic` is Bevy's `RegisterDiagnostic`. `bevy-ts.json`'s
//! `diagnostics` lists the Bevy plugins that measure (none by default).

use bevy::diagnostic::{Diagnostic, DiagnosticPath, DiagnosticsStore, FrameCount};
use bevy::ecs::system::SystemParam;
use bevy::prelude::*;
use bevy_ts::backend::{ScriptError, SourceLocation};
use bevy_ts::plugin_api::{AppliedSection, SectionInput};
use bevy_ts_native::prelude::*;
use serde::Deserialize;
use std::collections::BTreeMap;

pub const MODULE: &str = "bevy-ts/diagnostics";
/// The manifest section `app.registerDiagnostic` writes.
pub const SECTION: &str = "diagnostics";

/// Registers the module, its params and the `diagnostics` section.
pub struct ScriptDiagnosticsPlugin;

impl Plugin for ScriptDiagnosticsPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<DiagnosticsStore>()
            .init_resource::<ScriptOwned>()
            .ts_param::<StoreParam<'static>>()
            .ts_param::<DiagnosticsParam<'static, 'static>>()
            .ts_param::<FrameCountParam<'static>>()
            .add_ts_section(SECTION, |world, input| check(world, input).map(|_| ()))
            .on_ts_section_applied(SECTION, apply)
            .ts_module(MODULE, include_str!("../ts/diagnostics.js"), include_str!("../ts/diagnostics.d.ts"));
    }
}

/// A path as a script gives it, checked as Bevy's `DiagnosticPath::new` would (which panics).
fn path(p: &str) -> Result<DiagnosticPath, String> {
    if p.is_empty() || p.starts_with('/') || p.ends_with('/') || p.contains("//") {
        return Err(format!("{p:?} is not a diagnostic path: components joined by '/', none empty"));
    }
    Ok(DiagnosticPath::new(p.to_string()))
}

/// `DiagnosticsStore`: `Res<DiagnosticsStore>`. An unknown path reads `undefined`.
#[derive(SystemParam)]
pub struct StoreParam<'w> {
    store: Option<Res<'w, DiagnosticsStore>>,
}

impl StoreParam<'_> {
    fn get(&self, p: &str) -> Option<&Diagnostic> {
        self.store.as_ref()?.get(&path(p).ok()?)
    }

    fn paths(&self) -> Vec<&str> {
        let mut v: Vec<&str> = self.store.as_ref().map(|s| s.iter().map(|d| d.path().as_str()).collect()).unwrap_or_default();
        v.sort_unstable();
        v
    }
}

#[export(ts = "DiagnosticsStore", module = "bevy-ts/diagnostics")]
impl StoreParam<'_> {
    /// The latest measurement.
    pub fn value(&self, path: &str) -> Option<f64> {
        self.get(path)?.value()
    }

    /// The mean of the history.
    pub fn average(&self, path: &str) -> Option<f64> {
        self.get(path)?.average()
    }

    /// The exponential moving average.
    pub fn smoothed(&self, path: &str) -> Option<f64> {
        self.get(path)?.smoothed()
    }

    /// Measurements kept.
    pub fn history_len(&self, path: &str) -> Option<u32> {
        Some(self.get(path)?.history_len() as u32)
    }

    /// The most measurements the history keeps.
    pub fn max_history_length(&self, path: &str) -> Option<u32> {
        Some(self.get(path)?.get_max_history_length() as u32)
    }

    /// Seconds between the oldest and newest measurement.
    pub fn duration(&self, path: &str) -> Option<f64> {
        Some(self.get(path)?.duration()?.as_secs_f64())
    }

    /// Whether it is measured.
    pub fn is_enabled(&self, path: &str) -> Option<bool> {
        Some(self.get(path)?.is_enabled)
    }

    /// The unit it is logged with (`ms`, `%`).
    pub fn suffix(&self, path: &str) -> Option<String> {
        Some(self.get(path)?.suffix.to_string())
    }

    /// The history, oldest first, into `out`; returns how many were written.
    pub fn history(&self, path: &str, out: &mut [f64]) -> u32 {
        let Some(d) = self.get(path) else { return 0 };
        let mut n = 0;
        for (o, v) in out.iter_mut().zip(d.values()) {
            *o = *v;
            n += 1;
        }
        n
    }

    /// How many diagnostics are registered. Every method here is exported, so no `is_empty`.
    #[allow(clippy::len_without_is_empty)]
    pub fn len(&self) -> u32 {
        self.paths().len() as u32
    }

    /// The `i`th registered path, sorted.
    pub fn path(&self, i: u32) -> Option<String> {
        self.paths().get(i as usize).map(|p| p.to_string())
    }
}

/// `Diagnostics`: Bevy's writer param. A measurement applies after the system runs, and is
/// dropped when its path is unregistered or disabled, as from a Rust system.
#[derive(SystemParam)]
pub struct DiagnosticsParam<'w, 's> {
    inner: bevy::diagnostic::Diagnostics<'w, 's>,
}

#[export(ts = "Diagnostics", module = "bevy-ts/diagnostics")]
impl DiagnosticsParam<'_, '_> {
    /// Adds `value` to the diagnostic at `path`.
    pub fn add_measurement(&mut self, path: &str, value: f64) -> Result<(), String> {
        let p = self::path(path)?;
        self.inner.add_measurement(&p, || value);
        Ok(())
    }
}

/// `FrameCount`: Bevy's frame counter, updated in `Last`.
#[derive(SystemParam)]
pub struct FrameCountParam<'w> {
    count: Option<Res<'w, FrameCount>>,
}

#[export(ts = "FrameCount", module = "bevy-ts/diagnostics")]
impl FrameCountParam<'_> {
    /// Frames since the app started, wrapping at `u32::MAX`.
    pub fn value(&self) -> u32 {
        self.count.as_ref().map_or(0, |c| c.0)
    }
}

/// One `app.registerDiagnostic` call, as the module writes it.
#[derive(Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
struct Declared {
    path: String,
    suffix: String,
    max_history_length: usize,
    smoothing_factor: f64,
    #[serde(default)]
    location: Option<SourceLocation>,
}

impl Declared {
    fn same(&self, o: &Declared) -> bool {
        self.suffix == o.suffix && self.max_history_length == o.max_history_length && self.smoothing_factor == o.smoothing_factor
    }

    fn diagnostic(&self) -> Diagnostic {
        Diagnostic::new(DiagnosticPath::new(self.path.clone()))
            .with_suffix(self.suffix.clone())
            .with_max_history_length(self.max_history_length)
            .with_smoothing_factor(self.smoothing_factor)
    }
}

#[derive(Deserialize)]
struct Section {
    items: Vec<Declared>,
}

/// The paths scripts registered, with their settings: the rest of the store is Rust's.
#[derive(Resource, Default)]
pub struct ScriptOwned(BTreeMap<String, Declared>);

/// Fails the load on a bad path, a path declared twice, or a path a Rust plugin owns.
fn check(world: &World, input: &SectionInput) -> Result<Vec<Declared>, Vec<ScriptError>> {
    let section: Section = serde_json::from_slice(input.bytes).map_err(|e| vec![ScriptError::at(format!("diagnostics section: {e}"), input.location.clone())])?;
    let owned = world.resource::<ScriptOwned>();
    let store = world.get_resource::<DiagnosticsStore>();
    let mut errors = Vec::new();
    let mut seen = BTreeMap::new();
    for d in &section.items {
        let at = d.location.clone().or_else(|| input.location.clone());
        let fail = |m: String| ScriptError::at(format!("registerDiagnostic({:?}): {m}", d.path), at.clone());
        match path(&d.path) {
            Err(m) => errors.push(fail(m)),
            Ok(p) if !owned.0.contains_key(&d.path) && store.is_some_and(|s| s.get(&p).is_some()) => {
                errors.push(fail("a Rust plugin registered this path; read it with DiagnosticsStore, or register another".into()))
            }
            Ok(_) if seen.insert(d.path.clone(), ()).is_some() => errors.push(fail("is registered twice".into())),
            Ok(_) if d.max_history_length == 0 => errors.push(fail("withMaxHistoryLength takes 1 or more".into())),
            Ok(_) => {}
        }
    }
    if errors.is_empty() { Ok(section.items) } else { Err(errors) }
}

/// A generation's declarations, applied: new paths added, unchanged ones kept with their history,
/// changed ones replaced, and ones no longer declared disabled.
fn apply(world: &mut World, s: &AppliedSection) {
    let items: Vec<Declared> = if s.present { serde_json::from_slice::<Section>(s.bytes).map(|s| s.items).unwrap_or_default() } else { Vec::new() };
    let mut owned = std::mem::take(&mut world.resource_mut::<ScriptOwned>().0);
    let mut store = world.resource_mut::<DiagnosticsStore>();
    let declared: BTreeMap<String, Declared> = items.into_iter().map(|d| (d.path.clone(), d)).collect();
    for (p, d) in &declared {
        match owned.get(p) {
            Some(old) if old.same(d) => {
                if let Some(live) = store.get_mut(&DiagnosticPath::new(p.clone())) {
                    live.is_enabled = true;
                }
            }
            _ => store.add(d.diagnostic()),
        }
    }
    for (p, _) in owned.iter().filter(|(p, _)| !declared.contains_key(*p)) {
        if let Some(live) = store.get_mut(&DiagnosticPath::new(p.clone())) {
            live.is_enabled = false;
        }
    }
    // A dropped path stays owned: declaring it again later is a script's, not a clash.
    for (p, d) in declared {
        owned.insert(p, d);
    }
    world.resource_mut::<ScriptOwned>().0 = owned;
}

/// The Bevy plugins `bevy-ts.json`'s `diagnostics` may list, by Bevy's names, and bevy-ts's own
/// `ScriptStageDiagnosticsPlugin`. `RenderAssetDiagnosticPlugin<T>` takes the render assets the
/// engine prepares, named as Bevy names the type.
pub const PLUGINS: &[&str] = &[
    "FrameTimeDiagnosticsPlugin",
    "EntityCountDiagnosticsPlugin",
    "SystemInformationDiagnosticsPlugin",
    "LogDiagnosticsPlugin",
    "RenderDiagnosticsPlugin",
    "MeshAllocatorDiagnosticPlugin",
    "RenderAssetDiagnosticPlugin<RenderMesh>",
    "RenderAssetDiagnosticPlugin<GpuImage>",
    "RenderAssetDiagnosticPlugin<GpuShaderBuffer>",
    "ScriptStageDiagnosticsPlugin",
];

/// Checks a `diagnostics` list: every name one of [`PLUGINS`], none twice.
pub fn check_names(names: &[String]) -> Result<(), String> {
    for (i, n) in names.iter().enumerate() {
        if !PLUGINS.contains(&n.as_str()) {
            return Err(format!("diagnostics: {n:?} is not a diagnostics plugin bevy-ts adds; use one of {}", PLUGINS.join(", ")));
        }
        if names[..i].contains(n) {
            return Err(format!("diagnostics: {n:?} is listed twice"));
        }
    }
    Ok(())
}

/// Adds each listed plugin unless the app already has it. Render plugins only where the app
/// renders; a name not in [`PLUGINS`] was refused when the project was read.
pub fn add_plugins(app: &mut App, names: &[String]) {
    use bevy::diagnostic::{EntityCountDiagnosticsPlugin, FrameTimeDiagnosticsPlugin, LogDiagnosticsPlugin, SystemInformationDiagnosticsPlugin};
    use bevy::render::diagnostic::{MeshAllocatorDiagnosticPlugin, RenderAssetDiagnosticPlugin, RenderDiagnosticsPlugin};
    let renders = app.is_plugin_added::<bevy::render::RenderPlugin>();
    for name in names {
        match name.as_str() {
            "FrameTimeDiagnosticsPlugin" if !app.is_plugin_added::<FrameTimeDiagnosticsPlugin>() => {
                app.add_plugins(FrameTimeDiagnosticsPlugin::default());
            }
            "EntityCountDiagnosticsPlugin" if !app.is_plugin_added::<EntityCountDiagnosticsPlugin>() => {
                app.add_plugins(EntityCountDiagnosticsPlugin::default());
            }
            "SystemInformationDiagnosticsPlugin" if !app.is_plugin_added::<SystemInformationDiagnosticsPlugin>() => {
                app.add_plugins(SystemInformationDiagnosticsPlugin);
            }
            "LogDiagnosticsPlugin" if !app.is_plugin_added::<LogDiagnosticsPlugin>() => {
                app.add_plugins(LogDiagnosticsPlugin::default());
            }
            "RenderDiagnosticsPlugin" if renders && !app.is_plugin_added::<RenderDiagnosticsPlugin>() => {
                app.add_plugins(RenderDiagnosticsPlugin);
            }
            "MeshAllocatorDiagnosticPlugin" if renders && !app.is_plugin_added::<MeshAllocatorDiagnosticPlugin>() => {
                app.add_plugins(MeshAllocatorDiagnosticPlugin);
            }
            "RenderAssetDiagnosticPlugin<RenderMesh>" if renders && !app.is_plugin_added::<RenderAssetDiagnosticPlugin<bevy::render::mesh::RenderMesh>>() => {
                app.add_plugins(RenderAssetDiagnosticPlugin::<bevy::render::mesh::RenderMesh>::new(" meshes"));
            }
            "RenderAssetDiagnosticPlugin<GpuImage>" if renders && !app.is_plugin_added::<RenderAssetDiagnosticPlugin<bevy::render::texture::GpuImage>>() => {
                app.add_plugins(RenderAssetDiagnosticPlugin::<bevy::render::texture::GpuImage>::new(" images"));
            }
            "RenderAssetDiagnosticPlugin<GpuShaderBuffer>" if renders && !app.is_plugin_added::<RenderAssetDiagnosticPlugin<bevy::render::storage::GpuShaderBuffer>>() => {
                app.add_plugins(RenderAssetDiagnosticPlugin::<bevy::render::storage::GpuShaderBuffer>::new(" buffers"));
            }
            "ScriptStageDiagnosticsPlugin" if !app.is_plugin_added::<bevy_ts::systems::ScriptStageDiagnosticsPlugin>() => {
                app.add_plugins(bevy_ts::systems::ScriptStageDiagnosticsPlugin);
            }
            _ => {}
        }
    }
}
