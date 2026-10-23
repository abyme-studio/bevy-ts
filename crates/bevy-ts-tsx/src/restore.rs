//! What a Stop needs from tsx to restore a root instead of mounting it again: the root as the
//! game started (its baseline), the nodes the play changed since, and the plan the rebuild keeps.

use crate::reconcile::RNode;
use crate::tree::{TsxNode, TsxTree};
use bevy::ecs::archetype::ArchetypeId;
use bevy::ecs::change_detection::Tick;
use bevy::prelude::*;
use std::collections::{BTreeMap, HashMap};

/// An external root as the game started: the world's change tick then, and each node's entity
/// and archetype.
#[derive(Clone, Debug)]
pub struct Baseline {
    pub tick: Tick,
    pub nodes: HashMap<u64, (Entity, ArchetypeId)>,
}

/// The roots a Stop keeps, each with the key paths it respawns: the subtrees the play changed and
/// the nodes it despawned. Inserted for the Stop's rebuild, which keeps these roots, and removed
/// once they are restored.
#[derive(Resource, Default, Debug, Clone)]
pub struct StopPlan {
    pub roots: BTreeMap<String, Vec<Vec<String>>>,
}

/// The start handler: records a baseline for every external root applied now. Ticks after it
/// are the game's.
pub fn take_baselines(world: &mut World) {
    let tick = world.increment_change_tick();
    let baselines: Vec<(String, Baseline)> = {
        let tree = world.resource::<TsxTree>();
        tree.external
            .iter()
            .filter_map(|name| {
                let state = tree.roots.get(name).filter(|s| s.resolved.is_some())?;
                let nodes = state
                    .nodes
                    .iter()
                    .filter_map(|id| {
                        let e = tree.entity(*id)?;
                        let er = world.get_entity(e).ok().filter(|er| er.get::<TsxNode>().is_some_and(|n| n.id == *id))?;
                        Some((*id, (e, er.archetype().id())))
                    })
                    .collect();
                Some((name.clone(), Baseline { tick, nodes }))
            })
            .collect()
    };
    let mut tree = world.resource_mut::<TsxTree>();
    for (name, b) in baselines {
        if let Some(s) = tree.roots.get_mut(&name) {
            s.baseline = Some(b);
        }
    }
}

/// Whether the root has a baseline and a kept tree: a Stop can restore it.
pub fn has_baseline(world: &World, root: &str) -> bool {
    world.resource::<TsxTree>().roots.get(root).is_some_and(|s| s.baseline.is_some() && s.resolved.is_some())
}

/// The key paths of the root's highest changed nodes since its baseline: a node whose entity is
/// gone, has another archetype (a component added or removed), or has a component added or
/// changed after the baseline's tick. Below a changed node nothing is listed: its subtree goes
/// whole. None without a baseline.
/// The [`derived`] components do not count.
pub fn changed_paths(world: &World, root: &str) -> Option<Vec<Vec<String>>> {
    let tree = world.resource::<TsxTree>();
    let state = tree.roots.get(root)?;
    let (b, kept) = (state.baseline.as_ref()?, state.resolved.as_ref()?);
    let now = world.read_change_tick();
    let derived = derived(world);
    let changed = |id: u64| {
        let Some((e, arch)) = b.nodes.get(&id) else { return true };
        let Ok(er) = world.get_entity(*e) else { return true };
        if er.get::<TsxNode>().is_none_or(|n| n.id != id) || er.archetype().id() != *arch {
            return true;
        }
        er.archetype().iter_components().filter(|c| !derived.contains(c)).any(|c| er.get_change_ticks_by_id(c).is_some_and(|t| t.is_changed(b.tick, now)))
    };
    fn walk(nodes: &[RNode], changed: &dyn Fn(u64) -> bool, out: &mut Vec<Vec<String>>) {
        for n in nodes.iter().filter(|n| n.slot.is_none()) {
            if changed(n.id) {
                out.push(n.path.clone());
            } else {
                walk(&n.children, changed, out);
            }
        }
    }
    let mut out = Vec::new();
    walk(kept, &changed, &mut out);
    Some(out)
}

/// Components Bevy derives every frame from other state: they change with nothing of the game
/// touched, and a respawn cannot restore them (the next frame computes them again), so the scan
/// leaves them out. Each was seen to change in a play that touched nothing, under rendering and
/// Play's camera swap (stop-incremental, task 6.3). `Camera` changes every frame too
/// (`camera_system` takes `&mut` of its viewport) but holds the game's settings, so it counts:
/// a level camera respawns at each Stop.
pub fn derived(world: &World) -> Vec<bevy::ecs::component::ComponentId> {
    let c = world.components();
    [
        // `CheckVisibility`'s output from the cameras: switching from the editor's camera to the
        // game's flips it on every entity one sees and the other does not.
        c.component_id::<bevy::camera::visibility::ViewVisibility>(),
        // A camera's visible list, cleared and filled each frame.
        c.component_id::<bevy::camera::visibility::VisibleEntities>(),
        // A camera's light clusters, assigned each frame.
        c.component_id::<bevy::light::cluster::Clusters>(),
        // A camera's last view matrices, copied each frame for the prepass.
        c.component_id::<bevy::core_pipeline::prepass::PreviousViewData>(),
    ]
    .into_iter()
    .flatten()
    .collect()
}

/// Whether anything under the root holds a JS callback: a binding, a handler, a ref or a slot.
/// Each holds an id into the isolate a Stop throws away.
pub fn has_callbacks(world: &World, root: &str) -> bool {
    fn any(nodes: &[RNode]) -> bool {
        nodes.iter().any(|n| n.slot.is_some() || n.ref_id.is_some() || !n.on.is_empty() || n.comps.iter().any(|c| !c.bound.is_empty()) || any(&n.children))
    }
    world.resource::<TsxTree>().roots.get(root).and_then(|s| s.resolved.as_deref()).is_some_and(any)
}

/// The key paths of the root's tombstones.
pub fn tombstoned_paths(world: &World, root: &str) -> Vec<Vec<String>> {
    world.resource::<TsxTree>().tombstones.values().filter(|t| t.root == root).map(|t| t.key_path.clone()).collect()
}

/// Keeps only the paths no other path is above: the subtrees to respawn, whole.
pub fn highest(mut paths: Vec<Vec<String>>) -> Vec<Vec<String>> {
    paths.sort();
    paths.dedup();
    let mut out: Vec<Vec<String>> = Vec::new();
    for p in paths {
        // Sorted, so an ancestor comes right before its descendants.
        if !out.last().is_some_and(|a| p.starts_with(a)) {
            out.push(p);
        }
    }
    out
}

/// Respawns each of a kept root's paths from its kept tree.
pub fn restore_root(world: &mut World, root: &str, paths: &[Vec<String>]) -> crate::tree::ReconcileStats {
    let Some(kept) = world.resource::<TsxTree>().roots.get(root).and_then(|s| s.resolved.clone()) else { return Default::default() };
    let paths = highest(paths.to_vec());
    let by_id = crate::reconcile::index(&kept);
    let items: Vec<(Vec<String>, Option<&RNode>)> = paths.iter().map(|p| (p.clone(), by_id.get(&crate::desc::node_id(root, p)).copied())).collect();
    crate::reconcile::apply_at(world, root, &items, crate::reconcile::AtMode::Respawn)
}
