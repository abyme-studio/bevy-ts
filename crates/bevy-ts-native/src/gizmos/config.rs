//! `GizmoConfigStore` for TS (design 6): `ResMut<GizmoConfigStore>` reads and writes a group's
//! config, `Res<GizmoConfigStore>` reads it, both through `bevy_ts::reflect_value`. One export per
//! spelling; `bevy-ts/gizmos` declares their types, generic over the group.

use super::{MODULE, TsGizmoGroups};
use bevy::ecs::system::SystemParam;
use bevy::ecs::world::unsafe_world_cell::UnsafeWorldCell;
use bevy::gizmos::config::GizmoConfigStore;
use bevy::prelude::*;
use bevy_ts::host::{self, HostJson, HostValue, Kind, MethodSpec, ParamSpec, ParamState, RunTicks, SlotReader, SlotWriter};
use std::any::TypeId;

/// `ResMut<GizmoConfigStore>`: `config`, `insert`, `groups`.
#[derive(SystemParam)]
pub struct TsGizmoConfigStore<'w> {
    store: ResMut<'w, GizmoConfigStore>,
    groups: Res<'w, TsGizmoGroups>,
    registry: Res<'w, AppTypeRegistry>,
}

/// `Res<GizmoConfigStore>`: `config`, `groups`.
#[derive(SystemParam)]
pub struct TsGizmoConfigStoreRead<'w> {
    store: Res<'w, GizmoConfigStore>,
    groups: Res<'w, TsGizmoGroups>,
    registry: Res<'w, AppTypeRegistry>,
}

/// The id of the exposed group named `name`, or why not.
fn group(groups: &TsGizmoGroups, name: &str) -> Result<TypeId, String> {
    groups.0.iter().find(|g| g.short == name || g.type_path == name).map(|g| g.type_id).ok_or_else(|| {
        let names: Vec<&str> = groups.0.iter().map(|g| g.short).collect();
        format!("{name} is not a gizmo group exposed to TS; the groups are {}", names.join(", "))
    })
}

/// `[config, ext]` of group `name`, as reflection reads them.
fn read(store: &GizmoConfigStore, groups: &TsGizmoGroups, registry: &AppTypeRegistry, name: &str) -> Result<HostJson, String> {
    let id = group(groups, name)?;
    let (config, ext) = store.get_config_dyn(&id).ok_or_else(|| format!("{name} has no config in GizmoConfigStore"))?;
    let r = registry.read();
    let config = bevy_ts::reflect_value::read(&r, config)?;
    let ext = bevy_ts::reflect_value::read(&r, ext.as_partial_reflect())?;
    Ok(HostJson(serde_json::json!([config, ext])))
}

fn names(groups: &TsGizmoGroups) -> HostJson {
    HostJson(groups.0.iter().map(|g| g.short).collect::<Vec<_>>().into())
}

