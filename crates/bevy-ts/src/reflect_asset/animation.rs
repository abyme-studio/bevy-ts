//! Hand-map entries for Bevy's animation assets (change `animation-control`, design 12): an
//! `AnimationGraph`'s `graph` is a petgraph `DiGraph` reflection cannot reach, so it is read and
//! written in Bevy's `SerializedAnimationGraph` shape:
//!
//! ```json
//! { "graph": { "nodes": [{ "node_type": "Blend", "mask": 0, "weight": 1 },
//!                        { "node_type": { "Clip": "models/Fox.glb#Animation0" }, "mask": 0, "weight": 1 }],
//!              "node_holes": [], "edge_property": "directed", "edges": [[0, 1, null]] },
//!   "root": 0,
//!   "mask_groups": [["<uuid>", 1], [{ "names": ["root", "hip"] }, 2]] }
//! ```
//!
//! `mask_groups` is a list of `[target, mask]` pairs, since a target is a blake3 of a bone path TS
//! cannot compute: a UUID string, or `{ names }`, which Bevy's `from_names` turns into the id.
//! Reads give UUIDs. A clip is an asset path, or an asset ref or value (a script-built clip).

use super::{HandMap, Slots};
use crate::reflect_value::HandleSlots;
use bevy::animation::AnimationTargetId;
use bevy::animation::graph::{AnimationGraph, AnimationGraphNode, AnimationNodeIndex as NodeIndex, AnimationNodeType};
use bevy::asset::uuid::Uuid;
use bevy::prelude::*;
use bevy::reflect::PartialReflect;
use serde_json::{Value, json};
use std::any::TypeId;

pub const GRAPH: HandMap = HandMap { asset: TypeId::of::<AnimationGraph>, read: graph_read, write: graph_write };

/// The graph as JSON. A clip with no path reads as its ref when a script made it, and fails
/// naming its node otherwise, as Bevy's `TryFrom<AnimationGraph>` does.
fn graph_read(slots: &Slots, _: Option<&Value>, value: &dyn PartialReflect) -> Result<Value, String> {
    let g = value.try_downcast_ref::<AnimationGraph>().ok_or("not an AnimationGraph")?;
    let mut nodes = vec![];
    for n in g.graph.node_indices() {
        let node = &g.graph[n];
        let node_type = match &node.node_type {
            AnimationNodeType::Clip(clip) => match clip.path() {
                Some(path) => json!({ "Clip": path.to_string() }),
                None => match HandleSlots::read(slots, &clip.clone().untyped()) {
                    Some(r) => json!({ "Clip": r }),
                    None => return Err(format!("AnimationGraph node {}: its clip has no asset path", n.index())),
                },
            },
            AnimationNodeType::Blend => json!("Blend"),
            AnimationNodeType::Add => json!("Add"),
        };
        nodes.push(json!({ "node_type": node_type, "mask": node.mask, "weight": node.weight }));
    }
    let edges: Vec<Value> = g.graph.raw_edges().iter().map(|e| json!([e.source().index(), e.target().index(), null])).collect();
    let mut groups: Vec<(String, u64)> = g.mask_groups.iter().map(|(t, m)| (t.0.to_string(), *m)).collect();
    groups.sort();
    Ok(json!({
        "graph": { "nodes": nodes, "node_holes": [], "edge_property": "directed", "edges": edges },
        "root": g.root.index(),
        "mask_groups": groups.into_iter().map(|(t, m)| json!([t, m])).collect::<Vec<_>>(),
    }))
}

