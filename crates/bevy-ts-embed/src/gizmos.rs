//! The engine's gizmos (openspec script-gizmos): `GizmosPlugin` exposes Bevy's five config groups
//! to TS and ships `bevy-ts/gizmos`, which declares `Gizmos<G>`, its builders, the groups and
//! `GizmoAsset`.
//! The param and its method table are `bevy_ts_native::gizmos`; what `bevy-ts types` declares from
//! the registry is the CLI's (`bevy_ts_cli::gizmos::module_types`).

use bevy::gizmos::aabb::AabbGizmoConfigGroup;
use bevy::gizmos::config::DefaultGizmoConfigGroup;
use bevy::gizmos::frustum::FrustumGizmoConfigGroup;
use bevy::gizmos::skinned_mesh_bounds::SkinnedMeshBoundsGizmoConfigGroup;
use bevy::light::gizmos::LightGizmoConfigGroup;
use bevy::prelude::*;
use bevy_ts_native::BevyTsNativeExt;
use bevy_ts_native::gizmos::config::{TsGizmoConfigStore, TsGizmoConfigStoreRead};
use bevy_ts_native::gizmos::retained;
use bevy_ts_native::gizmos::MODULE;
use std::fmt::Write as _;

/// Bevy's groups, each a constant of `bevy-ts/gizmos`. bevy-ts's own editor groups are never
/// exposed.
pub const BEVY_GROUPS: [&str; 5] = ["DefaultGizmoConfigGroup", "AabbGizmoConfigGroup", "FrustumGizmoConfigGroup", "SkinnedMeshBoundsGizmoConfigGroup", "LightGizmoConfigGroup"];

/// Exposes Bevy's gizmo groups to TS and ships `bevy-ts/gizmos`. Every engine adds it.
pub struct GizmosPlugin;

impl Plugin for GizmosPlugin {
    fn build(&self, app: &mut App) {
        app.ts_gizmo_group::<DefaultGizmoConfigGroup>()
            .ts_gizmo_group::<AabbGizmoConfigGroup>()
            .ts_gizmo_group::<FrustumGizmoConfigGroup>()
            .ts_gizmo_group::<SkinnedMeshBoundsGizmoConfigGroup>()
            .ts_gizmo_group::<LightGizmoConfigGroup>()
            .ts_param::<TsGizmoConfigStore<'static>>()
            .ts_param::<TsGizmoConfigStoreRead<'static>>()
            .ts_module(MODULE, &module_js(), &module_dts());
    }
}

fn module_js() -> String {
    // `GizmoConfigStore` is only named in `ResMut<GizmoConfigStore>`, which reads as the export.
    let groups: String = BEVY_GROUPS.iter().map(|g| format!("export const {g} = {g:?};\n")).collect();
    format!("import {{ __hostAsset }} from \"bevy-ts\";\n{groups}export class GizmoConfigStore {{}}\n{}", retained::module_js())
}

fn module_dts() -> String {
    let mut s = String::from("import type { AssetClass, AssetFields, ColorValue, Entity } from \"bevy-ts\";\n");
    s.push_str("/** A gizmo config group, by its short name: `Gizmos<G>` draws into its storage. A Rust crate adds one with `app.ts_gizmo_group::<G>()`. */\n");
    s.push_str("export type GizmoGroup = keyof GizmoGroupValues;\n");
    for g in BEVY_GROUPS {
        let _ = writeln!(s, "export type {g} = {g:?};\nexport declare const {g}: {g};");
    }
    s.push_str("type DeepPartial<T> = T extends readonly unknown[] ? T : T extends object ? { [K in keyof T]?: DeepPartial<T[K]> } : T;\n");
    s.push_str("/** What `GizmoConfigStore.insert` takes: any part of a `GizmoConfig`. */\nexport type GizmoConfigPatch = DeepPartial<GizmoConfig>;\n");
    s.push_str(bevy_ts_native::gizmos::config::DECLARATION);
    s + &bevy_ts_native::gizmos::declaration() + &retained::declaration()
}
