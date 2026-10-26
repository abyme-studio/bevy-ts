//! The engine's gizmos (openspec script-gizmos): `GizmosPlugin` exposes Bevy's five config groups
//! to TS and ships `bevy-ts/gizmos`, which declares `Gizmos<G>`, its builders, the groups and
//! `GizmoAsset`.
//! The param and its method table are `bevy_ts_native::gizmos`.

use bevy::gizmos::aabb::AabbGizmoConfigGroup;
use bevy::gizmos::config::{DefaultGizmoConfigGroup, GizmoConfig};
use bevy::gizmos::frustum::FrustumGizmoConfigGroup;
use bevy::gizmos::skinned_mesh_bounds::SkinnedMeshBoundsGizmoConfigGroup;
use bevy::light::gizmos::LightGizmoConfigGroup;
use bevy::prelude::*;
use bevy::reflect::TypePath;
use bevy_ts_native::BevyTsNativeExt;
use bevy_ts_native::gizmos::config::{TsGizmoConfigStore, TsGizmoConfigStoreRead};
use bevy_ts_native::gizmos::{primitives, retained};
use bevy_ts_native::gizmos::{MODULE, TsGizmoGroups};
use std::fmt::Write as _;

/// Bevy's groups, each a constant of `bevy-ts/gizmos`. bevy-ts's own editor groups are never
/// exposed.
const BEVY_GROUPS: [&str; 5] = ["DefaultGizmoConfigGroup", "AabbGizmoConfigGroup", "FrustumGizmoConfigGroup", "SkinnedMeshBoundsGizmoConfigGroup", "LightGizmoConfigGroup"];

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

/// What `bevy-ts/gizmos` declares from the registry: each exposed group's value
/// (`GizmoGroupValues`), `GizmoConfig`, and a type for each group Bevy's five are not.
pub fn module_types(world: &World) -> Option<String> {
    let groups = world.get_resource::<TsGizmoGroups>()?;
    let registry = world.resource::<AppTypeRegistry>().read();
    let mut s = String::from("/** Each group's own config (`configExt()`), as reflection reads it. */\nexport interface GizmoGroupValues {\n");
    for g in &groups.0 {
        let _ = writeln!(s, "  {}: {};", g.short, crate::types::ts_type(&registry, g.type_path, 1));
    }
    s.push_str("}\n");
    let _ = writeln!(s, "/** Bevy's `GizmoConfig`, as reflection reads and writes it. */\nexport type GizmoConfig = {};", crate::types::ts_type(&registry, GizmoConfig::type_path(), 1));
    for g in groups.0.iter().filter(|g| !BEVY_GROUPS.contains(&g.short)) {
        let _ = writeln!(s, "export type {} = {:?};", g.short, g.short);
    }
    // The primitives and curves TS may draw: one-key objects of their reflected values.
    let [p2, p3, c2, c3] = primitives::type_paths();
    for (name, doc, keys) in [
        ("Primitive2d", "A 2D primitive for `primitive2d`: its type, then its fields over the type's default.", p2),
        ("Primitive3d", "A 3D primitive for `primitive3d`: its type, then its fields over the type's default.", p3),
        ("Curve2d", "A curve of `Vec2` for `curve2d`, as reflection writes it.", c2),
        ("Curve3d", "A curve of `Vec3` for `curve3d`, as reflection writes it.", c3),
    ] {
        let alts: Vec<String> = keys.iter().map(|(k, path)| format!("{{ {k}: {} }}", crate::types::ts_type(&registry, path, 1))).collect();
        let _ = writeln!(s, "/** {doc} */\nexport type {name} =\n  | {};", alts.join("\n  | "));
    }
    Some(s)
}
