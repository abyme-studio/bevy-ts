//! `bevy-ts/level_*` (protocol 1.6): open levels, one level entity's owners and overrides,
//! writer batches, "Apply" one owner inward, and the event stream.

use crate::edit::{WriteRequest, WriteResult, hash_text, write_level};
use crate::engine::{BsnLevel, LevelInstance, Levels};
use crate::route::{loaded_hash, set_ops};
use crate::syntax::LineIndex;
use crate::writer::{Base, NewPatch, Op, WValue};
use bevy::prelude::*;
use bevy::remote::{BrpError, BrpResult, error_codes};
use bevy_ts_tsx::TsxNode;
use serde_json::{Map, Value, json};
use std::collections::HashMap;

/// The crate the contract lists these methods under.
pub const CRATE: &str = "bevy-ts-levels";

/// Serves the `bevy-ts/level_*` methods; needs `RemotePlugin` (or `BevyTsRemotePlugin`) first.
pub fn add_methods(app: &mut App) {
    bevy_ts_remote::add_method(app, CRATE, "bevy-ts/level_list", list);
    bevy_ts_remote::add_method(app, CRATE, "bevy-ts/level_node", node);
    bevy_ts_remote::add_method(app, CRATE, "bevy-ts/level_write", write);
    bevy_ts_remote::add_method(app, CRATE, "bevy-ts/level_apply", apply);
    bevy_ts_remote::add_watching_method(app, CRATE, "bevy-ts/level+watch", watch);
    app.init_resource::<Watchers>();
}

fn invalid(message: impl Into<String>) -> BrpError {
    BrpError { code: error_codes::INVALID_PARAMS, message: message.into(), data: None }
}

fn params(p: Option<Value>) -> Result<Map<String, Value>, BrpError> {
    match p {
        Some(Value::Object(o)) => Ok(o),
        None => Ok(Map::new()),
        Some(other) => Err(invalid(format!("params must be an object, got {other}"))),
    }
}

fn strings(v: Option<&Value>, what: &str) -> Result<Vec<String>, BrpError> {
    match v {
        None => Ok(Vec::new()),
        Some(v) => serde_json::from_value(v.clone()).map_err(|_| invalid(format!("{what}: expected an array of strings"))),
    }
}

fn string(o: &Map<String, Value>, key: &str) -> Result<String, BrpError> {
    o.get(key).and_then(Value::as_str).map(String::from).ok_or_else(|| invalid(format!("{key} is required")))
}

/// A `LevelValue` as the writer's value.
pub fn value_from_json(v: &Value) -> Result<WValue, String> {
    Ok(match v {
        Value::Number(n) => WValue::Float { v: n.as_f64().ok_or("not a number")?, f32: false },
        Value::Bool(b) => WValue::Bool(*b),
        Value::String(s) => WValue::Str(s.clone()),
        Value::Object(o) if o.contains_key("int") => WValue::Int(o["int"].as_i64().ok_or("int: expected an integer")?),
        Value::Object(o) if o.contains_key("path") => WValue::Path(o["path"].as_str().ok_or("path: expected a string")?.to_string()),
        Value::Object(o) if o.contains_key("lanes") => WValue::Vector {
            ty: o.get("type").and_then(Value::as_str).ok_or("type: expected a string")?.to_string(),
            lanes: o["lanes"].as_object().ok_or("lanes: expected an object")?.iter().map(|(k, v)| Ok((k.clone(), v.as_f64().ok_or("a lane is a number")?))).collect::<Result<_, String>>()?,
        },
        other => return Err(format!("not a level value: {other}")),
    })
}

pub fn value_to_json(v: &WValue) -> Value {
    match v {
        WValue::Float { v, .. } => json!(v),
        WValue::Int(i) => json!({ "int": i }),
        WValue::Bool(b) => json!(b),
        WValue::Str(s) => json!(s),
        WValue::Vector { ty, lanes } => json!({ "type": ty, "lanes": lanes.iter().map(|(k, v)| (k.clone(), json!(v))).collect::<Map<_, _>>() }),
        WValue::Path(p) => json!({ "path": p }),
    }
}

fn values(v: Option<&Value>) -> Result<Vec<(String, WValue)>, String> {
    let Some(Value::Object(o)) = v else { return Ok(Vec::new()) };
    o.iter().map(|(k, v)| Ok((k.clone(), value_from_json(v)?))).collect()
}