/// The graph from JSON, whole: a key left out takes `AnimationGraph::new()`'s (one blend node, the
/// root). Each clip path loads through the asset server; in check mode (no server) nothing loads
/// and the form alone is checked.
fn graph_write(slots: &Slots, _: Option<&dyn PartialReflect>, json: &Value) -> Result<Box<dyn Reflect>, String> {
    let o = json.as_object().ok_or_else(|| format!("an AnimationGraph is an object, not {json}"))?;
    let mut g = AnimationGraph::new();
    if let Some(graph) = o.get("graph") {
        let graph = graph.as_object().ok_or_else(|| format!("AnimationGraph graph is an object, not {graph}"))?;
        if let Some(p) = graph.get("edge_property").filter(|p| *p != "directed") {
            return Err(format!("AnimationGraph graph edge_property is \"directed\", not {p}"));
        }
        if let Some(h) = graph.get("node_holes").filter(|h| h.as_array().is_none_or(|a| !a.is_empty())) {
            return Err(format!("AnimationGraph graph has no node holes, not {h}"));
        }
        g.graph.clear();
        for (i, node) in list(graph.get("nodes"), "graph nodes")?.iter().enumerate() {
            g.graph.add_node(graph_node(slots, node).map_err(|e| format!("AnimationGraph node {i}: {e}"))?);
        }
        let count = g.graph.node_count();
        for (i, edge) in list(graph.get("edges"), "graph edges")?.iter().enumerate() {
            if edge.is_null() {
                continue;
            }
            let pair = edge.as_array().filter(|a| a.len() >= 2).ok_or_else(|| format!("AnimationGraph edge {i} is [from, to, null], not {edge}"))?;
            let [from, to] = [&pair[0], &pair[1]].map(|n| n.as_u64().map(|n| n as usize).filter(|&n| n < count));
            let (Some(from), Some(to)) = (from, to) else {
                return Err(format!("AnimationGraph edge {i}: {edge} names a node the graph does not have ({count} nodes)"));
            };
            g.add_edge(NodeIndex::new(from), NodeIndex::new(to));
        }
        if !o.contains_key("root") && count == 0 {
            return Err("AnimationGraph graph has no nodes, so no root".into());
        }
    }
    if let Some(root) = o.get("root") {
        let n = root.as_u64().map(|n| n as usize).filter(|&n| n < g.graph.node_count());
        g.root = NodeIndex::new(n.ok_or_else(|| format!("AnimationGraph root {root} is not a node of the graph"))?);
    }
    if let Some(groups) = o.get("mask_groups") {
        for (i, pair) in list(Some(groups), "mask_groups")?.iter().enumerate() {
            let p = pair.as_array().filter(|a| a.len() == 2).ok_or_else(|| format!("AnimationGraph mask_groups {i} is [target, mask], not {pair}"))?;
            let target = target(&p[0]).map_err(|e| format!("AnimationGraph mask_groups {i}: {e}"))?;
            let mask = p[1].as_u64().ok_or_else(|| format!("AnimationGraph mask_groups {i}: a mask is a whole number, not {}", p[1]))?;
            // The same bone named twice (by names and by UUID) keeps both masks' groups.
            *g.mask_groups.entry(target).or_default() |= mask;
        }
    }
    Ok(Box::new(g))
}

fn list<'a>(v: Option<&'a Value>, what: &str) -> Result<&'a [Value], String> {
    match v {
        None => Ok(&[]),
        Some(v) => v.as_array().map(Vec::as_slice).ok_or_else(|| format!("AnimationGraph {what} is a list, not {v}")),
    }
}

fn graph_node(slots: &Slots, json: &Value) -> Result<AnimationGraphNode, String> {
    let o = json.as_object().ok_or_else(|| format!("a node is an object, not {json}"))?;
    let mut node = AnimationGraphNode::default();
    if let Some(t) = o.get("node_type") {
        node.node_type = match t {
            Value::String(s) if s == "Blend" => AnimationNodeType::Blend,
            Value::String(s) if s == "Add" => AnimationNodeType::Add,
            Value::Object(c) if c.len() == 1 && c.contains_key("Clip") => AnimationNodeType::Clip(clip(slots, &c["Clip"])?),
            _ => return Err(format!("node_type is \"Blend\", \"Add\" or {{ \"Clip\": path }}, not {t}")),
        };
    }
    if let Some(m) = o.get("mask") {
        node.mask = m.as_u64().ok_or_else(|| format!("mask is a whole number, not {m}"))?;
    }
    if let Some(w) = o.get("weight") {
        node.weight = w.as_f64().ok_or_else(|| format!("weight is a number, not {w}"))? as f32;
    }
    Ok(node)
}

