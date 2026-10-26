//! What `bevy-ts types` declares of `bevy-ts/gizmos` from the registry; the plugin and the module
//! every engine ships are `bevy_ts_embed::gizmos`.

use bevy::gizmos::config::GizmoConfig;
use bevy::prelude::*;
use bevy::reflect::TypePath;
use bevy_ts_embed::gizmos::BEVY_GROUPS;
use bevy_ts_native::gizmos::{TsGizmoGroups, primitives};
use std::fmt::Write as _;

pub use bevy_ts_embed::gizmos::GizmosPlugin;

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