/// A `LevelOp` as the writer's operation.
pub fn op_from_json(v: &Value) -> Result<Op, String> {
    let o = v.as_object().ok_or("an op is an object")?;
    let op = o.get("op").and_then(Value::as_str).ok_or("op is required")?;
    let s = |k: &str| o.get(k).and_then(Value::as_str).map(String::from).ok_or_else(|| format!("{op}: {k} is required"));
    let path = |k: &str| -> Result<Vec<String>, String> { serde_json::from_value(o.get(k).cloned().unwrap_or(json!([]))).map_err(|_| format!("{op}: {k} is a name path")) };
    let val = || value_from_json(o.get("value").ok_or_else(|| format!("{op}: value is required"))?);
    Ok(match op {
        "set" => Op::Set { path: path("path")?, component: s("component")?, field: path("field")?, value: val()?, containers: path("containers")? },
        "revert" => Op::Revert { path: path("path")?, component: s("component")?, field: path("field")? },
        "set_prop" => Op::SetProp { path: path("path")?, prop: s("prop")?, value: val()?, field: o.get("field").and_then(Value::as_bool).unwrap_or(false) },
        "add_entity" | "add_instance" => {
            let base = match (o.get("scene").and_then(Value::as_str), o.get("include").and_then(Value::as_str)) {
                (Some(c), _) => Base::Scene { component: c.into(), props: values(o.get("props"))?, fields: values(o.get("fields"))? },
                (None, Some(p)) => Base::Include(p.into()),
                (None, None) if op == "add_instance" => return Err("add_instance: give scene or include".into()),
                _ => Base::None,
            };
            let patches = match o.get("patches") {
                Some(Value::Array(a)) => a
                    .iter()
                    .map(|p| Ok(NewPatch { component: p.get("component").and_then(Value::as_str).ok_or("a patch names its component")?.into(), fields: values(p.get("fields"))? }))
                    .collect::<Result<_, String>>()?,
                _ => Vec::new(),
            };
            Op::Add { parent: path("parent")?, name: s("name")?, base, patches }
        }
        "delete" => Op::Delete { path: path("path")? },
        "rename" => Op::Rename { path: path("path")?, to: s("to")? },
        "reparent" => Op::Reparent { path: path("path")?, parent: path("parent")? },
        "restore" => Op::Restore { parent: path("parent")?, index: o.get("index").and_then(Value::as_u64).unwrap_or(0) as usize, text: s("text")? },
        other => return Err(format!("unknown op {other}")),
    })
}

fn kv(v: &[(String, WValue)]) -> Value {
    Value::Object(v.iter().map(|(k, v)| (k.clone(), value_to_json(v))).collect())
}

pub fn op_to_json(op: &Op) -> Value {
    match op {
        Op::Set { path, component, field, value, containers } => json!({ "op": "set", "path": path, "component": component, "field": field, "value": value_to_json(value), "containers": containers }),
        Op::Revert { path, component, field } => json!({ "op": "revert", "path": path, "component": component, "field": field }),
        Op::SetProp { path, prop, value, field } => json!({ "op": "set_prop", "path": path, "prop": prop, "value": value_to_json(value), "field": field }),
        Op::Add { parent, name, base, patches } => {
            let mut o = json!({ "op": "add_entity", "parent": parent, "name": name });
            match base {
                Base::None => {}
                Base::Scene { component, props, fields } => {
                    o["op"] = json!("add_instance");
                    o["scene"] = json!(component);
                    o["props"] = kv(props);
                    o["fields"] = kv(fields);
                }
                Base::Include(p) => {
                    o["op"] = json!("add_instance");
                    o["include"] = json!(p);
                }
            }
            if !patches.is_empty() {
                o["patches"] = json!(patches.iter().map(|p| json!({ "component": p.component, "fields": kv(&p.fields) })).collect::<Vec<_>>());
            }
            o
        }
        Op::Delete { path } => json!({ "op": "delete", "path": path }),
        Op::Rename { path, to } => json!({ "op": "rename", "path": path, "to": to }),
        Op::Reparent { path, parent } => json!({ "op": "reparent", "path": path, "parent": parent }),
        Op::Restore { parent, index, text } => json!({ "op": "restore", "parent": parent, "index": index, "text": text }),
    }
}

pub fn result_json(r: &WriteResult) -> Value {
    json!({
        "id": r.id, "file": r.file, "status": r.status, "reason": r.reason,
        "message": if r.message.is_empty() { Value::Null } else { json!(r.message) },
        "hash": r.hash, "lines": r.lines.iter().map(|(a, b)| json!([a, b])).collect::<Vec<_>>(), "disk": r.disk,
        "inverse": r.inverse.iter().map(op_to_json).collect::<Vec<_>>(),
    })
}

