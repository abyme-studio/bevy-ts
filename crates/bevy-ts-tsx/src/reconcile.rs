//! The keyed diff. A root's new description is matched to what was last applied by key path,
//! never compared with the world, and only the differences are written: spawns, despawns,
//! inserts and removes of components tsx owns, and controlled fields whose declared value
//! changed. Simulation state (fields systems wrote, entities systems despawned, components
//! systems added) is never touched.

use crate::desc::{NodeDesc, node_id};
use crate::intrinsics::{self, Mesh2dProps, MeshProps};
use crate::resolve::{self, NAME, Resolved, Target};
use crate::tree::{BindTarget, FieldState, Later, OwnedComponent, Provenance, ReconcileStats, SlotState, Tombstone, TsxNode, TsxTree};
use crate::ui;
use bevy::prelude::*;
use bevy_ts::backend::{ScriptError, SourceLocation};
use bevy_ts::components::TsComponents;
use bevy_ts::layout::{ComponentLayout, NativeLayouts};
use bevy_ts::value;
use serde_json::{Map, Value};
use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::hash::{Hash, Hasher};
use std::sync::Arc;

/// A description node resolved against the live components, with its identity.
#[derive(Clone, Debug)]
pub struct RNode {
    pub id: u64,
    pub path: Vec<String>,
    pub types: Vec<String>,
    pub source: Vec<SourceLocation>,
    pub comps: Arc<[Resolved]>,
    pub children: Vec<RNode>,
    pub ref_id: Option<u32>,
    pub on: BTreeMap<String, u32>,
    /// A `<Show>`/`<For>` slot: no entity; JS mounts its content under this node's key path.
    pub slot: Option<u32>,
}

/// Resolved component elements by the content of an entity's elements (its name and component
/// elements with their props and locations, not its key or call site), so the second identical
/// `<Enemy>` reuses what the first resolved and encoded. Cleared when a generation applies,
/// because ids and layouts may change with it.
#[derive(Resource, Default)]
pub struct TemplateCache {
    templates: HashMap<u64, Arc<[Resolved]>>,
    pub hits: u64,
    pub misses: u64,
}

impl TemplateCache {
    pub fn clear(&mut self) {
        self.templates.clear();
    }

    pub fn len(&self) -> usize {
        self.templates.len()
    }

    pub fn is_empty(&self) -> bool {
        self.templates.is_empty()
    }
}

fn content_hash(n: &NodeDesc) -> u64 {
    let mut h = std::hash::DefaultHasher::new();
    serde_json::to_vec(&(&n.name, &n.components)).expect("serializable").hash(&mut h);
    h.finish()
}

/// Resolves a description subtree under `prefix` (the parent's key path and types).
pub fn resolve_tree(
    world: &mut World,
    root: &str,
    nodes: &[NodeDesc],
    prefix: (&[String], &[String]),
) -> Result<Vec<RNode>, ScriptError> {
    let mut cache = world.remove_resource::<TemplateCache>().unwrap_or_default();
    let registry = world.resource::<AppTypeRegistry>().clone();
    let r = resolve_with(root, nodes, prefix, world.resource::<TsComponents>(), world.resource::<NativeLayouts>(), &registry.read(), &mut cache);
    world.insert_resource(cache);
    r
}

fn resolve_with(
    root: &str,
    nodes: &[NodeDesc],
    (path, types): (&[String], &[String]),
    ts: &TsComponents,
    native: &NativeLayouts,
    registry: &bevy::reflect::TypeRegistry,
    cache: &mut TemplateCache,
) -> Result<Vec<RNode>, ScriptError> {
    nodes
        .iter()
        .map(|n| {
            let mut p = path.to_vec();
            p.push(n.key.clone());
            let mut t = types.to_vec();
            t.push(n.ty.clone());
            if n.slot.is_some() {
                let id = node_id(root, &p);
                return Ok(RNode { id, path: p, types: t, source: n.source.clone(), comps: Arc::new([]), children: vec![], ref_id: None, on: BTreeMap::new(), slot: n.slot });
            }
            let key = content_hash(n);
            let comps = match cache.templates.get(&key) {
                Some(c) => {
                    cache.hits += 1;
                    c.clone()
                }
                None => {
                    cache.misses += 1;
                    let c: Arc<[Resolved]> = resolve::resolve_node(n, ts, native, registry)?.into();
                    cache.templates.insert(key, c.clone());
                    c
                }
            };
            let children = resolve_with(root, &n.children, (&p, &t), ts, native, registry, cache)?;
            Ok(RNode { id: node_id(root, &p), path: p, types: t, source: n.source.clone(), comps, children, ref_id: n.ref_id, on: n.on.clone(), slot: None })
        })
        .collect()
}

fn alive(world: &World, e: Entity, id: u64) -> bool {
    world.get::<TsxNode>(e).is_some_and(|n| n.id == id)
}

/// What a reconcile may despawn: nothing it was not given (runtime mounts), every node of the
/// root it was not given (a generation), or a slot's groups: `drop`
/// (unmounted groups) despawned before the nodes apply, and those of `stale` (remounted groups'
/// old nodes) the nodes did not describe again.
#[derive(Clone, Debug, PartialEq)]
pub enum Prune {
    None,
    Root,
    Groups { drop: Vec<u64>, stale: Vec<u64> },
}

fn key_path(world: &World, tree: &TsxTree, id: u64) -> Option<Vec<String>> {
    let e = tree.entities.get(&id)?;
    world.get::<TsxNode>(*e).filter(|n| n.id == id).map(|n| n.key_path.clone())
}