/// A clip: a path the server loads (in check mode, its form only), or a ref or asset value.
fn clip(slots: &Slots, json: &Value) -> Result<Handle<AnimationClip>, String> {
    if let Value::String(path) = json {
        if path.is_empty() {
            return Err("a clip path is empty".into());
        }
        return Ok(slots.server.map(|s| s.load(path.clone())).unwrap_or_default());
    }
    let h = slots.write(TypeId::of::<AnimationClip>(), json)?;
    h.try_typed::<AnimationClip>().map_err(|e| e.to_string())
}

/// A target: a UUID string, or `{ names: [...] }` hashed with Bevy's `from_names`.
pub fn target(json: &Value) -> Result<AnimationTargetId, String> {
    match json {
        Value::String(s) => Uuid::parse_str(s).map(AnimationTargetId).map_err(|e| format!("target {s} is not a UUID: {e}")),
        Value::Object(o) if o.len() == 1 && o.contains_key("names") => {
            let names = o["names"].as_array().ok_or_else(|| format!("target names is a list of strings, not {}", o["names"]))?;
            let names: Vec<Name> = names
                .iter()
                .map(|n| n.as_str().map(|s| Name::new(s.to_string())).ok_or_else(|| format!("target names is a list of strings, not {n}")))
                .collect::<Result<_, _>>()?;
            Ok(AnimationTargetId::from_names(names.iter()))
        }
        _ => Err(format!("a target is a UUID string or {{ names: [...] }}, not {json}")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::reflect_asset::{ScriptAssets, add, flush, get};
    use bevy::animation::graph::SerializedAnimationGraph;
    use bevy::asset::AssetPlugin;

    fn app() -> App {
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, AssetPlugin::default(), bevy::animation::AnimationPlugin));
        app.register_type::<Name>();
        app.init_resource::<ScriptAssets>();
        app
    }

    const PATH: &str = "bevy_animation::graph::AnimationGraph";

    fn with_slots<T>(app: &App, check: bool, f: impl FnOnce(&Slots) -> T) -> T {
        let registry = app.world().resource::<AppTypeRegistry>().read();
        let server = app.world().resource::<AssetServer>();
        let slots = Slots { registry: &registry, server: (!check).then_some(server), assets: app.world().resource::<ScriptAssets>(), check };
        f(&slots)
    }

    /// The graph a script builds: a blend of two clips, an additive layer and a masked bone.
    fn built() -> Value {
        json!({
            "graph": {
                "nodes": [
                    { "node_type": "Blend", "mask": 0, "weight": 1.0 },
                    { "node_type": "Blend", "mask": 0, "weight": 1.0 },
                    { "node_type": { "Clip": "models/Fox.glb#Animation0" }, "mask": 0, "weight": 0.5 },
                    { "node_type": { "Clip": "models/Fox.glb#Animation1" }, "mask": 1, "weight": 0.5 },
                    { "node_type": "Add", "mask": 0, "weight": 1.0 },
                ],
                "node_holes": [],
                "edge_property": "directed",
                "edges": [[0, 1, null], [1, 2, null], [1, 3, null], [0, 4, null], [4, 2, null]],
            },
            "root": 0,
            "mask_groups": [[{ "names": ["root", "b_Hip_01"] }, 1]],
        })
    }

    #[test]
    fn animation_graph_round_trips() {
        let mut app = app();
        let r = with_slots(&app, false, |s| add(s, PATH, &built())).unwrap();
        flush(app.world_mut());
        let read = with_slots(&app, false, |s| get(app.world(), s, &r)).unwrap();
        let hip = AnimationTargetId::from_names([Name::new("root"), Name::new("b_Hip_01")].iter()).0.to_string();
        let mut want = built();
        want["mask_groups"] = json!([[hip, 1]]);
        assert_eq!(read, want, "a write then a read gives the value written, targets as UUIDs");

        // Bevy's own serialized shape, for a graph whose clips all have paths.
        let n = super::super::parse_ref(&r).unwrap().unwrap();
        let h = app.world().resource::<ScriptAssets>().handle(n).unwrap();
        let g = app.world().resource::<Assets<AnimationGraph>>().get(h.id().typed::<AnimationGraph>()).unwrap().clone();
        let mut bevy = serde_json::to_value(SerializedAnimationGraph::try_from(g).unwrap()).unwrap();
        bevy["mask_groups"] = json!([[hip, 1]]);
        assert_eq!(read, bevy, "the shape is Bevy's SerializedAnimationGraph, mask_groups as pairs");

        // A UUID target writes back as itself.
        let again = with_slots(&app, false, |s| graph_write(s, None, &read)).unwrap();
        assert_eq!(with_slots(&app, false, |s| graph_read(s, None, again.as_partial_reflect())).unwrap(), read);
    }

    #[test]
    fn an_empty_object_is_a_new_graph() {
        let app = app();
        let g = with_slots(&app, true, |s| graph_write(s, None, &json!({}))).unwrap();
        let g = g.as_partial_reflect().try_downcast_ref::<AnimationGraph>().unwrap();
        assert_eq!((g.graph.node_count(), g.root.index()), (1, 0));
    }

    #[test]
    fn check_mode_loads_nothing() {
        let app = app();
        with_slots(&app, true, |s| graph_write(s, None, &built())).unwrap();
        assert!(app.world().resource::<AssetServer>().get_handle::<AnimationClip>("models/Fox.glb#Animation0").is_none(), "no clip loads");
        let e = with_slots(&app, true, |s| graph_write(s, None, &json!({ "graph": { "nodes": [{ "node_type": "Lerp" }] } }))).unwrap_err();
        assert!(e.contains("node 0") && e.contains("Lerp"), "{e}");
    }

    #[test]
    fn a_bone_named_twice_keeps_both_masks() {
        let app = app();
        let hip = AnimationTargetId::from_names([Name::new("root"), Name::new("b_Hip_01")].iter());
        let j = json!({ "mask_groups": [[{ "names": ["root", "b_Hip_01"] }, 1], [hip.0.to_string(), 4]] });
        let g = with_slots(&app, true, |s| graph_write(s, None, &j)).unwrap();
        let g = g.as_partial_reflect().try_downcast_ref::<AnimationGraph>().unwrap();
        assert_eq!(g.mask_groups.get(&hip), Some(&5));
    }

    #[test]
    fn bad_graphs_name_what_is_wrong() {
        let app = app();
        let bad = |j: Value| with_slots(&app, true, |s| graph_write(s, None, &j)).unwrap_err();
        assert!(bad(json!({ "graph": { "nodes": [{}], "edges": [[0, 3, null]] } })).contains("edge 0"));
        assert!(bad(json!({ "root": 2 })).contains("root 2"));
        assert!(bad(json!({ "mask_groups": [["nope", 1]] })).contains("UUID"));
        assert!(bad(json!({ "mask_groups": [[{ "names": [1] }, 1]] })).contains("strings"));
    }

    #[test]
    fn a_pathless_clip_fails_naming_its_node() {
        let app = app();
        let mut g = AnimationGraph::new();
        // A clip handle with no path that no script made.
        g.add_clip(Handle::Uuid(Uuid::from_u128(7), Default::default()), 1.0, g.root);
        let e = with_slots(&app, false, |s| graph_read(s, None, g.as_partial_reflect())).unwrap_err();
        assert!(e.contains("node 1"), "{e}");
    }
}