pub(crate) fn list(In(_): In<Option<Value>>, world: &World) -> BrpResult {
    let levels = world.resource::<Levels>();
    let assets = world.resource::<Assets<BsnLevel>>();
    let out: Vec<Value> = levels
        .roots
        .iter()
        .map(|(path, r)| {
            let mut extensions = Vec::new();
            if let Some(a) = assets.get(&r.handle) {
                let index = LineIndex::new(&a.parsed.source);
                for u in &a.parsed.extensions {
                    let (line, column) = index.line_col(&a.parsed.source, u.span.start);
                    extensions.push(json!({ "code": u.extension.code(), "line": line, "column": column }));
                }
            }
            let diagnostics: Vec<Value> = r.error.iter().chain(r.warnings.iter()).map(|e| json!({ "message": e.message, "location": e.location })).collect();
            json!({
                "path": path,
                "hash": r.applied.get(path).map(|h| hash_text(*h)),
                "files": r.files.iter().collect::<Vec<_>>(),
                "extensions": extensions,
                "diagnostics": diagnostics,
            })
        })
        .collect();
    Ok(json!({ "levels": out }))
}

fn entity_of(world: &World, o: &Map<String, Value>) -> Result<Entity, BrpError> {
    if let Some(bits) = o.get("entity").and_then(Value::as_u64) {
        return Entity::try_from_bits(bits).ok_or_else(|| invalid(format!("{bits} is not an entity")));
    }
    let level = string(o, "level")?;
    let path = strings(o.get("namePath"), "namePath")?;
    let id = bevy_ts_tsx::desc::node_id(&crate::engine::root_name(&level), &path);
    world.resource::<bevy_ts_tsx::TsxTree>().entity(id).ok_or_else(|| invalid(format!("no entity {} in {level}", path.join("/"))))
}

pub(crate) fn node(In(p): In<Option<Value>>, world: &World) -> BrpResult {
    let o = params(p)?;
    let e = entity_of(world, &o)?;
    let n = world.get::<TsxNode>(e).ok_or_else(|| invalid(format!("entity {e} was not made by a level")))?;
    let level = n.root.strip_prefix("level:").ok_or_else(|| invalid(format!("entity {e} is not in a level")))?;
    let owners: Vec<Value> = {
        let mut x = e;
        let mut inst = world.get::<LevelInstance>(x);
        while inst.is_none() {
            let Some(p) = world.get::<ChildOf>(x) else { break };
            x = p.parent();
            inst = world.get::<LevelInstance>(x);
        }
        inst.map(|i| i.owners.iter().map(|o| json!({ "file": o.file, "namePath": o.name_path, "line": o.line })).collect()).unwrap_or_default()
    };
    let mut overrides = Vec::new();
    for (c, owned) in &n.components {
        for (f, at) in &owned.overrides {
            overrides.push(json!({ "component": c, "field": f, "value": owned.fields.get(f).map(|s| s.applied.clone()), "location": at }));
        }
    }
    let prefab = n.types.last().filter(|t| t.as_str() != "entity").cloned();
    Ok(json!({ "level": level, "namePath": n.key_path, "entity": e.to_bits(), "owners": owners, "source": n.source, "overrides": overrides, "prefab": prefab }))
}

fn parse_hash(s: &str) -> Result<u64, BrpError> {
    u64::from_str_radix(s, 16).map_err(|_| invalid(format!("{s:?} is not a level hash")))
}

pub(crate) fn write(In(p): In<Option<Value>>, world: &mut World) -> BrpResult {
    let o = params(p)?;
    let file = string(&o, "file")?;
    let base = parse_hash(&string(&o, "base")?)?;
    let ops: Vec<Op> = o.get("ops").and_then(Value::as_array).ok_or_else(|| invalid("ops is required"))?.iter().map(op_from_json).collect::<Result<_, _>>().map_err(invalid)?;
    let id = o.get("id").and_then(Value::as_u64).unwrap_or(0);
    let r = write_level(world, WriteRequest { id, file, base, ops });
    Ok(result_json(&r))
}