/// Applies resolved nodes under `root`, with `parent` the entity top-level nodes are children
/// of, despawning what `prune` allows.
pub fn apply_root(
    world: &mut World,
    root: &str,
    location: Option<SourceLocation>,
    nodes: &[RNode],
    parent: Option<Entity>,
    prune: Prune,
) -> ReconcileStats {
    let mut stats = ReconcileStats::default();
    let mut tree = world.remove_resource::<TsxTree>().unwrap_or_default();
    let mut seen = BTreeSet::new();
    let mut slots = Vec::new();
    // Unmounted groups go first, so a group unmounted and mounted again in one call is respawned.
    if let Prune::Groups { drop, .. } = &prune {
        stats.checked += drop.len();
        for id in drop {
            forget(world, &mut tree, *id, &mut stats);
        }
    }
    let added = tree.journal.open.added.len();
    for n in nodes {
        apply_node(world, &mut tree, root, n, parent, &mut seen, &mut stats, &mut slots);
    }
    let state = tree.roots.entry(root.to_string()).or_default();
    // Applied since the game started: a Stop cannot trust what the start recorded.
    state.baseline = None;
    // Sibling lists whose order changed, by the key path of one of their children.
    let mut lists = Vec::new();
    record_order(&mut state.order, nodes, &mut lists);
    if location.is_some() {
        state.location = location;
    }
    let candidates: Vec<u64> = match &prune {
        Prune::None => vec![],
        Prune::Root => state.nodes.difference(&seen).copied().collect(),
        Prune::Groups { stale, .. } => {
            stats.checked += stale.len();
            stale.iter().filter(|id| !seen.contains(id)).copied().collect()
        }
    };
    state.nodes.extend(seen);
    tree.moved.retain(|_, r| r != root);
    let new: Vec<u64> = tree.journal.open.added[added..].to_vec();
    lists.extend(new.into_iter().filter_map(|id| key_path(world, &tree, id)));
    // Many new nodes share one sibling list: it is recorded once.
    let mut recorded = std::collections::HashSet::new();
    for path in lists {
        let list = (root.to_string(), tree.parent_row(root, &path));
        if recorded.insert(list.clone()) {
            tree.journal.open.ordered.push(list);
        }
    }
    if let Prune::Groups { .. } = &prune {
        // A slot's stale nodes are its own: no key path to check.
        for id in candidates {
            forget(world, &mut tree, id, &mut stats);
        }
        tree.last = stats.clone();
        tree.reconciles += 1;
        world.insert_resource(tree);
        return stats;
    }
    for id in candidates {
        let path = key_path(world, &tree, id);
        // Slot content is the slot's to keep or drop, after JS mounts it again.
        let in_slot = path.as_ref().is_some_and(|p| slots.iter().any(|s: &Vec<String>| p.starts_with(s)));
        if in_slot {
            continue;
        }
        forget(world, &mut tree, id, &mut stats);
    }
    tree.last = stats.clone();
    tree.reconciles += 1;
    world.insert_resource(tree);
    stats
}

/// How [`apply_at`] applies each subtree it is given.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AtMode {
    /// The live subtree despawns and the given one spawns in its place.
    Respawn,
    /// The given subtree is diffed against the live one, as a whole-root reconcile diffs it.
    Reconcile,
}

/// The node at `path` in a resolved tree.
pub fn find_node<'a>(nodes: &'a [RNode], path: &[String]) -> Option<&'a RNode> {
    let (first, rest) = path.split_first()?;
    let n = nodes.iter().find(|n| n.path.last() == Some(first))?;
    if rest.is_empty() { Some(n) } else { find_node(&n.children, rest) }
}

/// Every node of a resolved tree by id, so a partial apply finds each of its paths at once.
pub fn index(nodes: &[RNode]) -> HashMap<u64, &RNode> {
    fn walk<'a>(nodes: &'a [RNode], out: &mut HashMap<u64, &'a RNode>) {
        for n in nodes {
            out.insert(n.id, n);
            walk(&n.children, out);
        }
    }
    let mut out = HashMap::new();
    walk(nodes, &mut out);
    out
}

/// Replaces (or with `None` removes) the node at `path` in a resolved tree; a new one goes last
/// among its siblings.
fn replace_node(nodes: &mut Vec<RNode>, path: &[String], with: Option<RNode>) {
    let Some((first, rest)) = path.split_first() else { return };
    let at = nodes.iter().position(|n| n.path.last() == Some(first));
    match (at, rest.is_empty(), with) {
        (Some(i), true, Some(n)) => nodes[i] = n,
        (Some(i), true, None) => {
            nodes.remove(i);
        }
        (None, true, Some(n)) => nodes.push(n),
        (Some(i), false, with) => replace_node(&mut nodes[i].children, rest, with),
        _ => {}
    }
}

