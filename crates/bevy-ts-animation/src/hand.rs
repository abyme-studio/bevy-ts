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

use crate::curve;
use bevy_ts::reflect_asset::Slots;
use bevy_ts::reflect_value::HandleSlots;
use bevy::animation::AnimationTargetId;
use bevy::animation::graph::{AnimationGraph, AnimationGraphNode, AnimationNodeIndex as NodeIndex, AnimationNodeType};
use bevy::asset::uuid::Uuid;
use bevy::prelude::*;
use serde_json::{Value, json};
use std::any::TypeId;

/// Hand-maps `AnimationGraph` and `AnimationClip` for scripts (`assets.add`, `get`, `set`, refs).
pub(crate) fn register(app: &mut App) {
    use bevy_ts::prelude::BevyTsAppExt;
    app.ts_hand_map::<AnimationGraph>(graph_read, graph_write).ts_hand_map::<AnimationClip>(clip_read, clip_write);
}

/// An `AnimationClip` (design 20): `{ curves: [[target, { property, curve }], ...] | null,
/// duration? }`, each curve as [`curve`] builds it. Bevy keeps a clip's curves as boxed trait
/// objects no read can turn back into JSON, so a read gives the JSON the script wrote, and a file
/// clip reads as `{ duration, curves: null }`.
fn clip_read(_: &Slots, source: Option<&Value>, c: &AnimationClip) -> Result<Value, String> {
    if let Some(s) = source {
        return Ok(s.clone());
    }
    Ok(json!({ "duration": c.duration(), "curves": null }))
}

/// `curves` replaces the clip's curves (`null` or absent keeps the base's); `duration` sets it,
/// else it is the longest curve's end. A curve with no end needs a `duration`. Check mode builds
/// the curves and keeps nothing else either way: nothing loads.
fn clip_write(slots: &Slots, base: Option<&AnimationClip>, json: &Value) -> Result<AnimationClip, String> {
    let o = json.as_object().ok_or_else(|| format!("an AnimationClip is an object, not {json}"))?;
    let mut clip = base.cloned().unwrap_or_default();
    let mut unbounded = None;
    match o.get("curves") {
        None | Some(Value::Null) => {}
        Some(Value::Array(curves)) => {
            clip = AnimationClip::default();
            for (i, pair) in curves.iter().enumerate() {
                let p = pair.as_array().filter(|a| a.len() == 2).ok_or_else(|| format!("AnimationClip curve {i} is [target, {{ property, curve }}], not {pair}"))?;
                let target = curve::target(&p[0]).map_err(|e| format!("AnimationClip curve {i}: {e}"))?;
                let domain = curve::add_curve(slots.registry, &mut clip, target, &p[1]).map_err(|e| format!("AnimationClip curve {i}: {e}"))?;
                if !domain.end().is_finite() && unbounded.is_none() {
                    unbounded = Some(i);
                }
            }
        }
        Some(c) => return Err(format!("AnimationClip curves is a list of [target, {{ property, curve }}] or null, not {c}")),
    }
    match o.get("duration") {
        None | Some(Value::Null) => {
            if let Some(i) = unbounded {
                return Err(format!("AnimationClip curve {i} has no end (forever, or an unbounded domain): give the clip a duration"));
            }
        }
        Some(d) => {
            let d = d.as_f64().filter(|d| d.is_finite() && *d >= 0.0).ok_or_else(|| format!("AnimationClip duration is a number of seconds, not {d}"))?;
            clip.set_duration(d as f32);
        }
    }
    Ok(clip)
}

