//! The plugin set every engine ships: bevy-ts, tsx, script materials, world text, pixel 2D and
//! bitmap text, animation, script input, script time, levels, cameras, gizmos, Rust events,
//! diagnostics, the session, the ready hold and script assets.
//! `bevy-ts dev`, `bevy-ts play` and an embedding host add the same set, so what a game sees
//! cannot differ between them. Audio is the CLI's (`bevy_ts_cli::audio`): an embedding host has
//! none.

use crate::play::Switches;
use bevy::app::PluginGroupBuilder;
use bevy::input_focus::directional_navigation::DirectionalNavigationPlugin;
use bevy::input_focus::tab_navigation::TabNavigationPlugin;
use bevy::picking::mesh_picking::{MeshPickingPlugin, MeshPickingSettings};
use bevy::prelude::*;
use bevy_ts::BevyTsPlugin;
use bevy_ts::engine::EngineIdentity;
use bevy_ts_tsx::events::TsxHandlers;
use bevy_ts_v8::V8Options;
use std::path::Path;

/// Every plugin of the shipped set that gives scripts a built-in module (`bevy-ts/ready`) or
/// element. The CLI's `ScriptPlugins` is this and `bevy-ts/audio`, which `bevy-ts types` adds, so
/// the generated types declare every module a game can import.
pub struct ScriptPlugins;

impl PluginGroup for ScriptPlugins {
    fn build(self) -> PluginGroupBuilder {
        PluginGroupBuilder::start::<Self>()
            .add(bevy_ts_tsx::TsxPlugin)
            .add(bevy_ts_material::ScriptMaterialPlugin)
            .add(bevy_ts_material::WorldTextPlugin)
            .add(bevy_ts_animation::PlayAnimationPlugin)
            .add(crate::script_input::ScriptInputPlugin)
            .add(crate::time::ScriptTimePlugin)
            .add(crate::camera::CamerasPlugin)
            .add(crate::gizmos::GizmosPlugin)
            .add(crate::rust_events::RustEventsPlugin)
            .add(crate::diagnostics::ScriptDiagnosticsPlugin)
            .add(crate::session::ScriptSessionPlugin)
            .add(crate::ready::ScriptReadyPlugin)
            .add(crate::assets::ScriptAssetsPlugin)
    }
}

/// Adds the shipped set after Bevy's plugins: bevy-ts on V8 (`dev` for `bevy-ts dev`), then the
/// feature crates, and `identity` as the engine's.
pub fn add_shipped(app: &mut App, identity: EngineIdentity, options: V8Options, dev: bool) {
    app.add_plugins(BevyTsPlugin::new(bevy_ts_v8::factory(options)).dev(dev))
        .add_plugins(ScriptPlugins)
        .add_plugins(bevy_ts_material::Pixel2dPlugin)
        .add_plugins(bevy_ts_material::BitmapTextPlugin)
        .add_plugins(bevy_ts_levels::engine::LevelsPlugin)
        .add_systems(PostUpdate, warn_mesh_handlers.run_if(not(resource_exists::<MeshPickingSettings>)))
        .insert_resource(identity);
}

/// After the project's plugins: each switch on adds its Bevy plugin (the one place each is added,
/// so dev and play cannot differ) unless a native crate added it with its own settings, then the
/// `diagnostics` plugins. `assets` is where levels are read and written.
pub fn finish_shipped(app: &mut App, switches: Switches, diagnostics: &[String], assets: Option<&Path>) {
    if switches.mesh_picking && !app.is_plugin_added::<MeshPickingPlugin>() {
        app.add_plugins(MeshPickingPlugin);
    }
    if switches.tab_navigation && !app.is_plugin_added::<TabNavigationPlugin>() {
        app.add_plugins(TabNavigationPlugin);
    }
    if switches.directional_navigation && !app.is_plugin_added::<DirectionalNavigationPlugin>() {
        app.add_plugins(DirectionalNavigationPlugin);
    }
    crate::diagnostics::add_plugins(app, diagnostics);
    // Checked once the plugins are in, so a native crate that added it is not warned.
    if !app.is_plugin_added::<TabNavigationPlugin>() {
        app.add_systems(PostUpdate, warn_tab_props);
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

/// Names, once per process, an entity whose `tabIndex` or `tabGroup` prop mounted while nothing
/// moves focus by Tab. Keys on the prop's marker, not `TabIndex`: a `<tab>` writes its own.
fn warn_tab_props(
    added: Query<(Entity, Option<&Name>), Added<bevy_ts_tsx::ui::TabProp>>,
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
    let message = format!("entity {e}{name} has a tabIndex or tabGroup, but tab navigation is off: Tab does not move focus. Set \"tabNavigation\": true in bevy-ts.json");
    log.push(bevy_ts::log::LogLevel::Warn, message, None, generations.map_or(0, |g| g.live));
}