/// Applies `root` at the given key paths only, each from its resolved subtree (`None`
/// despawns it); nodes outside them are not written. A respawned subtree goes under its described
/// parent in description order, its tombstones cleared. The root's state, its kept tree and the
/// watch journal follow, as [`apply_root`] keeps them. Paths are disjoint: none under another.
pub fn apply_at(world: &mut World, root: &str, items: &[(Vec<String>, Option<&RNode>)], mode: AtMode) -> ReconcileStats {
    let mut stats = ReconcileStats::default();
    let mut tree = world.remove_resource::<TsxTree>().unwrap_or_default();
    let kept = tree.roots.get(root).and_then(|r| r.resolved.clone());
    let described = kept.as_deref().map(index).unwrap_or_default();
    let mut patched: Option<Vec<RNode>> = None;
    let mut slots = Vec::new();
    for (path, node) in items {
        let Some(id) = (!path.is_empty()).then(|| node_id(root, path)) else { continue };
        let parent = match path.len() {
            1 => None,
            n => match tree.entities.get(&node_id(root, &path[..n - 1])).copied().filter(|e| world.get_entity(*e).is_ok()) {
                Some(e) => Some(e),
                // Its parent is gone or tombstoned: nowhere to put it.
                None => continue,
            },
        };
        let old = rows_at(world, &tree, described.get(&id).copied(), root, path, id);
        if mode == AtMode::Respawn {
            clear(world, &mut tree, root, path, &old, &mut stats);
        }
        let mut seen = BTreeSet::new();
        let added = tree.journal.open.added.len();
        if let Some(n) = node {
            apply_node(world, &mut tree, root, n, parent, &mut seen, &mut stats, &mut slots);
        }
        let state = tree.roots.entry(root.to_string()).or_default();
        state.nodes.extend(seen.iter().copied());
        // The node keeps its index among its siblings; its subtree is recorded again.
        if let Some(n) = node {
            record_order(&mut state.order, &n.children, &mut Vec::new());
        }
        let gone: Vec<u64> = old.iter().filter(|i| !seen.contains(i)).copied().collect();
        for i in gone {
            match mode {
                AtMode::Reconcile => forget(world, &mut tree, i, &mut stats),
                AtMode::Respawn => {
                    for r in tree.roots.values_mut() {
                        r.order.remove(&i);
                    }
                }
            }
        }
        // The kept tree: unchanged when the node given is its own.
        if let Some(k) = &kept {
            let own = match (described.get(&id).copied().filter(|_| patched.is_none()).or_else(|| patched.as_ref().map_or_else(|| find_node(k, path), |p| find_node(p, path))), node) {
                (Some(a), Some(b)) => std::ptr::eq(a, *b),
                (None, None) => true,
                _ => false,
            };
            if !own {
                replace_node(patched.get_or_insert_with(|| k.to_vec()), path, node.map(|n| (*n).clone()));
            }
        }
        if tree.journal.open.added.len() > added || mode == AtMode::Respawn {
            let list = (root.to_string(), tree.parent_row(root, path));
            tree.journal.open.ordered.push(list);
        }
        if let Some(p) = parent {
            let pid = node_id(root, &path[..path.len() - 1]);
            let d = match &patched {
                Some(p) => find_node(p, &path[..path.len() - 1]),
                None => described.get(&pid).copied(),
            };
            if let Some(d) = d {
                order_children(world, &tree, p, d);
            }
        }
    }
    if let (Some(p), Some(state)) = (patched, tree.roots.get_mut(root)) {
        state.resolved = Some(p.into());
    }
    tree.last = stats.clone();
    tree.reconciles += 1;
    tree.journal.close();
    world.insert_resource(tree);
    stats
}

/// The rows of `root` at and under `path`: the node, its live descendants, its tombstones, and
/// the kept tree's nodes there that are still mapped (a despawned subtree's).
fn rows_at(world: &World, tree: &TsxTree, kept: Option<&RNode>, root: &str, path: &[String], id: u64) -> Vec<u64> {
    fn ids(n: &RNode, out: &mut Vec<u64>) {
        out.push(n.id);
        for c in &n.children {
            ids(c, out);
        }
    }
    let e = tree.entities.get(&id).copied().filter(|e| alive(world, *e, id));
    let mut out = vec![id];
    out.extend(subtree(world, tree, e, root, path));
    if let Some(n) = kept {
        let mut described = Vec::new();
        ids(n, &mut described);
        out.extend(described.into_iter().filter(|i| tree.entities.contains_key(i)));
    }
    out.sort_unstable();
    out.dedup();
    out.retain(|i| tree.is_row(root, *i) || tree.entities.contains_key(i));
    out
}

/// Despawns the live subtree at `path` and forgets its rows, so it spawns again as if first.
fn clear(world: &mut World, tree: &mut TsxTree, root: &str, path: &[String], rows: &[u64], stats: &mut ReconcileStats) {
    let id = node_id(root, path);
    if let Some(e) = tree.entities.get(&id).copied().filter(|e| alive(world, *e, id)) {
        world.despawn(e);
        stats.despawned += 1;
    }
    tree.reactive.stale = true;
    for i in rows {
        if tree.is_row(root, *i) {
            tree.journal.open.removed.push(*i);
        }
        tree.entities.remove(i);
        tree.tombstones.remove(i);
        if let Some(r) = tree.roots.get_mut(root) {
            r.nodes.remove(i);
        }
    }
}

/// The root and key path of a row: a live node's, or a tombstone's.
fn row_path(world: &World, tree: &TsxTree, id: u64) -> Option<(String, Vec<String>)> {
    let live = tree.entities.get(&id).and_then(|e| world.get::<TsxNode>(*e)).filter(|n| n.id == id).map(|n| (n.root.clone(), n.key_path.clone()));
    live.or_else(|| tree.tombstones.get(&id).map(|t| (t.root.clone(), t.key_path.clone())))
}

/// The rows of `root` under `path`: tsx nodes among `e`'s descendants, and tombstones.
fn subtree(world: &World, tree: &TsxTree, e: Option<Entity>, root: &str, path: &[String]) -> Vec<u64> {
    let under = |r: &str, p: &[String]| r == root && p.len() > path.len() && p.starts_with(path);
    let mut ids: Vec<u64> = e
        .map(|e| descendants(world, e))
        .unwrap_or_default()
        .into_iter()
        .filter_map(|d| world.get::<TsxNode>(d))
        .filter(|n| under(&n.root, &n.key_path))
        .map(|n| n.id)
        .collect();
    ids.extend(tree.tombstones.iter().filter(|(_, t)| under(&t.root, &t.key_path)).map(|(id, _)| *id));
    ids
}

