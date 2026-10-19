//! The keyed diff. A root's new description is matched to what was last applied by key path,
//! never compared with the world, and only the differences are written: spawns, despawns,
//! inserts and removes of components tsx owns, and controlled fields whose declared value
//! changed. Simulation state (fields systems wrote, entities systems despawned, components
//! systems added) is never touched.

use crate::desc::{NodeDesc, node_id};
use crate::intrinsics::{self, MeshProps};
use crate::resolve::{self, NAME, Resolved, Target};
use crate::tree::{FieldState, OwnedComponent, Provenance, ReconcileStats, Tombstone, TsxNode, TsxTree};
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
pub struct RNode {
    pub id: u64,
    pub path: Vec<String>,
    pub types: Vec<String>,
    pub source: Vec<SourceLocation>,
    pub comps: Arc<[Resolved]>,
    pub children: Vec<RNode>,
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
    let r = resolve_with(root, nodes, prefix, world.resource::<TsComponents>(), world.resource::<NativeLayouts>(), &mut cache);
    world.insert_resource(cache);
    r
}

fn resolve_with(
    root: &str,
    nodes: &[NodeDesc],
    (path, types): (&[String], &[String]),
    ts: &TsComponents,
    native: &NativeLayouts,
    cache: &mut TemplateCache,
) -> Result<Vec<RNode>, ScriptError> {
    nodes
        .iter()
        .map(|n| {
            let mut p = path.to_vec();
            p.push(n.key.clone());
            let mut t = types.to_vec();
            t.push(n.ty.clone());
            let key = content_hash(n);
            let comps = match cache.templates.get(&key) {
                Some(c) => {
                    cache.hits += 1;
                    c.clone()
                }
                None => {
                    cache.misses += 1;
                    let c: Arc<[Resolved]> = resolve::resolve_node(n, ts, native)?.into();
                    cache.templates.insert(key, c.clone());
                    c
                }
            };
            let children = resolve_with(root, &n.children, (&p, &t), ts, native, cache)?;
            Ok(RNode { id: node_id(root, &p), path: p, types: t, source: n.source.clone(), comps, children })
        })
        .collect()
}

fn alive(world: &World, e: Entity, id: u64) -> bool {
    world.get::<TsxNode>(e).is_some_and(|n| n.id == id)
}

/// Applies resolved nodes under `root`. With `prune`, nodes of the root missing from `nodes`
/// are despawned (a generation's reconcile); without, the mount is additive (runtime mounts).
/// `parent` is the entity the top-level nodes are children of.
pub fn apply_root(
    world: &mut World,
    root: &str,
    location: Option<SourceLocation>,
    nodes: &[RNode],
    parent: Option<Entity>,
    prune: bool,
) -> ReconcileStats {
    let mut stats = ReconcileStats::default();
    let mut tree = world.remove_resource::<TsxTree>().unwrap_or_default();
    let mut seen = BTreeSet::new();
    for n in nodes {
        apply_node(world, &mut tree, root, n, parent, &mut seen, &mut stats);
    }
    let state = tree.roots.entry(root.to_string()).or_default();
    if location.is_some() {
        state.location = location;
    }
    if prune {
        let gone: Vec<u64> = state.nodes.difference(&seen).copied().collect();
        state.nodes = seen;
        for id in gone {
            if let Some(e) = tree.entities.remove(&id)
                && alive(world, e, id)
            {
                // Descendants go with it, including entities systems parented under it.
                world.despawn(e);
                stats.despawned += 1;
            }
            tree.tombstones.remove(&id);
        }
    } else {
        state.nodes.extend(seen);
    }
    tree.last = stats.clone();
    tree.reconciles += 1;
    world.insert_resource(tree);
    stats
}

