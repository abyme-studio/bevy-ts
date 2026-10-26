//! What a Stop needs from tsx to restore a root instead of mounting it again: the root as the
//! game started (its baseline), the nodes the play changed since, and the plan the rebuild keeps.

use crate::reconcile::RNode;
use crate::tree::{TsxNode, TsxTree};
use bevy::ecs::archetype::ArchetypeId;
use bevy::ecs::change_detection::Tick;
use bevy::ecs::component::ComponentId;
use bevy::ecs::world::EntityRef;
use bevy::prelude::*;
use std::collections::{BTreeMap, HashMap};

/// An external root as the game started: the world's change tick then, and each node.
#[derive(Clone, Debug)]
pub struct Baseline {
    pub tick: Tick,
    pub nodes: HashMap<u64, NodeBase>,
}

/// A node as the game started: its entity and archetype, and those of the entities below it that
/// are not nodes (a glTF model's hierarchy, a child a plugin spawned), down to the next node.
#[derive(Clone, Debug)]
pub struct NodeBase {
    pub entity: Entity,
    pub archetype: ArchetypeId,
    pub below: Vec<(Entity, ArchetypeId)>,
}

/// The entities below `e` that are not tsx nodes, with their archetypes, stopping at each node.
fn below(world: &World, e: Entity) -> Vec<(Entity, ArchetypeId)> {
    let mut out = Vec::new();
    let mut stack = vec![e];
    while let Some(x) = stack.pop() {
        let Some(children) = world.get::<Children>(x) else { continue };
        for c in children.iter() {
            let Ok(er) = world.get_entity(c) else { continue };
            if er.contains::<TsxNode>() {
                continue;
            }
            out.push((c, er.archetype().id()));
            stack.push(c);
        }
    }
    out
}

/// The roots a Stop keeps, each with the key paths it respawns (the subtrees the play changed and
/// the nodes it despawned) and those it writes back. Inserted for the Stop's rebuild, which keeps
/// these roots, and removed once they are restored.
#[derive(Resource, Default, Debug, Clone)]
pub struct StopPlan {
    pub roots: BTreeMap<String, Vec<Vec<String>>>,
    /// Per kept root, the nodes to write back in place rather than respawn ([`write_back`]).
    pub write: BTreeMap<String, Vec<Vec<String>>>,
}

impl StopPlan {
    /// The roots a Stop under way keeps; none without one.
    pub fn kept(world: &World) -> std::collections::BTreeSet<String> {
        world.get_resource::<StopPlan>().map(|p| p.roots.keys().cloned().collect()).unwrap_or_default()
    }
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
                        Some((*id, NodeBase { entity: e, archetype: er.archetype().id(), below: below(world, e) }))
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

/// What the play changed under a root since its baseline, by key path.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Changes {
    /// Nodes to respawn, each with its subtree: the entity is gone or has another archetype (a
    /// component added or removed), or an entity below it that is not a node changed.
    pub respawn: Vec<Vec<String>>,
    /// Nodes whose own components changed, archetype and parts as they were: candidates for
    /// [`write_back`]. Their children are listed on their own.
    pub write: Vec<Vec<String>>,
}

/// What the play changed under the root since its baseline, a node at a time: a node whose
/// entity is gone or has another archetype, or one of whose parts (the entities below it that
/// are not nodes) is gone, has another archetype or has a component added or changed, respawns
/// with its subtree; a node with a component added or changed is written back. None without a
/// baseline. The [`derived`] and [`recomputed`] components do not count.
pub fn changes(world: &World, root: &str) -> Option<Changes> {
    let tree = world.resource::<TsxTree>();
    let state = tree.roots.get(root)?;
    let (b, kept) = (state.baseline.as_ref()?, state.resolved.as_ref()?);
    let now = world.read_change_tick();
    let derived = derived(world);
    let skip: Vec<_> = derived.iter().copied().chain(recomputed(world)).collect();
    let touched = |er: &EntityRef, skip: &[ComponentId]| er.archetype().iter_components().filter(|c| !skip.contains(c)).any(|c| er.get_change_ticks_by_id(c).is_some_and(|t| t.is_changed(b.tick, now)));
    let part_changed = |e: Entity, arch: ArchetypeId| world.get_entity(e).map_or(true, |er| er.archetype().id() != arch || touched(&er, &skip));
    let state = |id: u64| {
        let Some(n) = b.nodes.get(&id) else { return Change::Respawn };
        let Ok(er) = world.get_entity(n.entity) else { return Change::Respawn };
        if er.get::<TsxNode>().is_none_or(|t| t.id != id) || er.archetype().id() != n.archetype || n.below.iter().any(|(e, a)| part_changed(*e, *a)) {
            return Change::Respawn;
        }
        // The recomputed ones change on an untouched node too: a moved node's ancestors and descendants.
        if touched(&er, &skip) { Change::Write } else { Change::Same }
    };
    fn walk(nodes: &[RNode], state: &dyn Fn(u64) -> Change, out: &mut Changes) {
        for n in nodes.iter().filter(|n| n.slot.is_none()) {
            match state(n.id) {
                Change::Respawn => out.respawn.push(n.path.clone()),
                Change::Write => {
                    out.write.push(n.path.clone());
                    walk(&n.children, state, out);
                }
                Change::Same => walk(&n.children, state, out),
            }
        }
    }
    let mut out = Changes::default();
    walk(kept, &state, &mut out);
    Some(out)
}