/// Despawns a node (descendants go with it, including entities systems parented under it) and
/// forgets it and every row under it. Its sibling list is recorded as changed.
fn forget(world: &mut World, tree: &mut TsxTree, id: u64, stats: &mut ReconcileStats) {
    if !tree.roots.values().any(|r| r.nodes.contains(&id)) && !tree.tombstones.contains_key(&id) {
        return;
    }
    tree.reactive.stale = true;
    let e = tree.entities.get(&id).copied().filter(|e| alive(world, *e, id));
    let mut ids = vec![id];
    if let Some((root, path)) = row_path(world, tree, id) {
        ids.extend(subtree(world, tree, e, &root, &path));
        let list = (root.clone(), tree.parent_row(&root, &path));
        tree.journal.open.ordered.push(list);
    }
    if let Some(e) = e {
        world.despawn(e);
        stats.despawned += 1;
    }
    for i in ids {
        tree.journal.open.removed.push(i);
        tree.entities.remove(&i);
        tree.tombstones.remove(&i);
        for r in tree.roots.values_mut() {
            r.nodes.remove(&i);
            r.order.remove(&i);
        }
    }
}

/// Records each described node's index among its siblings, down the whole description. A
/// sibling list whose known nodes are now in another order is pushed to `changed`, by the key
/// path of one of its children.
fn record_order(order: &mut HashMap<u64, u32>, nodes: &[RNode], changed: &mut Vec<Vec<String>>) {
    let old: Vec<u32> = nodes.iter().filter_map(|n| order.get(&n.id).copied()).collect();
    if old.windows(2).any(|w| w[0] > w[1]) {
        changed.push(nodes[0].path.clone());
    }
    for (i, n) in nodes.iter().enumerate() {
        order.insert(n.id, i as u32);
        record_order(order, &n.children, changed);
    }
}

/// Despawns every node of `root` under `prefix` whose next key is not in `keep`.
pub fn sweep(world: &mut World, root: &str, prefix: &[String], keep: &BTreeSet<String>) -> ReconcileStats {
    let mut stats = ReconcileStats::default();
    let mut tree = world.remove_resource::<TsxTree>().unwrap_or_default();
    let ids: Vec<u64> = tree.roots.get(root).map(|r| r.nodes.iter().copied().collect()).unwrap_or_default();
    for id in ids {
        let Some(p) = key_path(world, &tree, id) else {
            if tree.entities.contains_key(&id) {
                tree.entities.remove(&id);
            }
            continue;
        };
        if p.len() > prefix.len() && p.starts_with(prefix) && !keep.contains(&p[prefix.len()]) {
            forget(world, &mut tree, id, &mut stats);
        }
    }
    world.insert_resource(tree);
    stats
}

/// Despawns every node of a root and forgets it.
pub fn unmount_root(world: &mut World, root: &str) -> ReconcileStats {
    let mut stats = ReconcileStats::default();
    let mut tree = world.remove_resource::<TsxTree>().unwrap_or_default();
    if let Some(state) = tree.roots.remove(root) {
        tree.journal.open.removed.extend(&state.nodes);
        // Its tombstones stay (dead until a restart) but leave the tree with it.
        let dead: Vec<u64> = tree.tombstones.iter().filter(|(id, t)| t.root == root && !state.nodes.contains(id)).map(|(id, _)| *id).collect();
        tree.journal.open.removed.extend(dead);
        for id in state.nodes {
            if let Some(e) = tree.entities.remove(&id)
                && alive(world, e, id)
            {
                world.despawn(e);
                stats.despawned += 1;
            }
            tree.tombstones.remove(&id);
        }
    }
    tree.last = stats.clone();
    world.insert_resource(tree);
    stats
}

/// Despawns one node (and its descendants) by id, wherever it is mounted.
pub fn unmount_node(world: &mut World, id: u64) -> bool {
    let mut tree = world.remove_resource::<TsxTree>().unwrap_or_default();
    let mut removed = false;
    let Some(e) = tree.entities.get(&id).copied() else {
        world.insert_resource(tree);
        return false;
    };
    let ids: Vec<u64> = descendants(world, e).into_iter().filter_map(|d| world.get::<TsxNode>(d).map(|n| n.id)).collect();
    if let Some((root, path)) = row_path(world, &tree, id) {
        let list = (root.clone(), tree.parent_row(&root, &path));
        tree.journal.open.ordered.push(list);
    }
    if alive(world, e, id) {
        world.despawn(e);
        removed = true;
    }
    for i in std::iter::once(id).chain(ids) {
        tree.journal.open.removed.push(i);
        tree.entities.remove(&i);
        tree.tombstones.remove(&i);
        for r in tree.roots.values_mut() {
            r.nodes.remove(&i);
            r.order.remove(&i);
        }
    }
    world.insert_resource(tree);
    removed
}

fn descendants(world: &World, e: Entity) -> Vec<Entity> {
    let mut out = Vec::new();
    let mut stack = vec![e];
    while let Some(x) = stack.pop() {
        if let Some(children) = world.get::<Children>(x) {
            for c in children.iter() {
                out.push(c);
                stack.push(c);
            }
        }
    }
    out
}

