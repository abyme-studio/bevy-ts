//! The plugin set every engine ships: bevy-ts, tsx, script materials, animation, script input,
//! levels and cameras. `bevy-ts dev`, `bevy-ts play` and an embedding host add the same set, so
//! what a game sees cannot differ between them.

use bevy::picking::mesh_picking::{MeshPickingPlugin, MeshPickingSettings};
use bevy::prelude::*;
use bevy_ts::BevyTsPlugin;
use bevy_ts::engine::EngineIdentity;
use bevy_ts_tsx::events::TsxHandlers;
use bevy_ts_v8::V8Options;
use std::path::Path;

/// Adds the shipped set after Bevy's plugins: bevy-ts on V8 (`dev` for `bevy-ts dev`), then the
/// feature crates, and `identity` as the engine's.
pub fn add_shipped(app: &mut App, identity: EngineIdentity, options: V8Options, dev: bool) {
    app.add_plugins(BevyTsPlugin::new(bevy_ts_v8::factory(options)).dev(dev))
        .add_plugins(bevy_ts_tsx::TsxPlugin)
        .add_plugins(bevy_ts_material::ScriptMaterialPlugin)
        .add_plugins(bevy_ts_animation::PlayAnimationPlugin)
        .add_plugins(crate::script_input::ScriptInputPlugin)
        .add_plugins(bevy_ts_levels::engine::LevelsPlugin)
        .add_plugins(crate::camera::CamerasPlugin)
        .add_systems(PostUpdate, warn_mesh_handlers.run_if(not(resource_exists::<MeshPickingSettings>)))
        .insert_resource(identity);
}

/// After the project's plugins: `meshPicking` adds Bevy's `MeshPickingPlugin` (the one place it is
/// added, so dev and play cannot differ), unless a native crate added it with its own settings.
/// `assets` is where levels are read and written.
pub fn finish_shipped(app: &mut App, mesh_picking: bool, assets: Option<&Path>) {
    if mesh_picking && !app.is_plugin_added::<MeshPickingPlugin>() {
        app.add_plugins(MeshPickingPlugin);
    }
    if let Some(root) = assets {
        app.world_mut().resource_mut::<bevy_ts_levels::engine::Levels>().asset_root = root.to_path_buf();
    }
}

/// Sets what scripts may connect to and see of the environment, from `net.allow` and `env`,
/// reading `${NAME}` entries and the variables from `lookup` now. Skipped entries are logged.
pub fn insert_net_policy(app: &mut App, allow: &[String], env: &[String], lookup: &dyn Fn(&str) -> Option<String>) {
    let (policy, warnings) = bevy_ts::net::NetPolicy::from_config(allow, env, lookup);
    app.insert_resource(policy);
    let mut log = app.world_mut().resource_mut::<bevy_ts::log::ScriptLog>();
    for w in warnings {
        log.push(bevy_ts::log::LogLevel::Warn, w, None, 0);
    }
}

/// A mesh that has a pointer handler, one of the two just added.
type NewMeshHandler = (With<Mesh3d>, With<TsxHandlers>, Or<(Added<Mesh3d>, Added<TsxHandlers>)>);

/// Names, once per process, a mesh with a pointer handler while nothing picks meshes: its
/// `onPointer*` would never fire. Checks the resource, not the switch, so a native crate that adds
/// `MeshPickingPlugin` itself is not warned.
fn warn_mesh_handlers(
    added: Query<(Entity, Option<&Name>), NewMeshHandler>,
    log: Option<ResMut<bevy_ts::log::ScriptLog>>,
    generations: Option<Res<bevy_ts::generation::Generations>>,
    mut warned: Local<bool>,
) {
    if *warned {
        return;
    }
    let (Some((e, name)), Some(mut log)) = (added.iter().next(), log) else { return };
    *warned = true;
    let name = name.map(|n| format!(" \"{n}\"")).unwrap_or_default();
    let message = format!(
        "entity {e}{name} has a pointer handler on a mesh, but mesh picking is off: it never fires. Set \"meshPicking\": true in bevy-ts.json"
    );
    log.push(bevy_ts::log::LogLevel::Warn, message, None, generations.map_or(0, |g| g.live));
}