macro_rules! readers {
    ($param:ident, $config:ident, $groups:ident) => {
        fn $config(state: &mut ParamState, world: UnsafeWorldCell<'_>, ticks: RunTicks, r: &mut SlotReader, w: &mut SlotWriter, _: &mut [&mut [u8]]) -> Result<(), String> {
            // SAFETY: as `#[export]`'s calls.
            let p = unsafe { host::fetch::<$param<'static>>(state, world, ticks) }?;
            let name = String::read(r).map_err(|e| format!("group: {e}"))?;
            read(&p.store, &p.groups, &p.registry, &name)?.write(w);
            Ok(())
        }

        fn $groups(state: &mut ParamState, world: UnsafeWorldCell<'_>, ticks: RunTicks, _: &mut SlotReader, w: &mut SlotWriter, _: &mut [&mut [u8]]) -> Result<(), String> {
            // SAFETY: as `#[export]`'s calls.
            let p = unsafe { host::fetch::<$param<'static>>(state, world, ticks) }?;
            names(&p.groups).write(w);
            Ok(())
        }
    };
}

readers!(TsGizmoConfigStore, config_mut, groups_mut);
readers!(TsGizmoConfigStoreRead, config_read, groups_read);

/// `insert(group, config, ext)`: each written over the group's current value, keys left out keeping
/// theirs; a bad field throws before either is stored.
fn insert(state: &mut ParamState, world: UnsafeWorldCell<'_>, ticks: RunTicks, r: &mut SlotReader, _: &mut SlotWriter, _: &mut [&mut [u8]]) -> Result<(), String> {
    // SAFETY: as `#[export]`'s calls.
    let mut p = unsafe { host::fetch::<TsGizmoConfigStore<'static>>(state, world, ticks) }?;
    let name = String::read(r).map_err(|e| format!("group: {e}"))?;
    let config_json = HostJson::read(r).map_err(|e| format!("config: {e}"))?.0;
    let ext_json = HostJson::read(r).map_err(|e| format!("ext: {e}"))?.0;
    let id = group(&p.groups, &name)?;
    let registry = p.registry.clone();
    let r = registry.read();
    let (config, ext) = p.store.get_config_dyn(&id).ok_or_else(|| format!("{name} has no config in GizmoConfigStore"))?;
    let mut config = config.clone();
    bevy_ts::reflect_value::write(&r, None, &mut config, None, &config_json).map_err(|e| format!("{name} config: {e}"))?;
    let mut ext = ext.reflect_clone().map_err(|e| format!("{name}: {e}"))?;
    bevy_ts::reflect_value::write(&r, None, ext.as_partial_reflect_mut(), None, &ext_json).map_err(|e| format!("{name} ext: {e}"))?;
    let (config_slot, ext_slot) = p.store.get_config_mut_dyn(&id).expect("read above");
    // The value first: if it refuses, the config is left as it was too.
    ext_slot.try_apply(ext.as_partial_reflect()).map_err(|e| format!("{name} ext: {e}"))?;
    *config_slot = config;
    Ok(())
}

fn method(name: &str, args: Vec<host::ArgSpec>, ret: Kind, call: host::HostFn) -> MethodSpec {
    MethodSpec { name: name.into(), args, ret, call, chain: None, of: None }
}

fn group_arg() -> host::ArgSpec {
    host::ArgSpec::Value("group", Kind::Str)
}

impl crate::ExportParam for TsGizmoConfigStore<'static> {
    fn spec() -> ParamSpec {
        ParamSpec {
            ts: "GizmoConfigStoreMut",
            module: MODULE,
            rust: std::any::type_name::<Self>(),
            stands_for: Some("ResMut<GizmoConfigStore>"),
            init: host::init::<Self>,
            methods: vec![
                method("config", vec![group_arg()], Kind::Json("unknown"), config_mut),
                method(
                    "insert",
                    vec![group_arg(), host::ArgSpec::Value("config", Kind::Json("unknown")), host::ArgSpec::Value("ext", Kind::Json("unknown"))],
                    Kind::Unit,
                    insert,
                ),
                method("groups", vec![], Kind::Json("unknown"), groups_mut),
            ],
            module_declares: true,
        }
    }
}

impl crate::ExportParam for TsGizmoConfigStoreRead<'static> {
    fn spec() -> ParamSpec {
        ParamSpec {
            ts: "GizmoConfigStoreRead",
            module: MODULE,
            rust: std::any::type_name::<Self>(),
            stands_for: Some("Res<GizmoConfigStore>"),
            init: host::init::<Self>,
            methods: vec![method("config", vec![group_arg()], Kind::Json("unknown"), config_read), method("groups", vec![], Kind::Json("unknown"), groups_read)],
            module_declares: true,
        }
    }
}

/// The TS declarations of both, generic over the group.
pub const DECLARATION: &str = r#"/** Bevy's `GizmoConfigStore`: take `ResMut<GizmoConfigStore>` to read and write a group's config, `Res<GizmoConfigStore>` to read it. */
export declare class GizmoConfigStore {
  private readonly gizmoConfigStore: true;
}
/** `Res<GizmoConfigStore>`. */
export interface GizmoConfigStoreRead {
  /** The group's `GizmoConfig` and its own config, as reflection reads them. */
  config<G extends GizmoGroup>(group: G): [GizmoConfig, GizmoGroupValues[G]];
  /** The groups exposed to TS. */
  groups(): GizmoGroup[];
}
/** `ResMut<GizmoConfigStore>`. */
export interface GizmoConfigStoreMut extends GizmoConfigStoreRead {
  /** Writes each over the group's current value, keys left out keeping theirs; a bad field throws and changes nothing. */
  insert<G extends GizmoGroup>(group: G, config: GizmoConfigPatch, ext: Partial<GizmoGroupValues[G]>): void;
}
"#;