fn tombstone_subtree(world: &World, tree: &mut TsxTree, root: &str, n: &RNode, seen: &mut BTreeSet<u64>, stats: &mut ReconcileStats) {
    seen.insert(n.id);
    let dead = tree.entities.get(&n.id).is_some_and(|e| !alive(world, *e, n.id));
    if dead || tree.tombstones.contains_key(&n.id) {
        if tree.entities.remove(&n.id).is_some() {
            stats.tombstoned += 1;
        }
        if !tree.tombstones.contains_key(&n.id) {
            tree.journal.open.tombstoned.push(n.id);
        }
        tree.tombstones
            .entry(n.id)
            .or_insert_with(|| Tombstone { root: root.to_string(), key_path: n.path.clone(), source: n.source.clone() });
    }
    for c in &n.children {
        tombstone_subtree(world, tree, root, c, seen, stats);
    }
}

#[expect(clippy::too_many_arguments, reason = "the walk's state, threaded down")]
fn apply_node(
    world: &mut World,
    tree: &mut TsxTree,
    root: &str,
    n: &RNode,
    parent: Option<Entity>,
    seen: &mut BTreeSet<u64>,
    stats: &mut ReconcileStats,
    slots: &mut Vec<Vec<String>>,
) {
    if let Some(slot) = n.slot {
        // A generation clears slots first; a reconcile between generations (an overlay) keeps
        // the groups the slot has mounted.
        tree.reactive.slots.entry(slot).or_insert_with(|| SlotState {
            root: root.to_string(),
            parent,
            prefix: n.path.clone(),
            types: n.types.clone(),
            groups: BTreeSet::new(),
            rows: HashMap::new(),
            later: Vec::new(),
        });
        tree.reactive.due = true;
        slots.push(n.path.clone());
        return;
    }
    seen.insert(n.id);
    for r in n.comps.iter() {
        for (prop, id) in &r.bound {
            tree.reactive.targets.insert(*id, BindTarget { node: n.id, component: r.key.clone(), prop: prop.clone() });
            tree.reactive.due = true;
        }
    }
    if let Some(r) = n.ref_id {
        tree.reactive.refs.insert(r, n.id);
        tree.reactive.new_refs.push(r);
        tree.reactive.due = true;
    }
    let moved = tree.moved.remove(&n.id).is_some();
    let known = tree.entities.get(&n.id).copied();
    let entity = match known {
        _ if tree.tombstones.contains_key(&n.id) => None,
        Some(e) if alive(world, e, n.id) => {
            if moved {
                reparent(world, e, root, parent);
            }
            let before = stats.clone();
            if update(world, e, n, stats) {
                tree.journal.open.moved.push(n.id);
            }
            if stats.written + stats.inserted + stats.removed != before.written + before.inserted + before.removed {
                tree.journal.open.written.push(n.id);
            }
            Some(e)
        }
        // Despawned by something other than tsx: a system killed it. It stays dead.
        Some(_) => None,
        None => {
            tree.journal.open.added.push(n.id);
            Some(spawn(world, root, n, parent, stats))
        }
    };
    match entity {
        Some(e) => {
            tree.entities.insert(n.id, e);
            crate::events::set(world, e, &n.on);
            for r in n.comps.iter() {
                for (prop, site) in &r.sites {
                    let id = r.bound[prop];
                    tree.reactive.sites.entry(*site).or_default().instances.insert(e, id);
                    tree.reactive.new_instances.push((id, e));
                }
            }
            let reorder = n.children.iter().any(|c| tree.moved.contains_key(&c.id));
            for (i, c) in n.children.iter().enumerate() {
                apply_node(world, tree, root, c, Some(e), seen, stats, slots);
                if let Some(s) = c.slot.and_then(|s| tree.reactive.slots.get_mut(&s)) {
                    s.later = n.children[i + 1..].iter().map(|c| c.slot.map_or(Later::Node(c.id), Later::Slot)).collect();
                }
            }
            if reorder {
                order_children(world, tree, e, n);
            }
        }
        None => tombstone_subtree(world, tree, root, n, seen, stats),
    }
}

fn owned(r: &Resolved) -> OwnedComponent {
    let mut fields = BTreeMap::new();
    for (f, v) in &r.controlled {
        fields.insert(f.clone(), FieldState { provenance: r.controlled_provenance(f), applied: v.clone() });
    }
    for (f, v) in &r.reflected {
        fields.insert(f.clone(), FieldState { provenance: r.controlled_provenance(f), applied: v.clone() });
    }
    for (f, v) in &r.initial {
        fields.insert(f.clone(), FieldState { provenance: Provenance::Initial, applied: v.clone() });
    }
    for (f, v) in &r.defaults {
        fields.insert(f.clone(), FieldState { provenance: Provenance::Default, applied: v.clone() });
    }
    for f in r.bound.keys() {
        fields.insert(f.clone(), FieldState { provenance: Provenance::Bound, applied: Value::Null });
    }
    OwnedComponent { fields, source: r.source.clone(), overrides: r.overrides.clone() }
}

