//! A level as a picture: composed as loading it would, then spawned as a detached tree of what
//! draws, for the asset viewer's thumbnail. Nothing a TS observer, a TS query, physics or the
//! reconciler reacts to is ever inserted, so the game never sees it.

use crate::engine::compose_level;
use bevy::prelude::*;
use bevy_ts::backend::ScriptError;
use bevy_ts::components::TsComponents;
use bevy_ts::layout::NativeLayouts;
use bevy_ts_tsx::desc::NodeDesc;
use bevy_ts_tsx::resolve::{Resolved, Target, resolve_node};

/// Reflect-tier components a picture keeps, by short type name: what draws, and nothing else.
const DRAWS: &[&str] = &["Visibility", "Mesh3d", "MeshMaterial3d", "PointLight", "SpotLight", "DirectionalLight", "WorldAssetRoot", "SceneRoot"];

fn short_name(type_path: &str) -> &str {
    let base = type_path.split('<').next().unwrap_or(type_path);
    base.rsplit("::").next().unwrap_or(base)
}

/// Composes the level at `path` and spawns its nodes under one root, keeping `Transform`,
/// `Visibility`, mesh, material and script-material intrinsics, `Mesh3d`/`MeshMaterial3d`, lights and glTF scene
/// roots. Every entity is spawned with `marker` (which should hold `ScriptHidden`) before any
/// other component. Returns the root, or the compose errors; nothing is logged.
pub fn spawn_level_picture(world: &mut World, path: &str, marker: impl Bundle + Clone) -> Result<Entity, Vec<ScriptError>> {
    let (composed, _) = compose_level(world, path);
    let composed = composed?;
    let resolved = resolve(world, &composed.nodes).map_err(|e| vec![e])?;
    let root = world.spawn(marker.clone()).id();
    world.entity_mut(root).insert((Transform::default(), Visibility::default()));
    let transform = world.components().component_id::<Transform>();
    spawn(world, &resolved, root, transform, &marker);
    Ok(root)
}

struct Node {
    comps: Vec<Resolved>,
    children: Vec<Node>,
}

fn resolve(world: &World, nodes: &[NodeDesc]) -> Result<Vec<Node>, ScriptError> {
    let ts = world.resource::<TsComponents>();
    let native = world.resource::<NativeLayouts>();
    let registry = world.resource::<AppTypeRegistry>().read();
    nodes
        .iter()
        .filter(|n| n.slot.is_none())
        .map(|n| Ok(Node { comps: resolve_node(n, ts, native, &registry)?, children: resolve(world, &n.children)? }))
        .collect()
}

fn spawn(world: &mut World, nodes: &[Node], parent: Entity, transform: Option<bevy::ecs::component::ComponentId>, marker: &(impl Bundle + Clone)) {
    for n in nodes {
        let e = world.spawn((marker.clone(), ChildOf(parent))).id();
        // Every node carries visibility, so a child's is inherited through a node that only moves.
        world.entity_mut(e).insert((Transform::default(), Visibility::default()));
        for r in &n.comps {
            match &r.target {
                Target::Native { id, .. } if Some(*id) == transform => bevy_ts_tsx::reconcile::insert(world, e, r),
                Target::Mesh(_) | Target::Material(_) | Target::ScriptMaterial { .. } => bevy_ts_tsx::reconcile::insert(world, e, r),
                Target::Reflect { type_path } if DRAWS.contains(&short_name(type_path)) => {
                    let props = r.controlled.clone().into_iter().collect();
                    // A component that does not read is left out: the game's log stays the game's.
                    if let Err(m) = bevy_ts_tsx::reflected::insert(world, e, type_path, &props) {
                        debug!("level picture: {type_path}: {m}");
                    }
                }
                _ => {}
            }
        }
        spawn(world, &n.children, e, transform, marker);
    }
}

#[cfg(test)]
mod tests {
    use super::short_name;

    #[test]
    fn short_names_drop_paths_and_generics() {
        assert_eq!(short_name("bevy_pbr::mesh_material::MeshMaterial3d<bevy_pbr::pbr_material::StandardMaterial>"), "MeshMaterial3d");
        assert_eq!(short_name("bevy_light::point_light::PointLight"), "PointLight");
        assert_eq!(short_name("Visibility"), "Visibility");
    }
}