/// Moves an override one owner inward (decision 7).
pub(crate) fn apply(In(p): In<Option<Value>>, world: &mut World) -> BrpResult {
    let o = params(p)?;
    let e = entity_of(world, &o)?;
    let component = string(&o, "component")?;
    let field = string(&o, "field")?;
    let id = o.get("id").and_then(Value::as_u64).unwrap_or(0);
    let top = field.split('.').next().unwrap_or(&field).to_string();
    let refused = |reason: &str| Ok(json!({ "status": "refused", "reason": reason }));
    let (at, value, element, frames) = {
        let Some(n) = world.get::<TsxNode>(e) else { return refused("not a level entity") };
        let Some(c) = n.components.get(&component) else { return refused("no such component") };
        let Some(at) = c.overrides.get(&top) else { return refused("no override") };
        let value = c.fields.get(&top).map(|s| s.applied.clone()).unwrap_or(Value::Null);
        let mut x = e;
        let mut inst = world.get::<LevelInstance>(x).cloned();
        while inst.is_none() {
            let Some(p) = world.get::<ChildOf>(x) else { break };
            x = p.parent();
            inst = world.get::<LevelInstance>(x).cloned();
        }
        // Each frame's path to this entity: the frame's path to the level-made ancestor, then the
        // key segments below it (a prefab's inner entities).
        let frames: Vec<(String, Vec<String>)> = inst
            .map(|i| {
                let below = n.key_path.get(i.name_path.len()..).unwrap_or(&[]).to_vec();
                i.owners.iter().map(|o| (o.file.clone(), o.name_path.iter().chain(below.iter()).cloned().collect())).collect()
            })
            .unwrap_or_default();
        // The TSX element that sets the field below the override, if any.
        let element = n.source.first().cloned();
        (at.clone(), value, element, frames)
    };
    // The override's frame, then the next one inward.
    let Some(i) = frames.iter().position(|(f, _)| *f == at.file) else { return refused("the override's frame is not in the owner chain") };
    let outer_path = frames[i].1.clone();
    let revert = Op::Revert { path: outer_path.clone(), component: component.clone(), field: vec![top.clone()] };
    let base = loaded_hash(world, &at.file).ok_or_else(|| invalid(format!("{} is not loaded", at.file)))?;
    match frames.get(i + 1) {
        Some((inner_file, inner_path)) if inner_file.ends_with(".bsn") && inner_file != &at.file => {
            let ops = set_ops(world, inner_path, &component, &top, &value, &Map::new()).map_err(invalid)?;
            let inner_base = loaded_hash(world, inner_file).ok_or_else(|| invalid(format!("{inner_file} is not loaded")))?;
            let inner = write_level(world, WriteRequest { id, file: inner_file.clone(), base: inner_base, ops });
            if inner.status != "written" && inner.status != "rebased" {
                return Ok(json!({ "status": "refused", "reason": inner.reason, "inner": result_json(&inner) }));
            }
            let outer = write_level(world, WriteRequest { id, file: at.file.clone(), base, ops: vec![revert] });
            if outer.status != "written" && outer.status != "rebased" {
                // One transaction: the inner write is undone.
                let undo_base = loaded_hash(world, inner_file).unwrap_or(0);
                write_level(world, WriteRequest { id, file: inner_file.clone(), base: undo_base, ops: inner.inverse.clone() });
                return Ok(json!({ "status": "refused", "reason": outer.reason, "outer": result_json(&outer) }));
            }
            Ok(json!({ "status": "written", "inner": result_json(&inner), "outer": result_json(&outer) }))
        }
        _ => {
            // TSX is next: the editor writes it, then removes the override with `then`.
            let Some(element) = element else { return refused("no TSX element sets this field") };
            Ok(json!({
                "status": "tsx",
                "edit": { "element": element, "prop": top, "value": value },
                "then": { "id": id, "file": at.file, "base": hash_text(base), "ops": [op_to_json(&revert)] },
            }))
        }
    }
}

/// Subscribers of the watch: params to the last sequence number sent.
#[derive(Resource, Default)]
struct Watchers(HashMap<String, u64>);

pub(crate) fn watch(In(p): In<Option<Value>>, world: &mut World) -> BrpResult<Option<Value>> {
    let key = p.as_ref().map(Value::to_string).unwrap_or_default();
    let seq = world.resource::<Levels>().seq;
    let sent = world.resource::<Watchers>().0.get(&key).copied();
    let Some(sent) = sent else {
        // A new subscriber: one `loaded` per open level.
        world.resource_mut::<Watchers>().0.insert(key, seq);
        let levels = world.resource::<Levels>();
        let first = levels.roots.iter().next().map(|(p, r)| json!({ "event": "loaded", "level": p, "hash": r.applied.get(p).map(|h| hash_text(*h)) }));
        return Ok(first);
    };
    let next = world.resource::<Levels>().events.iter().find(|(s, _)| *s > sent).cloned();
    match next {
        Some((s, m)) => {
            world.resource_mut::<Watchers>().0.insert(key, s);
            Ok(Some(m))
        }
        None => Ok(None),
    }
}
