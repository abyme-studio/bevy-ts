//! Runtime mounts: `ctx.commands.mount(element, { parent })` and `ctx.commands.unmount(target)`
//! from a TS system or handler, applied in order with that run's other commands. Mounted nodes
//! live under the runtime root and survive reloads like scene nodes, until unmounted or
//! despawned.

use crate::desc::{NodeDesc, node_id, normalize_sources};
use crate::reconcile::{apply_root, resolve_tree, unmount_node};
use crate::resolve;
use crate::tree::{RUNTIME_ROOT, TsxNode};
use bevy::prelude::*;
use bevy_ts::components::TsComponents;
use bevy_ts::layout::NativeLayouts;
use serde::Deserialize;

/// The `tsx/mount` command kind.
pub const MOUNT: &str = "tsx/mount";
/// The `tsx/unmount` command kind.
pub const UNMOUNT: &str = "tsx/unmount";

#[derive(Deserialize)]
struct MountOp {
    nodes: Vec<NodeDesc>,
    /// `Entity::to_bits` of the parent, if any.
    #[serde(default)]
    parent: Option<u64>,
    /// `Entity::to_bits` of an existing entity the one top-level node lands on: `spawn(<el/>)`
    /// (spawned by the op just before) and `entity(e).insert(<el/>)`.
    #[serde(default)]
    entity: Option<u64>,
}

#[derive(Deserialize)]
#[serde(untagged)]
enum UnmountOp {
    /// A key given at mount (top level of the runtime root, no parent).
    Key { key: String },
    Entity { entity: u64 },
}

/// Mounts a description under the runtime root. Checked whole before anything is written.
pub fn mount(world: &mut World, bytes: &[u8]) -> Result<(), String> {
    let mut op: MountOp = serde_json::from_slice(bytes).map_err(|e| format!("bad mount payload: {e}"))?;
    normalize_sources(&mut op.nodes);
    let at = match op.entity {
        Some(bits) => {
            let e = Entity::try_from_bits(bits).ok_or_else(|| format!("mount: {bits} is not an entity"))?;
            world.get_entity(e).map_err(|_| format!("insert: entity {e} does not exist"))?;
            if world.get::<TsxNode>(e).is_some() {
                return Err(format!("insert: entity {e} was made by tsx; mount the element under it instead"));
            }
            if op.nodes.len() != 1 || op.nodes[0].slot.is_some() {
                return Err("an element spawned or inserted on an entity has one root <entity>".into());
            }
            Some(e)
        }
        None => None,
    };
    let parent = match op.parent {
        Some(bits) => {
            let p = Entity::try_from_bits(bits).ok_or_else(|| format!("mount: {bits} is not an entity"))?;
            world.get_entity(p).map_err(|_| format!("mount: parent {p} does not exist"))?;
            Some(p)
        }
        None => None,
    };
    let mut errors = Vec::new();
    let registry = world.resource::<AppTypeRegistry>().clone();
    resolve::check_nodes(&op.nodes, world.resource::<TsComponents>(), world.resource::<NativeLayouts>(), &registry.read(), &mut errors);
    if let Some(e) = errors.into_iter().next() {
        return Err(e.to_string());
    }
    if !crate::intrinsics::available(world) && crate::uses_assets(&op.nodes).is_some() {
        return Err("<mesh> and <material> need the render plugins (no Assets<Mesh>)".into());
    }
    if !world.contains_resource::<AssetServer>() {
        let mut errors = Vec::new();
        crate::textures_without_server(&op.nodes, &mut errors);
        if let Some(e) = errors.into_iter().next() {
            return Err(e.to_string());
        }
    }
    // Under a parent, keys are scoped to it, so two parents may each mount an `e9`; on an
    // entity, to that entity.
    let (prefix, types): (Vec<String>, Vec<String>) = match (at, parent) {
        (Some(e), _) => (vec![format!("={}", e.to_bits())], vec!["entity".into()]),
        (None, Some(p)) => (vec![format!("@{}", p.to_bits())], vec!["parent".into()]),
        (None, None) => (vec![], vec![]),
    };
    let nodes = resolve_tree(world, RUNTIME_ROOT, &op.nodes, (&prefix, &types)).map_err(|e| e.to_string())?;
    if let Some(e) = at {
        // The root node is `e`: a node with nothing applied yet, so the reconcile inserts every
        // component on it and spawns the children under it.
        let n = &nodes[0];
        world.entity_mut(e).insert(TsxNode {
            id: n.id,
            root: RUNTIME_ROOT.to_string(),
            key_path: n.path.clone(),
            types: n.types.clone(),
            source: n.source.clone(),
            components: Default::default(),
        });
        let mut tree = world.resource_mut::<crate::TsxTree>();
        tree.entities.insert(n.id, e);
        tree.journal.open.added.push(n.id);
    }
    apply_root(world, RUNTIME_ROOT, None, &nodes, if at.is_some() { None } else { parent }, crate::reconcile::Prune::None);
    world.resource_mut::<crate::TsxTree>().journal.close();
    Ok(())
}

/// Unmounts a runtime node by its key, or any tsx node by its entity.
pub fn unmount(world: &mut World, bytes: &[u8]) -> Result<(), String> {
    let op: UnmountOp = serde_json::from_slice(bytes).map_err(|e| format!("bad unmount payload: {e}"))?;
    let id = match op {
        UnmountOp::Key { key } => node_id(RUNTIME_ROOT, &[key]),
        UnmountOp::Entity { entity } => {
            let e = Entity::try_from_bits(entity).ok_or_else(|| format!("unmount: {entity} is not an entity"))?;
            world.get::<TsxNode>(e).ok_or_else(|| format!("unmount: {e} was not mounted by tsx"))?.id
        }
    };
    let removed = unmount_node(world, id);
    let mut tree = world.resource_mut::<crate::TsxTree>();
    tree.journal.open.removed.push(id);
    tree.journal.close();
    if removed { Ok(()) } else { Err("unmount: no such mounted node".into()) }
}