enum Change {
    Same,
    Write,
    Respawn,
}

/// The key paths of the root's highest changed nodes since its baseline (see [`changes`]): below
/// a changed node nothing is listed, its subtree goes whole. None without a baseline.
pub fn changed_paths(world: &World, root: &str) -> Option<Vec<Vec<String>>> {
    let c = changes(world, root)?;
    Some(highest(c.respawn.into_iter().chain(c.write).collect()))
}

/// Components Bevy computes again in the next frame from a node's own components once they are
/// written, as after a spawn: a written-back node leaves them to it. Each has a test that Bevy
/// recomputes it (`tests/it/restore.rs`). Not `InheritedVisibility`: Bevy recomputes it only when
/// a `Visibility` or the hierarchy changes, not from what a write-back writes, so a node whose
/// `InheritedVisibility` changed respawns.
pub fn recomputed(world: &World) -> Vec<ComponentId> {
    let c = world.components();
    [
        // `propagate_parent_transforms`, from `Transform` down the hierarchy.
        c.component_id::<GlobalTransform>(),
        // `mark_dirty_trees`, set where `Transform` changed and on each ancestor.
        c.component_id::<bevy::transform::components::TransformTreeChanged>(),
    ]
    .into_iter()
    .flatten()
    .collect()
}

/// Writes each node at `paths` back from the kept tree: inserts every component tsx resolved
/// for it, the same values a spawn inserts, and leaves the [`recomputed`] ones to Bevy. Bevy's
/// replace and insert hooks fire for each. Returns the paths it could not restore so, to
/// respawn: no kept node or entity, or a component the play changed that tsx does not write
/// (one a plugin inserted), or another archetype after the write.
pub fn write_back(world: &mut World, root: &str, paths: &[Vec<String>]) -> Vec<Vec<String>> {
    if paths.is_empty() {
        return Vec::new();
    }
    let (kept, tick) = {
        let s = world.resource::<TsxTree>().roots.get(root);
        match s.and_then(|s| Some((s.resolved.clone()?, s.baseline.as_ref()?.tick))) {
            Some(k) => k,
            None => return paths.to_vec(),
        }
    };
    let by_id = crate::reconcile::index(&kept);
    let skip: Vec<_> = derived(world).into_iter().chain(recomputed(world)).collect();
    let mut failed = Vec::new();
    for p in paths {
        let id = crate::desc::node_id(root, p);
        let tree = world.resource::<TsxTree>();
        let (Some(n), Some(e)) = (by_id.get(&id), tree.entity(id)) else {
            failed.push(p.clone());
            continue;
        };
        let arch = tree.roots[root].baseline.as_ref().and_then(|b| b.nodes.get(&id)).map(|b| b.archetype);
        let before = world.increment_change_tick();
        for r in n.comps.iter() {
            crate::reconcile::insert(world, e, r);
        }
        let now = world.change_tick();
        let Ok(er) = world.get_entity(e) else {
            failed.push(p.clone());
            continue;
        };
        // Changed since the baseline means rewritten just now.
        let restored = Some(er.archetype().id()) == arch
            && er.archetype().iter_components().filter(|c| !skip.contains(c)).all(|c| er.get_change_ticks_by_id(c).is_none_or(|t| !t.is_changed(tick, now) || t.is_changed(before, now)));
        if !restored {
            failed.push(p.clone());
        }
    }
    failed
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
