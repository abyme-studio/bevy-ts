//! `bevy-ts/tsx_*`: what a scene editor needs from the reconciler. Map an entity to its TSX
//! element (`tsx_node`), a source location to every entity it made (`tsx_find`), see the whole
//! tree (`tsx_tree`), and follow what each reconcile changed (`tsx_tree+watch`). The editor, not
//! bevy-ts, parses and rewrites source at these locations.

use crate::desc::node_id;
use crate::tree::{Change, RootState, TsxNode, TsxTree};
use bevy::prelude::*;
use bevy::remote::{BrpError, BrpResult, error_codes};
use bevy_ts::runtime::ScriptFrame;
use serde::Deserialize;
use serde_json::{Map, Value, json};
use std::collections::{BTreeSet, HashMap};

/// The crate the contract lists these methods under.
pub const CRATE: &str = "bevy-ts-tsx";

/// Serves the `bevy-ts/tsx_*` methods; needs `RemotePlugin` (or `BevyTsRemotePlugin`) first.
pub fn add_methods(app: &mut App) {
    bevy_ts_remote::add_method(app, CRATE, "bevy-ts/tsx_tree", tree);
    bevy_ts_remote::add_method(app, CRATE, "bevy-ts/tsx_node", node);
    bevy_ts_remote::add_method(app, CRATE, "bevy-ts/tsx_find", find);
    bevy_ts_remote::add_watching_method(app, CRATE, "bevy-ts/tsx_tree+watch", watch);
    app.init_resource::<Watchers>();
}

fn invalid(message: impl Into<String>) -> BrpError {
    BrpError { code: error_codes::INVALID_PARAMS, message: message.into(), data: None }
}

/// A node id as the protocol writes it: 16 hex digits (a u64 does not fit a JS number).
pub fn id_str(id: u64) -> String {
    format!("{id:016x}")
}

fn parse_id(s: &str) -> Result<u64, BrpError> {
    u64::from_str_radix(s, 16).map_err(|_| invalid(format!("{s:?} is not a tsx node id")))
}

/// Fills `level` on `tsx_tree` rows: a plugin that places entities from files (levels) registers
/// it, and tsx knows nothing of what it reads. With it, each `app.scene` root also carries
/// `sidecar`, its sidecar level's path; without it, rows and roots carry neither.
#[derive(Resource, Clone, Copy)]
pub struct TreeProvenance(pub fn(&World, Entity, &TsxNode) -> Option<Value>);

fn live(world: &World, tree: &TsxTree, id: u64) -> Option<(Entity, TsxNode)> {
    let e = *tree.entities.get(&id)?;
    world.get::<TsxNode>(e).filter(|n| n.id == id).map(|n| (e, n.clone()))
}

fn parent(tree: &TsxTree, root: &str, path: &[String]) -> Value {
    json!(tree.parent_row(root, path).map(id_str))
}

fn tree_node(world: &World, tree: &TsxTree, id: u64) -> Option<Value> {
    if let Some((e, n)) = live(world, tree, id) {
        let mut row = json!({
            "node": id_str(id),
            "entity": e.to_bits(),
            "keyPath": n.key_path,
            "type": n.types.last(),
            "source": n.source.first(),
            "tombstoned": false,
            "root": n.root,
            "parent": parent(tree, &n.root, &n.key_path),
        });
        if let Some(level) = world.get_resource::<TreeProvenance>().and_then(|p| (p.0)(world, e, &n)) {
            row["level"] = level;
        }
        return Some(row);
    }
    let t = tree.tombstones.get(&id)?;
    Some(json!({
        "node": id_str(id),
        "entity": null,
        "keyPath": t.key_path,
        "type": "entity",
        "source": t.source.first(),
        "tombstoned": true,
        "root": t.root,
        "parent": parent(tree, &t.root, &t.key_path),
    }))
}

/// A row's place in description order, parents first: at each step of its key path, the step's
/// index among its siblings, then its key (siblings mounted apart, a slot's groups, tie on index).
fn description_order(root: &str, order: &HashMap<u64, u32>, key_path: &Value) -> Vec<(u32, String)> {
    let path: Vec<String> = serde_json::from_value(key_path.clone()).unwrap_or_default();
    (1..=path.len()).map(|i| (order.get(&node_id(root, &path[..i])).copied().unwrap_or(u32::MAX), path[i - 1].clone())).collect()
}