pub fn insert(world: &mut World, e: Entity, r: &Resolved) {
    match &r.target {
        Target::Native { id, type_path, .. } => {
            let bytes = r.insert_bytes().expect("native target");
            // SAFETY: `id` was registered with the layout `bytes` was encoded with.
            unsafe { bevy_ts::commands::insert_bytes(&mut world.entity_mut(e), *id, &bytes) };
            holes(world, e, r, type_path.as_deref());
        }
        Target::Name => {
            let Some(Value::String(s)) = r.controlled.get("value") else { unreachable!("checked") };
            world.entity_mut(e).insert(Name::new(s.clone()));
        }
        Target::Reflect { type_path } => {
            let props: Map<String, Value> = r.initial.clone().into_iter().chain(r.controlled.clone()).collect();
            if let Err(m) = crate::reflected::insert(world, e, type_path, &props) {
                log_error(world, m, r.source.clone());
            }
        }
        Target::StateScoped { kind } => {
            let v = &r.controlled["value"];
            let (state, value) = (v["state"].as_str().unwrap_or(""), v["value"].as_str().unwrap_or(""));
            if let Err(m) = bevy_ts::states::insert_scoped(world, e, *kind, state, value) {
                log_error(world, m, r.source.clone());
            }
        }
        Target::Mesh(p) => intrinsics::insert_mesh(world, e, p),
        Target::Material(p) => intrinsics::insert_material(world, e, p, r.source.clone()),
        Target::Mesh2d(p) => intrinsics::insert_mesh2d(world, e, p),
        Target::Material2d(p) => intrinsics::insert_material2d(world, e, p, r.source.clone()),
        Target::ScriptMaterial { kind, props, gltf_material } => intrinsics::insert_script_material(world, e, kind, props, gltf_material.as_deref(), r.source.clone()),
        Target::Ui(kind) => {
            let props: Map<String, Value> = r.controlled.clone().into_iter().collect();
            if let Err(m) = ui::insert(world, e, *kind, &props) {
                log_error(world, format!("<{}> {m}", r.key), r.source.clone());
            }
        }
    }
}

/// A native component's holes given as props, written over its bytes through reflection.
fn holes(world: &mut World, e: Entity, r: &Resolved, type_path: Option<&str>) {
    let Some(type_path) = type_path else { return };
    for (f, v) in &r.reflected {
        if let Err(m) = crate::reflected::write(world, e, type_path, f, v) {
            log_error(world, m, r.source.clone());
        }
    }
}

/// Logs an error found while applying (bound values are checked only when they arrive).
pub fn log_error(world: &mut World, message: String, location: Option<SourceLocation>) {
    let generation = bevy_ts::runtime::current_generation(world);
    world.resource_mut::<bevy_ts::log::ScriptLog>().push(bevy_ts::log::LogLevel::Error, message, location, generation);
}

/// Applied values of a component's fields, leaving out bound ones not delivered yet.
fn applied(c: &OwnedComponent) -> Map<String, Value> {
    c.fields.iter().filter(|(_, s)| !s.applied.is_null()).map(|(k, s)| (k.clone(), s.applied.clone())).collect()
}

/// A node a rekey moved (a level's `reparent`) follows its described parent. A root node leaves
/// only a parent of its own root, not one something else gave it. Nodes nothing moved keep
/// whatever parent game code gave them.
fn reparent(world: &mut World, e: Entity, root: &str, parent: Option<Entity>) {
    let current = world.get::<ChildOf>(e).map(ChildOf::parent);
    if current == parent {
        return;
    }
    match parent {
        Some(p) => {
            world.entity_mut(e).insert(ChildOf(p));
        }
        None if current.and_then(|c| world.get::<TsxNode>(c)).is_some_and(|n| n.root == root) => {
            world.entity_mut(e).remove::<ChildOf>();
        }
        None => {}
    }
}

/// Puts `e`'s children in description order, after a moved child arrived or moved among them.
/// Children tsx did not make keep their order, after the described ones.
fn order_children(world: &mut World, tree: &TsxTree, e: Entity, n: &RNode) {
    let order: HashMap<Entity, usize> = n.children.iter().enumerate().filter_map(|(i, c)| Some((*tree.entities.get(&c.id)?, i))).collect();
    if let Some(mut children) = world.get_mut::<Children>(e) {
        children.sort_by_key(|c| order.get(c).copied().unwrap_or(usize::MAX));
    }
}

/// Puts a slot's group entities in `tops` order among `parent`'s children, leaving the other
/// children where they are, and records the order for `tsx_tree`.
pub fn order_slot(world: &mut World, root: &str, parent: Option<Entity>, prefix: &[String], tops: &[u64]) {
    let mut tree = world.remove_resource::<TsxTree>().unwrap_or_default();
    let rank: HashMap<Entity, usize> = tops
        .iter()
        .filter_map(|id| tree.entities.get(id).copied().filter(|e| alive(world, *e, *id)))
        .enumerate()
        .map(|(i, e)| (e, i))
        .collect();
    if let Some(state) = tree.roots.get_mut(root) {
        for (i, id) in tops.iter().enumerate() {
            state.order.insert(*id, i as u32);
        }
    }
    if let Some(p) = parent
        && let Some(mut children) = world.get_mut::<Children>(p)
    {
        let mut sorted: Vec<Entity> = children.iter().filter(|c| rank.contains_key(c)).collect();
        sorted.sort_by_key(|e| rank[e]);
        let mut next = sorted.into_iter();
        // Each slot entity's place goes to the next in order; every other child keeps its own.
        let place: HashMap<Entity, usize> =
            children.iter().enumerate().map(|(i, c)| (if rank.contains_key(&c) { next.next().expect("as many") } else { c }, i)).collect();
        children.sort_by_key(|c| place[c]);
    }
    let list = (root.to_string(), tree.parent_row(root, prefix));
    tree.journal.open.ordered.push(list);
    world.insert_resource(tree);
}

