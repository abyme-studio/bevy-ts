//! Bevy's `Gizmos` for TS (openspec script-gizmos): `TsGizmos<C>` is `Gizmos<C>` with the builder
//! a setting may redraw, exported once per config group a crate registers with
//! [`crate::BevyTsNativeExt::ts_gizmo_group`]. Its methods come from [`draw`]'s table.
//!
//! `ExportParam` is written by hand from the table, not with `#[export]`: the export is generic
//! over the group, which `#[export]` does not take, and the same table drives the retained replay
//! ([`retained`]).

pub mod config;
pub mod draw;
pub mod primitives;
pub mod retained;

use bevy::app::{App, Plugin};
use bevy::ecs::system::SystemParam;
use bevy::ecs::world::unsafe_world_cell::UnsafeWorldCell;
use bevy::gizmos::config::{DefaultGizmoConfigGroup, GizmoConfigGroup, GizmoConfigStore};
use bevy::prelude::*;
use bevy_ts::host::{self, HostJson, HostValue, Kind, MethodSpec, ParamSpec, ParamState, RunTicks, SlotReader, SlotWriter};
use std::any::TypeId;
use std::sync::Mutex;

/// The module that declares `Gizmos<G>`, its builders and the groups.
pub const MODULE: &str = "bevy-ts/gizmos";

/// `Gizmos<C>` for TS: Bevy's param, the builder a setting may still redraw, and the registry
/// `config()` reads through.
#[derive(SystemParam)]
pub struct TsGizmos<'w, 's, C: GizmoConfigGroup> {
    pub gizmos: Gizmos<'w, 's, C>,
    open: Local<'s, draw::OpenBuilder>,
    registry: Res<'w, AppTypeRegistry>,
}

/// A group exposed to TS: its short name (`LightGizmoConfigGroup`), type path and id.
#[derive(Clone, Debug)]
pub struct GizmoGroup {
    pub short: &'static str,
    pub type_path: &'static str,
    pub type_id: TypeId,
}

/// The groups exposed to TS, in registration order.
#[derive(Resource, Default, Debug)]
pub struct TsGizmoGroups(pub Vec<GizmoGroup>);

/// The TS name and claimed spelling of `C`'s export, leaked once per group: `Gizmos` and
/// `Gizmos<DefaultGizmoConfigGroup>` for the default group, `Gizmos_<Short>` and `Gizmos<Short>`
/// for the rest.
pub fn names<C: GizmoConfigGroup>() -> (&'static str, &'static str) {
    static NAMES: Mutex<Vec<(TypeId, &'static str, &'static str)>> = Mutex::new(Vec::new());
    let id = TypeId::of::<C>();
    let mut names = NAMES.lock().unwrap();
    if let Some(&(_, ts, claim)) = names.iter().find(|(t, ..)| *t == id) {
        return (ts, claim);
    }
    let short = C::short_type_path();
    let ts: &'static str = if id == TypeId::of::<DefaultGizmoConfigGroup>() { "Gizmos" } else { Box::leak(format!("Gizmos_{short}").into_boxed_str()) };
    let claim: &'static str = Box::leak(host::normalise_spelling(&format!("Gizmos<{short}>")).into_boxed_str());
    names.push((id, ts, claim));
    (ts, claim)
}

/// `config()` or (`EXT`) `configExt()`: the group's `GizmoConfig` or value, read by reflection.
fn call_config<C: GizmoConfigGroup, const EXT: bool>(
    state: &mut ParamState,
    world: UnsafeWorldCell<'_>,
    ticks: RunTicks,
    _: &mut SlotReader,
    w: &mut SlotWriter,
    _: &mut [&mut [u8]],
) -> Result<(), String> {
    // SAFETY: as `draw`'s calls.
    let p = unsafe { host::fetch::<TsGizmos<'static, 'static, C>>(state, world, ticks) }?;
    let registry = p.registry.read();
    let value: &dyn PartialReflect = if EXT { p.gizmos.config_ext } else { p.gizmos.config };
    HostJson(bevy_ts::reflect_value::read(&registry, value)?).write(w);
    Ok(())
}

impl<C: GizmoConfigGroup> crate::ExportParam for TsGizmos<'static, 'static, C> {
    fn spec() -> ParamSpec {
        let (ts, claim) = names::<C>();
        let mut methods = draw::methods::<C>();
        let config = |name: &str, ty, call| MethodSpec { name: name.into(), args: Vec::new(), ret: Kind::Json(ty), call, chain: None, of: None };
        methods.push(config("config", "GizmoConfig", call_config::<C, false>));
        methods.push(config("configExt", "GizmoGroupValues[G]", call_config::<C, true>));
        ParamSpec {
            ts,
            module: MODULE,
            rust: std::any::type_name::<Self>(),
            stands_for: Some(claim),
            init: host::init::<Self>,
            methods,
            module_declares: true,
        }
    }
}

/// Registers `G`'s export and records the group for `bevy-ts/gizmos`; [`GizmoGroupCheck`] fails
/// the app at `finish` for a group Bevy never registered.
pub(crate) fn expose<G: GizmoConfigGroup>(app: &mut App) {
    if !app.is_plugin_added::<GizmoGroupCheck>() {
        app.add_plugins(GizmoGroupCheck);
        primitives::register_types(app);
        retained::register(app);
    }
    let mut groups = app.world_mut().get_resource_or_init::<TsGizmoGroups>();
    if groups.0.iter().any(|g| g.type_id == TypeId::of::<G>()) {
        return;
    }
    groups.0.push(GizmoGroup { short: G::short_type_path(), type_path: G::type_path(), type_id: TypeId::of::<G>() });
    crate::BevyTsNativeExt::ts_param::<TsGizmos<'static, 'static, G>>(app);
}

/// Fails the app at `finish` for an exposed group missing from `GizmoConfigStore`.
pub struct GizmoGroupCheck;

impl Plugin for GizmoGroupCheck {
    fn build(&self, _: &mut App) {}

    fn finish(&self, app: &mut App) {
        if let Some(missing) = missing_groups(app.world()) {
            panic!("{missing}");
        }
    }
}

/// Why the exposed groups cannot be drawn into, if one was never registered with Bevy.
pub fn missing_groups(world: &World) -> Option<String> {
    let groups = world.get_resource::<TsGizmoGroups>()?;
    let store = world.get_resource::<GizmoConfigStore>();
    let missing: Vec<&str> = groups.0.iter().filter(|g| store.is_none_or(|s| s.get_config_dyn(&g.type_id).is_none())).map(|g| g.type_path).collect();
    (!missing.is_empty()).then(|| {
        format!("bevy-ts: ts_gizmo_group exposes {} to TS, but Bevy has no such gizmo group; call app.init_gizmo_group::<G>() first", missing.join(", "))
    })
}

/// The declaration of `Gizmos<G>` and its builders, from the default group's export: every group's
/// export has the same methods, `configExt()` typed by `G`.
pub fn declaration() -> String {
    let spec = <TsGizmos<'static, 'static, DefaultGizmoConfigGroup> as crate::ExportParam>::spec();
    host::declaration_as(&spec, "Gizmos<G extends GizmoGroup = DefaultGizmoConfigGroup>")
}