/// A root's rows, in description order.
fn rows(world: &World, tree: &TsxTree, name: &str, r: &RootState) -> Vec<Value> {
    let mut ids: Vec<u64> = r.nodes.iter().copied().collect();
    ids.extend(tree.tombstones.iter().filter(|(_, t)| t.root == name).map(|(id, _)| *id));
    ids.sort();
    ids.dedup();
    let mut nodes: Vec<(Vec<(u32, String)>, Value)> = ids
        .into_iter()
        .filter_map(|id| tree_node(world, tree, id))
        .map(|row| (description_order(name, &r.order, &row["keyPath"]), row))
        .collect();
    nodes.sort_by(|a, b| a.0.cmp(&b.0));
    nodes.into_iter().map(|(_, row)| row).collect()
}

fn roots(world: &World) -> Vec<Value> {
    let tree = world.resource::<TsxTree>();
    tree.roots
        .iter()
        .map(|(name, r)| {
            let nodes = rows(world, tree, name, r);
            let mut root = json!({ "name": name, "location": r.location, "nodes": nodes });
            if world.contains_resource::<TreeProvenance>()
                && let Some(sidecar) = crate::scene_level(world, name)
            {
                root["sidecar"] = json!(sidecar);
            }
            root
        })
        .collect()
}

pub(crate) fn tree(In(_): In<Option<Value>>, world: &World) -> BrpResult {
    Ok(json!({ "seq": world.resource::<TsxTree>().journal.seq, "roots": roots(world) }))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct NodeParams {
    entity: Option<u64>,
    node: Option<String>,
}

pub(crate) fn node(In(p): In<Option<Value>>, world: &World) -> BrpResult {
    let p: NodeParams = serde_json::from_value(p.ok_or_else(|| invalid("params are required"))?).map_err(|e| invalid(e.to_string()))?;
    let tree = world.resource::<TsxTree>();
    let (e, n) = match (p.entity, p.node) {
        (Some(bits), _) => {
            let e = Entity::try_from_bits(bits).ok_or_else(|| invalid(format!("{bits} is not an entity")))?;
            let n = world.get::<TsxNode>(e).ok_or_else(|| invalid(format!("entity {e} was not made by tsx")))?.clone();
            (Some(e), n)
        }
        (None, Some(s)) => {
            let id = parse_id(&s)?;
            match live(world, tree, id) {
                Some((e, n)) => (Some(e), n),
                None => {
                    let t = tree.tombstones.get(&id).ok_or_else(|| invalid(format!("no tsx node {s}")))?;
                    return Ok(json!({
                        "node": s, "root": t.root, "entity": null, "keyPath": t.key_path,
                        "types": [], "source": t.source, "components": {},
                    }));
                }
            }
        }
        (None, None) => return Err(invalid("give entity or node")),
    };
    let components: Map<String, Value> = n
        .components
        .iter()
        .map(|(k, c)| {
            let fields: Map<String, Value> = c.fields.iter().map(|(f, s)| (f.clone(), json!({ "provenance": s.provenance, "value": s.applied }))).collect();
            (k.clone(), json!({ "source": c.source, "fields": fields }))
        })
        .collect();
    Ok(json!({
        "node": id_str(n.id),
        "root": n.root,
        "entity": e.map(|e| e.to_bits()),
        "keyPath": n.key_path,
        "types": n.types,
        "source": n.source,
        "components": components,
    }))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct FindParams {
    file: String,
    line: u32,
    column: Option<u32>,
}

/// Paths match when equal or when one ends with the other at a path boundary, so an editor may
/// send a project-relative or an absolute path.
fn same_file(a: &str, b: &str) -> bool {
    let (long, short) = if a.len() >= b.len() { (a, b) } else { (b, a) };
    long == short || (long.ends_with(short) && long[..long.len() - short.len()].ends_with('/'))
}

pub(crate) fn find(In(p): In<Option<Value>>, world: &mut World) -> BrpResult {
    let p: FindParams = serde_json::from_value(p.ok_or_else(|| invalid("params are required"))?).map_err(|e| invalid(e.to_string()))?;
    let mut matches: Vec<Value> = world
        .query::<(Entity, &TsxNode)>()
        .iter(world)
        .filter_map(|(e, n)| {
            let depth = n.source.iter().position(|s| same_file(&s.file, &p.file) && s.line == p.line && p.column.is_none_or(|c| c == s.column))?;
            Some(json!({ "node": id_str(n.id), "entity": e.to_bits(), "depth": depth }))
        })
        .collect();
    matches.sort_by(|a, b| a["node"].as_str().cmp(&b["node"].as_str()));
    Ok(json!({ "matches": matches }))
}

/// Subscribers of the watch: params to (frame last served, seq last sent).
#[derive(Resource, Default)]
struct Watchers(HashMap<String, (u64, u64)>);

fn message(world: &World, seq: u64, changes: &[(u64, Change)]) -> Value {
    let tree = world.resource::<TsxTree>();
    let mut merged = Change::default();
    for (_, c) in changes {
        merged.added.extend(&c.added);
        merged.removed.extend(&c.removed);
        merged.tombstoned.extend(&c.tombstoned);
        merged.moved.extend(&c.moved);
        merged.written.extend(&c.written);
        merged.ordered.extend(c.ordered.iter().cloned());
    }
    // A node removed and added again since the last message is still there.
    merged.removed.retain(|id| !tree.roots.iter().any(|(name, _)| tree.is_row(name, *id)));
    let ids = |v: &[u64]| v.iter().map(|id| id_str(*id)).collect::<Vec<_>>();
    let mut m = Map::new();
    m.insert("seq".into(), json!(seq));
    let added: Vec<Value> = merged.added.iter().filter_map(|id| tree_node(world, tree, *id)).collect();
    if !added.is_empty() {
        m.insert("added".into(), json!(added));
    }
    if !merged.removed.is_empty() {
        m.insert("removed".into(), json!(ids(&merged.removed)));
    }
    if !merged.tombstoned.is_empty() {
        m.insert("tombstoned".into(), json!(ids(&merged.tombstoned)));
    }
    if !merged.moved.is_empty() {
        let moved: Vec<Value> = merged
            .moved
            .iter()
            .map(|id| json!({ "node": id_str(*id), "source": live(world, tree, *id).and_then(|(_, n)| n.source.first().cloned()) }))
            .collect();
        m.insert("moved".into(), json!(moved));
    }
    if !merged.written.is_empty() {
        m.insert("written".into(), json!(ids(&merged.written)));
    }
    let ordered = ordered(world, tree, &merged.ordered);
    if !ordered.is_empty() {
        m.insert("ordered".into(), json!(ordered));
    }
    Value::Object(m)
}

/// Each sibling list's full order as the live tree has it now, skipping lists whose parent is gone.
fn ordered(world: &World, tree: &TsxTree, lists: &[(String, Option<u64>)]) -> Vec<Value> {
    let lists: BTreeSet<&(String, Option<u64>)> = lists.iter().collect();
    let mut children: HashMap<&str, HashMap<Option<String>, Vec<Value>>> = HashMap::new();
    let mut out = Vec::new();
    for (root, parent) in lists {
        let Some(r) = tree.roots.get(root) else { continue };
        if parent.is_some_and(|p| !tree.is_row(root, p)) {
            continue;
        }
        let by_parent = children.entry(root).or_insert_with(|| {
            let mut by_parent: HashMap<Option<String>, Vec<Value>> = HashMap::new();
            for row in rows(world, tree, root, r) {
                by_parent.entry(row["parent"].as_str().map(str::to_string)).or_default().push(row["node"].clone());
            }
            by_parent
        });
        let parent = parent.map(id_str);
        let list = by_parent.get(&parent).cloned().unwrap_or_default();
        out.push(json!({ "root": root, "parent": parent, "children": list }));
    }
    out
}

pub(crate) fn watch(In(p): In<Option<Value>>, world: &mut World) -> BrpResult<Option<Value>> {
    let frame = world.get_resource::<ScriptFrame>().map_or(0, |f| f.0);
    let key = p.as_ref().map(Value::to_string).unwrap_or_default();
    let seq = world.resource::<TsxTree>().journal.seq;
    let known = world.resource::<Watchers>().0.get(&key).copied();
    match known {
        // Served last frame (or this one): send what reconciled since.
        Some((last_frame, sent)) if last_frame + 1 >= frame => {
            world.resource_mut::<Watchers>().0.insert(key, (frame, seq));
            if sent == seq {
                return Ok(None);
            }
            let tree = world.resource::<TsxTree>();
            let changes: Vec<(u64, Change)> = tree.journal.messages.iter().filter(|(s, _)| *s > sent).cloned().collect();
            Ok(Some(message(world, seq, &changes)))
        }
        // A new subscriber: the snapshot.
        _ => {
            world.resource_mut::<Watchers>().0.insert(key, (frame, seq));
            Ok(Some(json!({ "seq": seq, "snapshot": roots(world) })))
        }
    }
}
