//! `bevy-ts/level_*` (protocol 1.6; @props and revert_prop 1.7): open levels, one level entity's owners and overrides,
//! writer batches, "Apply" one owner inward, and the event stream.

use crate::edit::{WriteRequest, WriteResult, hash_text, write_level};
use crate::engine::{BsnLevel, LevelInstance, Levels};
use crate::route::{loaded_hash, set_ops};
use crate::syntax::LineIndex;
use crate::writer::{Base, NewPatch, Op, WValue};
use bevy::prelude::*;
use bevy::remote::{BrpError, BrpResult, error_codes};
use bevy_ts::backend::SourceLocation;
use bevy_ts::components::TsComponents;
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
    bevy_ts_remote::add_method(app, CRATE, "bevy-ts/level_catalog", catalog);
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
        "revert_prop" => Op::RevertProp { path: path("path")?, prop: s("prop")?, field: o.get("field").and_then(Value::as_bool).unwrap_or(false) },
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
            let name = o.get("name").and_then(Value::as_str).map(String::from);
            Op::Add { parent: path("parent")?, name, base, patches, asset: o.get("asset").and_then(Value::as_str).map(String::from) }
        }
        "delete" => Op::Delete { path: path("path")? },
        "rename" => Op::Rename { path: path("path")?, to: s("to")? },
        "reparent" => Op::Reparent { path: path("path")?, parent: path("parent")?, index: o.get("index").and_then(Value::as_u64).map(|i| i as usize) },
        "restore" => Op::Restore { parent: path("parent")?, index: o.get("index").and_then(Value::as_u64).unwrap_or(0) as usize, text: s("text")? },
        "add_component" => Op::AddComponent { path: path("path")?, component: s("component")?, fields: values(o.get("fields"))?, text: o.get("text").and_then(Value::as_str).map(String::from) },
        "remove_component" => Op::RemoveComponent { path: path("path")?, component: s("component")? },
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
        Op::RevertProp { path, prop, field } => json!({ "op": "revert_prop", "path": path, "prop": prop, "field": field }),
        Op::Add { parent, name, base, patches, asset } => {
            let mut o = json!({ "op": "add_entity", "parent": parent });
            if let Some(n) = name {
                o["name"] = json!(n);
            }
            if let Some(a) = asset {
                o["asset"] = json!(a);
            }
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
        Op::Reparent { path, parent, index } => {
            let mut o = json!({ "op": "reparent", "path": path, "parent": parent });
            if let Some(i) = index {
                o["index"] = json!(i);
            }
            o
        }
        Op::Restore { parent, index, text } => json!({ "op": "restore", "parent": parent, "index": index, "text": text }),
        Op::AddComponent { path, component, fields, text } => {
            let mut o = json!({ "op": "add_component", "path": path, "component": component });
            if !fields.is_empty() {
                o["fields"] = kv(fields);
            }
            if let Some(t) = text {
                o["text"] = json!(t);
            }
            o
        }
        Op::RemoveComponent { path, component } => json!({ "op": "remove_component", "path": path, "component": component }),
    }
}