/// Moves the entities of `later` (a slot's siblings described after it, a later slot's rows
/// included) to the end of `parent`'s children, keeping their order, so rows the slot just
/// spawned sit before them.
pub fn keep_after(world: &mut World, parent: Entity, later: &[Later]) {
    let tree = world.resource::<TsxTree>();
    let entity = |id: &u64| tree.entities.get(id).copied().filter(|e| alive(world, *e, *id));
    let mut after = std::collections::HashSet::new();
    for l in later {
        match l {
            Later::Node(id) => after.extend(entity(id)),
            Later::Slot(s) => {
                let Some(slot) = tree.reactive.slots.get(s) else { continue };
                after.extend(slot.rows.values().flat_map(|r| r.tops.iter()).filter_map(entity));
            }
        }
    }
    if after.is_empty() {
        return;
    }
    let Some(mut children) = world.get_mut::<Children>(parent) else { return };
    let n = children.len();
    let place: HashMap<Entity, usize> = children.iter().enumerate().map(|(i, c)| (c, if after.contains(&c) { n + i } else { i })).collect();
    children.sort_by_key(|c| place[c]);
}

/// Spawns a node's entity. Its `ChildOf`, `TsxNode` and native-layout components go in with one
/// archetype move; the rest (reflection, assets, UI) after.
fn spawn(world: &mut World, root: &str, n: &RNode, parent: Option<Entity>, stats: &mut ReconcileStats) -> Entity {
    let mut components = BTreeMap::new();
    // By id: a component given twice keeps its last value, as inserting it twice would.
    let mut native: BTreeMap<bevy::ecs::component::ComponentId, Vec<u8>> = BTreeMap::new();
    let mut after = Vec::new();
    for r in n.comps.iter() {
        components.insert(r.key.clone(), owned(r));
        match &r.target {
            Target::Native { id, .. } => {
                native.insert(*id, r.insert_bytes().expect("native target"));
                after.push(r);
            }
            _ => after.push(r),
        }
    }
    let node = TsxNode { id: n.id, root: root.to_string(), key_path: n.path.clone(), types: n.types.clone(), source: n.source.clone(), components };
    let ids = (world.register_component::<TsxNode>(), world.register_component::<ChildOf>());
    let parts: Vec<_> = native.iter().map(|(id, b)| (*id, b.as_slice())).collect();
    let mut em = world.spawn_empty();
    let e = em.id();
    bevy::ptr::OwningPtr::make(node, |node| match parent {
        // SAFETY: each pointer holds a value of its id's type, and each native id was registered
        // with the layout its bytes were encoded with.
        Some(p) => bevy::ptr::OwningPtr::make(ChildOf(p), |child_of| unsafe {
            bevy_ts::commands::insert_bytes_many(&mut em, vec![(ids.0, node), (ids.1, child_of)], &parts)
        }),
        None => unsafe { bevy_ts::commands::insert_bytes_many(&mut em, vec![(ids.0, node)], &parts) },
    });
    for r in after {
        match &r.target {
            Target::Native { type_path, .. } => holes(world, e, r, type_path.as_deref()),
            _ => insert(world, e, r),
        }
    }
    stats.spawned += 1;
    e
}

/// A bound field keeps whatever its binding last wrote; the reconciler never writes it.
fn bound_fields(r: &Resolved, prev: &OwnedComponent, state: &mut OwnedComponent) {
    for f in r.bound.keys() {
        let applied = prev.fields.get(f).map_or(Value::Null, |s| s.applied.clone());
        state.fields.insert(f.clone(), FieldState { provenance: Provenance::Bound, applied });
    }
}

/// Writes one field of a native-layout component in place, marking it changed.
pub fn write_field(world: &mut World, e: Entity, id: bevy::ecs::component::ComponentId, layout: &ComponentLayout, f: &str, v: &Value) -> bool {
    let fl = layout.field(f).expect("checked field");
    let mut bytes = vec![0u8; fl.ty.size()];
    value::encode_field(f, &fl.ty, v, &mut bytes).expect("checked value");
    let mut em = world.entity_mut(e);
    let Ok(mut m) = em.get_mut_by_id(id) else { return false };
    // SAFETY: the component has `layout`, so the field's bytes lie inside it.
    unsafe { std::ptr::copy_nonoverlapping(bytes.as_ptr(), m.as_mut().as_ptr().add(fl.offset), bytes.len()) };
    true
}

fn props_of(c: &OwnedComponent) -> Map<String, Value> {
    c.fields.iter().map(|(k, s)| (k.clone(), s.applied.clone())).collect()
}

fn remove(world: &mut World, e: Entity, key: &str, prev: &OwnedComponent) {
    match key {
        NAME => {
            world.entity_mut(e).remove::<Name>();
        }
        resolve::MESH => {
            if let Ok(p) = serde_json::from_value::<MeshProps>(Value::Object(props_of(prev))) {
                intrinsics::remove_mesh(world, e, &p);
            }
        }
        resolve::MATERIAL => intrinsics::remove_material(world, e),
        resolve::MESH2D => {
            if let Ok(p) = serde_json::from_value::<Mesh2dProps>(Value::Object(props_of(prev))) {
                intrinsics::remove_mesh2d(world, e, &p);
            }
        }
        resolve::MATERIAL2D => intrinsics::remove_material2d(world, e),
        tag if ui::UiKind::from_tag(tag).is_some() => ui::remove(world, e, ui::UiKind::from_tag(tag).expect("checked")),
        scoped if bevy_ts::states::scoped_kind(scoped).is_some() => {
            bevy_ts::states::remove_scoped(world, e, bevy_ts::states::scoped_kind(scoped).expect("checked"));
        }
        name => {
            let id = world
                .resource::<TsComponents>()
                .get(name)
                .map(|c| c.id)
                .or_else(|| world.resource::<NativeLayouts>().get(name).map(|n| n.id))
                .or_else(|| world.resource::<NativeLayouts>().reflect_tier(name).map(|t| t.id));
            if let Some(id) = id {
                world.entity_mut(e).remove_by_id(id);
            }
        }
    }
}