/// The graph as JSON. A clip with no path reads as its ref when a script made it, and fails
/// naming its node otherwise, as Bevy's `TryFrom<AnimationGraph>` does.
fn graph_read(slots: &Slots, _: Option<&Value>, g: &AnimationGraph) -> Result<Value, String> {
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
fn graph_write(slots: &Slots, _: Option<&AnimationGraph>, json: &Value) -> Result<AnimationGraph, String> {
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
    Ok(g)
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
    use bevy_ts::reflect_asset::{ScriptAssets, add, flush, get, parse_ref};
    use bevy::animation::graph::SerializedAnimationGraph;
    use bevy::asset::AssetPlugin;

    fn app() -> App {
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, AssetPlugin::default(), bevy::animation::AnimationPlugin));
        app.register_type::<Name>();
        app.register_type::<Transform>();
        app.init_resource::<ScriptAssets>();
        register(&mut app);
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
        let n = parse_ref(&r).unwrap().unwrap();
        let h = app.world().resource::<ScriptAssets>().handle(n).unwrap();
        let g = app.world().resource::<Assets<AnimationGraph>>().get(h.id().typed::<AnimationGraph>()).unwrap().clone();
        let mut bevy = serde_json::to_value(SerializedAnimationGraph::try_from(g).unwrap()).unwrap();
        bevy["mask_groups"] = json!([[hip, 1]]);
        assert_eq!(read, bevy, "the shape is Bevy's SerializedAnimationGraph, mask_groups as pairs");

        // A UUID target writes back as itself.
        let again = with_slots(&app, false, |s| graph_write(s, None, &read)).unwrap();
        assert_eq!(with_slots(&app, false, |s| graph_read(s, None, &again)).unwrap(), read);
    }

    const CLIP_PATH: &str = "bevy_animation::AnimationClip";

    /// A tween of the root's translation and a forever spin of a named bone.
    fn tween() -> Value {
        let ease = json!({ "EasingCurve": { "start": [0, 0, 0], "end": [0, 2, 0], "ease": "CubicOut" } });
        let h = std::f64::consts::FRAC_1_SQRT_2;
        let spin = json!({ "Forever": { "EasingCurve": { "start": [0, 0, 0, 1], "end": [0, h, 0, h], "ease": "Linear" } } });
        json!({
            "curves": [
                [{ "names": [] }, { "property": { "component": "Transform", "field": "translation" }, "curve": { "ReparametrizeLinear": [ease, [0, 0.5]] } }],
                [{ "names": ["sign"] }, { "property": { "component": "bevy_transform::components::transform::Transform", "field": "rotation" }, "curve": spin }],
            ],
            "duration": 2,
        })
    }

    #[test]
    fn animation_clip_round_trips() {
        let mut app = app();
        let r = with_slots(&app, false, |s| add(s, CLIP_PATH, &tween())).unwrap();
        flush(app.world_mut());
        assert_eq!(with_slots(&app, false, |s| get(app.world(), s, &r)).unwrap(), tween(), "a read gives the JSON written");
        let n = parse_ref(&r).unwrap().unwrap();
        let h = app.world().resource::<ScriptAssets>().handle(n).unwrap();
        let c = app.world().resource::<Assets<AnimationClip>>().get(h.id().typed::<AnimationClip>()).unwrap();
        assert_eq!(c.duration(), 2.0);
        let root = AnimationTargetId::from_names(std::iter::empty::<&Name>());
        let sign = AnimationTargetId::from_names([Name::new("sign")].iter());
        assert_eq!((c.curves_for_target(root).map(|v| v.len()), c.curves_for_target(sign).map(|v| v.len())), (Some(1), Some(1)));

        // With no duration, the bounded curves set it.
        let mut short = tween();
        short["curves"].as_array_mut().unwrap().pop();
        short.as_object_mut().unwrap().remove("duration");
        let c = with_slots(&app, true, |s| clip_write(s, None, &short)).unwrap();
        assert_eq!(c.duration(), 0.5);

        // A clip no script wrote (a file) reads its duration and no curves; writing that back keeps its curves.
        let file = with_slots(&app, true, |s| clip_write(s, None, &tween())).unwrap();
        let read = with_slots(&app, true, |s| clip_read(s, None, &file)).unwrap();
        assert_eq!(read, json!({ "duration": 2.0, "curves": null }));
        let back = with_slots(&app, true, |s| clip_write(s, Some(&file), &json!({ "curves": null, "duration": 3 }))).unwrap();
        assert_eq!((back.duration(), back.curves().len()), (3.0, 2));
    }

    #[test]
    fn bad_clips_name_the_curve() {
        let app = app();
        let bad = |j: Value| with_slots(&app, true, |s| clip_write(s, None, &j)).unwrap_err();
        let mut forever = tween();
        forever.as_object_mut().unwrap().remove("duration");
        let e = bad(forever);
        assert!(e.contains("curve 1") && e.contains("duration"), "{e}");
        let e = bad(json!({ "curves": [[{ "names": [] }, { "property": { "component": "Transform", "field": "translation" }, "curve": { "ConstantCurve": { "domain": [0, 1], "value": 3 } } }]] }));
        assert!(e.contains("curve 0") && e.contains("Transform::translation") && e.contains("Vec3"), "{e}");
        let e = bad(json!({ "curves": [[{ "names": [] }, { "property": { "component": "Transform", "field": "nope" }, "curve": {} }]] }));
        assert!(e.contains("curve 0") && e.contains("no field nope"), "{e}");
        assert!(bad(json!({ "curves": 3 })).contains("curves"));
        assert!(bad(json!({ "duration": -1 })).contains("duration"));
    }

    #[test]
    fn an_empty_object_is_a_new_graph() {
        let app = app();
        let g = with_slots(&app, true, |s| graph_write(s, None, &json!({}))).unwrap();
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
        let e = with_slots(&app, false, |s| graph_read(s, None, &g)).unwrap_err();
        assert!(e.contains("node 1"), "{e}");
    }
}