pub fn result_json(r: &WriteResult) -> Value {
    json!({
        "id": r.id, "file": r.file, "status": r.status, "reason": r.reason,
        "message": if r.message.is_empty() { Value::Null } else { json!(r.message) },
        "hash": r.hash, "lines": r.lines.iter().map(|(a, b)| json!([a, b])).collect::<Vec<_>>(), "disk": r.disk,
        "created": r.created.iter().map(|(p, e)| json!({ "namePath": p, "entity": e.map(|e| e.to_bits()) })).collect::<Vec<_>>(),
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
    // A scene component instance's @props, beside its fields.
    let props: Vec<Value> = world
        .get::<LevelInstance>(e)
        .and_then(|i| i.scene.as_ref())
        .map(|s| {
            s.props
                .iter()
                .map(|p| {
                    let mut o = json!({ "name": p.name, "type": p.ty, "value": p.value, "overridden": p.location.is_some() });
                    if let Some(at) = &p.location {
                        o["location"] = json!(at);
                    }
                    o
                })
                .collect()
        })
        .unwrap_or_default();
    // Components the file adds that its prefab does not give: set where the file writes them,
    // other than the scene component the entity instances and its `#Name`.
    let patches: Vec<&String> = n.components.iter().filter(|(k, c)| prefab.as_ref() != Some(*k) && k.as_str() != "Name" && c.source.as_ref().is_some_and(|s| s.file == level)).map(|(k, _)| k).collect();
    Ok(json!({ "level": level, "namePath": n.key_path, "entity": e.to_bits(), "owners": owners, "source": n.source, "overrides": overrides, "prefab": prefab, "props": props, "patches": patches }))
}

/// The live generation's scene components a level can place, by name.
pub fn catalog(In(_): In<Option<Value>>, world: &World) -> BrpResult {
    let ts = world.resource::<TsComponents>();
    let mut scenes: Vec<Value> = ts
        .current()
        .filter(|c| ts.live(&c.decl.name).is_some())
        .filter_map(|c| {
            let scene = c.decl.scene.as_ref()?;
            let props: Vec<Value> = scene
                .props
                .iter()
                .map(|p| json!({ "name": p.name, "type": p.ty, "default": p.default, "required": p.default.is_none() && !p.has_default && !p.optional }))
                .collect();
            let at = c.decl.location.as_ref();
            Some(json!({ "name": c.decl.name, "file": at.map(|l| &l.file), "line": at.map(|l| l.line), "props": props }))
        })
        .collect();
    scenes.sort_by(|a, b| a["name"].as_str().cmp(&b["name"].as_str()));
    Ok(json!({ "scenes": scenes }))
}

fn parse_hash(s: &str) -> Result<u64, BrpError> {
    u64::from_str_radix(s, 16).map_err(|_| invalid(format!("{s:?} is not a level hash")))
}

pub(crate) fn write(In(p): In<Option<Value>>, world: &mut World) -> BrpResult {
    let o = params(p)?;
    let file = string(&o, "file")?;
    // A file that does not exist yet (a sidecar's first write) has base null.
    let base = match o.get("base") {
        Some(Value::Null) => crate::engine::content_hash(b""),
        _ => parse_hash(&string(&o, "base")?)?,
    };
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
    if let Some(prop) = field.strip_prefix('@') {
        return apply_prop(world, e, &component, prop, id);
    }
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
        // The TSX element that sets the field below the override (`<Transform>`, not the
        // `<entity>` it is in), then the prefab call sites above it; a component only a level
        // adds has none.
        let tsx = |s: &&SourceLocation| !s.file.ends_with(".bsn");
        let element = c.source.as_ref().filter(tsx).map(|el| std::iter::once(el).chain(n.source.iter().skip(1).filter(tsx)).cloned().collect::<Vec<_>>());
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
            move_inward(world, id, inner_file, ops, &at.file, base, revert)
        }
        _ => {
            // TSX is next: the editor writes it, then removes the override with `then`.
            let Some(chain) = element else { return refused("no TSX element sets this field") };
            Ok(json!({
                "status": "tsx",
                "edit": { "element": chain[0], "prop": top, "value": value, "target": "element", "chain": chain },
                "then": { "id": id, "file": at.file, "base": hash_text(base), "ops": [op_to_json(&revert)] },
            }))
        }
    }
}

/// Writes `ops` into the inner file and `revert` into the outer one, as one transaction.
fn move_inward(world: &mut World, id: u64, inner_file: &str, ops: Vec<Op>, outer_file: &str, base: u64, revert: Op) -> BrpResult {
    let inner_base = loaded_hash(world, inner_file).ok_or_else(|| invalid(format!("{inner_file} is not loaded")))?;
    let inner = write_level(world, WriteRequest { id, file: inner_file.to_string(), base: inner_base, ops });
    if inner.status != "written" && inner.status != "rebased" {
        return Ok(json!({ "status": "refused", "reason": inner.reason, "inner": result_json(&inner) }));
    }
    let outer = write_level(world, WriteRequest { id, file: outer_file.to_string(), base, ops: vec![revert] });
    if outer.status != "written" && outer.status != "rebased" {
        // One transaction: the inner write is undone.
        let undo_base = loaded_hash(world, inner_file).unwrap_or(0);
        write_level(world, WriteRequest { id, file: inner_file.to_string(), base: undo_base, ops: inner.inverse.clone() });
        return Ok(json!({ "status": "refused", "reason": outer.reason, "outer": result_json(&outer) }));
    }
    Ok(json!({ "status": "written", "inner": result_json(&inner), "outer": result_json(&outer) }))
}