/// Returns whether the node's source chain moved.
fn update(world: &mut World, e: Entity, n: &RNode, stats: &mut ReconcileStats) -> bool {
    let old = world.get::<TsxNode>(e).expect("alive").clone();
    let mut components = BTreeMap::new();
    for r in n.comps.iter() {
        let Some(prev) = old.components.get(&r.key) else {
            // A new component element on an existing entity: inserted with every value.
            insert(world, e, r);
            stats.inserted += 1;
            components.insert(r.key.clone(), owned(r));
            continue;
        };
        let mut state = OwnedComponent { fields: BTreeMap::new(), source: r.source.clone(), overrides: r.overrides.clone() };
        match &r.target {
            Target::Native { id, layout, type_path, .. } => {
                // Holes given as props: written through reflection when their value changed.
                for (f, v) in &r.reflected {
                    let provenance = r.controlled_provenance(f);
                    let same = prev.fields.get(f).is_some_and(|s| s.provenance == provenance && &s.applied == v);
                    if !same && let Some(type_path) = type_path {
                        match crate::reflected::write(world, e, type_path, f, v) {
                            Ok(true) => stats.written += 1,
                            Ok(false) => {}
                            Err(m) => log_error(world, m, r.source.clone()),
                        }
                    }
                    state.fields.insert(f.clone(), FieldState { provenance, applied: v.clone() });
                }
                for (f, v) in &r.controlled {
                    // A field whose owner changed (an override added or removed) is written.
                    let provenance = r.controlled_provenance(f);
                    let same = prev.fields.get(f).is_some_and(|s| s.provenance == provenance && &s.applied == v);
                    // A system may have removed the component; tsx does not put it back.
                    if !same && write_field(world, e, *id, layout, f, v) {
                        stats.written += 1;
                    }
                    state.fields.insert(f.clone(), FieldState { provenance, applied: v.clone() });
                }
                // Initial and default fields are never written to an existing component; they
                // keep the value written on insert.
                for (f, v, p) in r
                    .initial
                    .iter()
                    .map(|(f, v)| (f, v, Provenance::Initial))
                    .chain(r.defaults.iter().map(|(f, v)| (f, v, Provenance::Default)))
                {
                    // A level override removed (levels rule 4): the field takes its default again.
                    if p == Provenance::Default && prev.fields.get(f).is_some_and(|s| s.provenance == Provenance::Override) {
                        if write_field(world, e, *id, layout, f, v) {
                            stats.written += 1;
                        }
                        state.fields.insert(f.clone(), FieldState { provenance: p, applied: v.clone() });
                        continue;
                    }
                    let applied = prev.fields.get(f).map_or_else(|| v.clone(), |s| s.applied.clone());
                    state.fields.insert(f.clone(), FieldState { provenance: p, applied });
                }
                bound_fields(r, prev, &mut state);
            }
            Target::Reflect { type_path } => {
                for (f, v) in &r.controlled {
                    let provenance = r.controlled_provenance(f);
                    let same = prev.fields.get(f).is_some_and(|s| s.provenance == provenance && &s.applied == v);
                    if !same {
                        match crate::reflected::write(world, e, type_path, f, v) {
                            Ok(true) => stats.written += 1,
                            Ok(false) => {}
                            Err(m) => log_error(world, m, r.source.clone()),
                        }
                    }
                    state.fields.insert(f.clone(), FieldState { provenance, applied: v.clone() });
                }
                // Initial fields are written on insert only; they keep that value.
                for (f, v) in &r.initial {
                    let applied = prev.fields.get(f).map_or_else(|| v.clone(), |s| s.applied.clone());
                    state.fields.insert(f.clone(), FieldState { provenance: Provenance::Initial, applied });
                }
                bound_fields(r, prev, &mut state);
            }
            Target::Ui(kind) => {
                // Prop by prop: only the components behind changed props are written.
                let old_props = applied(prev);
                for (f, v) in &r.controlled {
                    state.fields.insert(f.clone(), FieldState { provenance: Provenance::Controlled, applied: v.clone() });
                }
                bound_fields(r, prev, &mut state);
                let new_props = applied(&state);
                match ui::update(world, e, *kind, &old_props, &new_props) {
                    Ok(n) => stats.written += n,
                    Err(m) => log_error(world, format!("<{}> {m}", r.key), r.source.clone()),
                }
            }
            target => {
                // Name and asset intrinsics: their props as a whole, rebuilt when they change.
                if props_of(prev) != r.controlled.clone().into_iter().collect::<Map<_, _>>() {
                    // A scoped component may move to another machine's slot: its old type goes first.
                    if matches!(target, Target::Mesh(_) | Target::Mesh2d(_) | Target::StateScoped { .. }) {
                        remove(world, e, &r.key, prev);
                    }
                    insert(world, e, r);
                    stats.written += 1;
                }
                state = owned(r);
            }
        }
        components.insert(r.key.clone(), state);
    }
    for (key, prev) in &old.components {
        if !components.contains_key(key) {
            remove(world, e, key, prev);
            stats.removed += 1;
        }
    }
    let node = TsxNode { source: n.source.clone(), types: n.types.clone(), components, ..old.clone() };
    let moved = node.source != old.source;
    // Only a real difference touches `TsxNode`, so an identical reload moves no change tick.
    if node != old {
        *world.get_mut::<TsxNode>(e).expect("alive") = node;
    }
    moved
}