/// Despawns every node of a root and forgets it.
pub fn unmount_root(world: &mut World, root: &str) -> ReconcileStats {
    let mut stats = ReconcileStats::default();
    let mut tree = world.remove_resource::<TsxTree>().unwrap_or_default();
    if let Some(state) = tree.roots.remove(root) {
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
    if alive(world, e, id) {
        world.despawn(e);
        removed = true;
    }
    for i in std::iter::once(id).chain(ids) {
        tree.entities.remove(&i);
        tree.tombstones.remove(&i);
        for r in tree.roots.values_mut() {
            r.nodes.remove(&i);
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
        tree.tombstones
            .entry(n.id)
            .or_insert_with(|| Tombstone { root: root.to_string(), key_path: n.path.clone(), source: n.source.clone() });
    }
    for c in &n.children {
        tombstone_subtree(world, tree, root, c, seen, stats);
    }
}

fn apply_node(
    world: &mut World,
    tree: &mut TsxTree,
    root: &str,
    n: &RNode,
    parent: Option<Entity>,
    seen: &mut BTreeSet<u64>,
    stats: &mut ReconcileStats,
) {
    seen.insert(n.id);
    let known = tree.entities.get(&n.id).copied();
    let entity = match known {
        _ if tree.tombstones.contains_key(&n.id) => None,
        Some(e) if alive(world, e, n.id) => {
            update(world, e, n, stats);
            Some(e)
        }
        // Despawned by something other than tsx: a system killed it. It stays dead.
        Some(_) => None,
        None => Some(spawn(world, root, n, parent, stats)),
    };
    match entity {
        Some(e) => {
            tree.entities.insert(n.id, e);
            for c in &n.children {
                apply_node(world, tree, root, c, Some(e), seen, stats);
            }
        }
        None => tombstone_subtree(world, tree, root, n, seen, stats),
    }
}

fn owned(r: &Resolved) -> OwnedComponent {
    let mut fields = BTreeMap::new();
    for (f, v) in &r.controlled {
        fields.insert(f.clone(), FieldState { provenance: Provenance::Controlled, applied: v.clone() });
    }
    for (f, v) in &r.initial {
        fields.insert(f.clone(), FieldState { provenance: Provenance::Initial, applied: v.clone() });
    }
    for (f, v) in &r.defaults {
        fields.insert(f.clone(), FieldState { provenance: Provenance::Default, applied: v.clone() });
    }
    OwnedComponent { fields, source: r.source.clone() }
}

fn insert(world: &mut World, e: Entity, r: &Resolved) {
    match &r.target {
        Target::Native { id, .. } => {
            let bytes = r.insert_bytes().expect("native target");
            // SAFETY: `id` was registered with the layout `bytes` was encoded with.
            unsafe { bevy_ts::commands::insert_bytes(&mut world.entity_mut(e), *id, &bytes) };
        }
        Target::Name => {
            let Some(Value::String(s)) = r.controlled.get("value") else { unreachable!("checked") };
            world.entity_mut(e).insert(Name::new(s.clone()));
        }
        Target::Mesh(p) => intrinsics::insert_mesh(world, e, p),
        Target::Material(p) => intrinsics::insert_material(world, e, p),
    }
}

fn spawn(world: &mut World, root: &str, n: &RNode, parent: Option<Entity>, stats: &mut ReconcileStats) -> Entity {
    let e = world.spawn_empty().id();
    if let Some(p) = parent {
        world.entity_mut(e).insert(ChildOf(p));
    }
    let mut components = BTreeMap::new();
    for r in n.comps.iter() {
        insert(world, e, r);
        components.insert(r.key.clone(), owned(r));
    }
    world.entity_mut(e).insert(TsxNode {
        id: n.id,
        root: root.to_string(),
        key_path: n.path.clone(),
        types: n.types.clone(),
        source: n.source.clone(),
        components,
    });
    stats.spawned += 1;
    e
}

/// Writes one field of a native-layout component in place, marking it changed.
fn write_field(world: &mut World, e: Entity, id: bevy::ecs::component::ComponentId, layout: &ComponentLayout, f: &str, v: &Value) -> bool {
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
        name => {
            let id = world
                .resource::<TsComponents>()
                .get(name)
                .map(|c| c.id)
                .or_else(|| world.resource::<NativeLayouts>().get(name).map(|n| n.id));
            if let Some(id) = id {
                world.entity_mut(e).remove_by_id(id);
            }
        }
    }
}

fn update(world: &mut World, e: Entity, n: &RNode, stats: &mut ReconcileStats) {
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
        let mut state = OwnedComponent { fields: BTreeMap::new(), source: r.source.clone() };
        match &r.target {
            Target::Native { id, layout, .. } => {
                for (f, v) in &r.controlled {
                    let same = prev.fields.get(f).is_some_and(|s| s.provenance == Provenance::Controlled && &s.applied == v);
                    // A system may have removed the component; tsx does not put it back.
                    if !same && write_field(world, e, *id, layout, f, v) {
                        stats.written += 1;
                    }
                    state.fields.insert(f.clone(), FieldState { provenance: Provenance::Controlled, applied: v.clone() });
                }
                // Initial and default fields are never written to an existing component; they
                // keep the value written on insert.
                for (f, v, p) in r
                    .initial
                    .iter()
                    .map(|(f, v)| (f, v, Provenance::Initial))
                    .chain(r.defaults.iter().map(|(f, v)| (f, v, Provenance::Default)))
                {
                    let applied = prev.fields.get(f).map_or_else(|| v.clone(), |s| s.applied.clone());
                    state.fields.insert(f.clone(), FieldState { provenance: p, applied });
                }
            }
            target => {
                // Name and asset intrinsics: their props as a whole, rebuilt when they change.
                if props_of(prev) != r.controlled.clone().into_iter().collect::<Map<_, _>>() {
                    if let Target::Mesh(_) = target {
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
    // Only a real difference touches `TsxNode`, so an identical reload moves no change tick.
    if node != old {
        *world.get_mut::<TsxNode>(e).expect("alive") = node;
    }
}