/// A placed prop's value as the writer prints it, by its manifest type.
fn prop_wvalue(ty: &Value, v: &Value) -> Result<WValue, String> {
    let kind = ty.get("kind").and_then(Value::as_str).unwrap_or("");
    let vector = |name: &str, n: usize| -> Result<WValue, String> {
        let a = v.as_array().filter(|a| a.len() == n).ok_or_else(|| format!("expected a {name}, got {v}"))?;
        let lanes = ["x", "y", "z", "w"][..n].iter().zip(a).map(|(k, x)| Ok((k.to_string(), x.as_f64().ok_or("a lane is a number")?))).collect::<Result<_, String>>()?;
        Ok(WValue::Vector { ty: name.into(), lanes })
    };
    Ok(match kind {
        "f32" | "f64" => WValue::Float { v: v.as_f64().ok_or("expected a number")?, f32: kind == "f32" },
        "bool" => WValue::Bool(v.as_bool().ok_or("expected a bool")?),
        "string" => WValue::Str(v.as_str().ok_or("expected a string")?.into()),
        "enum" => WValue::Path(v.as_str().ok_or("expected a variant name")?.into()),
        "vec2" => vector("Vec2", 2)?,
        "vec3" => vector("Vec3", 3)?,
        "vec4" => vector("Vec4", 4)?,
        "quat" => vector("Quat", 4)?,
        _ => WValue::Int(v.as_i64().ok_or("expected an integer")?),
    })
}

/// Apply for a scene component instance's `@prop`: into the next `.bsn` owner, or the scene's
/// destructured default (`static scene({ hp = 100 })`) through the editor.
fn apply_prop(world: &mut World, e: Entity, component: &str, prop: &str, id: u64) -> BrpResult {
    let refused = |reason: &str| Ok(json!({ "status": "refused", "reason": reason }));
    let Some(inst) = world.get::<LevelInstance>(e) else { return refused("not a level entity") };
    let Some(scene) = &inst.scene else { return refused("not a scene component instance") };
    if scene.component != component {
        return refused("no such component");
    }
    let Some(p) = scene.props.iter().find(|p| p.name == prop) else { return refused("no such prop") };
    let Some(at) = p.location.clone() else { return refused("no override") };
    let (ty, value) = (p.ty.clone(), p.value.clone());
    let frames: Vec<(String, Vec<String>)> = inst.owners.iter().map(|o| (o.file.clone(), o.name_path.clone())).collect();
    let Some(i) = frames.iter().position(|(f, _)| *f == at.file) else { return refused("the override's frame is not in the owner chain") };
    let revert = Op::RevertProp { path: frames[i].1.clone(), prop: prop.into(), field: false };
    let base = loaded_hash(world, &at.file).ok_or_else(|| invalid(format!("{} is not loaded", at.file)))?;
    match frames.get(i + 1) {
        Some((inner_file, inner_path)) if inner_file.ends_with(".bsn") && inner_file != &at.file => {
            let ops = vec![Op::SetProp { path: inner_path.clone(), prop: prop.into(), value: prop_wvalue(&ty, &value).map_err(invalid)?, field: false }];
            let inner_file = inner_file.clone();
            move_inward(world, id, &inner_file, ops, &at.file, base, revert)
        }
        _ => {
            let decl = world.resource::<TsComponents>().current().find(|c| c.decl.name == component).and_then(|c| c.decl.scene.as_ref()?.props.iter().find(|d| d.name == prop).cloned());
            let Some(decl) = decl else { return refused("no such prop") };
            if decl.has_default && decl.default.is_none() {
                return refused("computed");
            }
            let Some(site) = decl.location else { return refused("the scene does not destructure this prop") };
            Ok(json!({
                "status": "tsx",
                "edit": { "element": site, "prop": prop, "value": value, "target": "default", "chain": [site] },
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
