//! Component schemas, reads and edits: TS components from their native layout, reflected Rust
//! components through the stock `bevy_remote` handlers, in the stock shapes.

use crate::{Subscribers, frame, invalid, params, protocol, required};
use bevy::ecs::component::ComponentId;
use bevy::prelude::*;
use bevy::remote::builtin_methods::{
    export_registry_types, process_remote_get_components_request, process_remote_mutate_components_request,
};
use bevy::remote::{BrpError, BrpResult, error_codes};
use bevy_ts::components::{ComponentState, TsComponent, TsComponents};
use bevy_ts::layout::FieldType;
use bevy_ts::value::{check_hints, decode_component, encode_field, resolve_path};
use serde_json::{Map, Value, json};
use std::collections::{HashMap, HashSet};

fn short_path(name: &str) -> String {
    name.rsplit("::").next().unwrap_or(name).to_string()
}

fn field_schema(ty: &FieldType, hints: Option<&bevy_ts::manifest::FieldHints>) -> Value {
    let mut s = match ty {
        FieldType::F32 | FieldType::F64 => json!({ "type": "number" }),
        FieldType::Bool => json!({ "type": "boolean" }),
        FieldType::Entity => json!({ "type": "integer", "minimum": 0 }),
        FieldType::Vec2 | FieldType::Vec3 | FieldType::Vec4 | FieldType::Quat => {
            let n = ty.lanes().unwrap();
            json!({ "type": "array", "items": { "type": "number" }, "minItems": n, "maxItems": n })
        }
        FieldType::Enum { variants } => json!({ "type": "string", "enum": variants }),
        FieldType::Array { of, len } => json!({ "type": "array", "items": field_schema(of, None), "minItems": len, "maxItems": len }),
        int => {
            let (min, max): (i64, i64) = match int {
                FieldType::I8 => (i8::MIN.into(), i8::MAX.into()),
                FieldType::U8 => (0, u8::MAX.into()),
                FieldType::I16 => (i16::MIN.into(), i16::MAX.into()),
                FieldType::U16 => (0, u16::MAX.into()),
                FieldType::I32 => (i32::MIN.into(), i32::MAX.into()),
                _ => (0, u32::MAX.into()),
            };
            json!({ "type": "integer", "minimum": min, "maximum": max })
        }
    };
    if let Some(h) = hints {
        if let Some(min) = h.min {
            s["minimum"] = json!(min);
        }
        if let Some(max) = h.max {
            s["maximum"] = json!(max);
        }
        if let Some(label) = &h.label {
            s["title"] = json!(label);
        }
    }
    s
}

/// A TS component in `registry.schema`'s shape with `x-bevy-ts`.
pub(crate) fn ts_schema(c: &TsComponent) -> Value {
    let mut properties = Map::new();
    let mut fields = Map::new();
    for f in &c.layout.fields {
        let decl = c.decl.field(&f.name);
        properties.insert(f.name.clone(), field_schema(&f.ty, decl.map(|d| &d.hints)));
        let mut info = json!({ "type": f.ty, "offset": f.offset, "hints": decl.map(|d| d.hints.clone()).unwrap_or_default() });
        if let Some(d) = decl.and_then(|d| d.default.clone()) {
            info["default"] = d;
        }
        fields.insert(f.name.clone(), info);
    }
    json!({
        "typePath": c.decl.name,
        "shortPath": short_path(&c.decl.name),
        "kind": "Struct",
        "type": "object",
        "properties": properties,
        "required": c.layout.fields.iter().map(|f| f.name.clone()).collect::<Vec<_>>(),
        "reflectTypes": ["Component"],
        "x-bevy-ts": {
            "source": "ts",
            "file": c.decl.location.as_ref().map(|l| l.file.clone()),
            "line": c.decl.location.as_ref().map(|l| l.line),
            "declared": c.state == ComponentState::Live,
            "layout": c.layout,
            "fields": fields,
        },
    })
}

fn matches(names: &[String], type_path: &str, short: &str) -> bool {
    names.is_empty() || names.iter().any(|n| n == type_path || n == short)
}

pub(crate) fn schema(In(p): In<Option<Value>>, world: &World) -> BrpResult {
    let p: protocol::SchemaParams = params(p)?;
    let mut out = Map::new();
    if p.source != Some(protocol::ComponentSource::Rust) {
        for c in world.resource::<TsComponents>().current() {
            if matches(&p.names, &c.decl.name, &short_path(&c.decl.name)) {
                out.insert(c.decl.name.clone(), ts_schema(c));
            }
        }
    }
    if p.source != Some(protocol::ComponentSource::Ts) {
        let stock = export_registry_types(In(Some(json!({ "with_crates": [], "type_limit": { "with": ["Component"] } }))), world)?;
        if let Value::Object(all) = stock {
            for (path, s) in all {
                let short = s.get("shortPath").and_then(Value::as_str).unwrap_or_default().to_string();
                if matches(&p.names, &path, &short) {
                    out.insert(path, s);
                }
            }
        }
    }
    Ok(Value::Object(out))
}

/// A live or undeclared TS component by name (retired ids never answer).
fn ts_component(world: &World, name: &str) -> Option<TsComponent> {
    world.resource::<TsComponents>().get(name).cloned()
}

fn read_ts(world: &World, entity: Entity, c: &TsComponent) -> Option<Value> {
    let p = world.get_by_id(entity, c.id)?;
    // SAFETY: the component's column holds `layout.size` bytes for this entity.
    let bytes = unsafe { std::slice::from_raw_parts(p.as_ptr(), c.layout.size) };
    Some(decode_component(&c.layout, bytes))
}

fn component_error(code: i16, message: String) -> Value {
    json!({ "code": code, "message": message })
}

/// Splits requested names into TS components and the rest (handed to the stock handler).
fn split(world: &World, names: &[String]) -> (Vec<TsComponent>, Vec<String>) {
    let mut ts = Vec::new();
    let mut rust = Vec::new();
    for n in names {
        match ts_component(world, n) {
            Some(c) => ts.push(c),
            None => rust.push(n.clone()),
        }
    }
    (ts, rust)
}

pub(crate) fn get_components(In(p): In<Option<Value>>, world: &World) -> BrpResult {
    let p: protocol::GetComponentsParams = required(p)?;
    let entity = Entity::try_from_bits(p.entity.0).ok_or_else(|| invalid(format!("{} is not an entity", p.entity.0)))?;
    if world.get_entity(entity).is_err() {
        return Err(BrpError::entity_not_found(entity));
    }
    let (ts, rust) = split(world, &p.components);
    let mut components = Map::new();
    let mut errors = Map::new();
    for c in ts {
        match read_ts(world, entity, &c) {
            Some(v) => {
                components.insert(c.decl.name.clone(), v);
            }
            None if p.strict => return Err(invalid(format!("entity {entity} has no {}", c.decl.name))),
            None => {
                errors.insert(c.decl.name.clone(), component_error(error_codes::COMPONENT_ERROR, format!("entity {entity} has no {}", c.decl.name)));
            }
        }
    }
    if !rust.is_empty() {
        let stock = process_remote_get_components_request(
            In(Some(json!({ "entity": entity, "components": rust, "strict": p.strict }))),
            world,
        )?;
        if let Some(Value::Object(c)) = stock.get("components") {
            components.extend(c.clone());
        }
        if let Some(Value::Object(e)) = stock.get("errors") {
            errors.extend(e.clone());
        }
    }
    Ok(json!({ "components": components, "errors": errors }))
}

/// What a `get_components+watch` subscriber was last sent.
#[derive(Default)]
struct WatchState {
    tick: u32,
    present: HashSet<String>,
}

#[derive(Resource, Default)]
struct ComponentWatches(HashMap<String, WatchState>);

fn component_id(world: &World, name: &str) -> Option<ComponentId> {
    if let Some(c) = world.resource::<TsComponents>().get(name) {
        return Some(c.id);
    }
    let registry = world.resource::<AppTypeRegistry>().read();
    let reg = registry.get_with_type_path(name)?;
    world.components().get_id(reg.type_id())
}

pub(crate) fn get_components_watch(In(p): In<Option<Value>>, world: &mut World) -> BrpResult<Option<Value>> {
    let key = p.as_ref().map(Value::to_string).unwrap_or_default();
    let now = frame(world);
    let fresh = world.resource_mut::<Subscribers>().cursor("get_components", &p, now).is_none();
    let params: protocol::GetComponentsParams = required(p)?;
    let entity = Entity::try_from_bits(params.entity.0).ok_or_else(|| invalid(format!("{} is not an entity", params.entity.0)))?;
    world.init_resource::<ComponentWatches>();
    let this_run = world.change_tick();
    let mut watches = world.remove_resource::<ComponentWatches>().unwrap();
    if fresh {
        watches.0.remove(&key);
    }
    let first = !watches.0.contains_key(&key);
    let state = watches.0.entry(key).or_default();
    let last = bevy::ecs::change_detection::Tick::new(state.tick);
    let mut changed = Vec::new();
    let mut removed = Vec::new();
    let mut errors = Map::new();
    let mut present = HashSet::new();
    if let Ok(e) = world.get_entity(entity) {
        for name in &params.components {
            let Some(id) = component_id(world, name) else {
                errors.insert(name.clone(), component_error(error_codes::COMPONENT_ERROR, format!("unknown component {name}")));
                continue;
            };
            match e.get_change_ticks_by_id(id) {
                Some(t) => {
                    present.insert(name.clone());
                    if first || t.is_changed(last, this_run) {
                        changed.push(name.clone());
                    }
                }
                None => {
                    if state.present.contains(name) {
                        removed.push(name.clone());
                    }
                }
            }
        }
    } else if !state.present.is_empty() {
        removed.extend(state.present.iter().cloned());
    }
    state.tick = this_run.get();
    state.present = present;
    world.insert_resource(watches);
    if changed.is_empty() && removed.is_empty() && (errors.is_empty() || !first) {
        return Ok(None);
    }
    let mut components = Map::new();
    if !changed.is_empty() {
        let got = get_components(In(Some(json!({ "entity": entity, "components": changed, "strict": false }))), world)?;
        if let Some(Value::Object(c)) = got.get("components") {
            components = c.clone();
        }
        if let Some(Value::Object(e)) = got.get("errors") {
            errors.extend(e.clone());
        }
    }
    Ok(Some(json!({ "components": components, "removed": removed, "errors": errors })))
}

fn write_ts(world: &mut World, entity: Entity, c: &TsComponent, path: &str, v: &Value) -> Result<(), BrpError> {
    let target = resolve_path(&c.layout, path).map_err(|e| invalid(e.to_string()))?;
    let hints = c.decl.field(&target.field).map(|f| f.hints.clone()).unwrap_or_default();
    let mut bytes = vec![0u8; target.ty.size()];
    let check = encode_field(path, &target.ty, v, &mut bytes).and_then(|_| check_hints(path, &target.ty, &hints, v));
    check.map_err(|e| BrpError { code: error_codes::INVALID_PARAMS, message: e.to_string(), data: Some(json!({ "path": e.path, "expected": e.expected })) })?;
    let mut e = world.get_entity_mut(entity).map_err(|_| BrpError::entity_not_found(entity))?;
    let mut m = e
        .get_mut_by_id(c.id)
        .map_err(|_| invalid(format!("entity {entity} has no {}", c.decl.name)))?;
    // Marks the component changed for watchers and `Changed`.
    let ptr = m.as_mut();
    // SAFETY: `target` lies inside the component's `layout.size` bytes.
    unsafe { std::ptr::copy_nonoverlapping(bytes.as_ptr(), ptr.as_ptr().add(target.offset), bytes.len()) };
    Ok(())
}

pub(crate) fn mutate_components(In(p): In<Option<Value>>, world: &mut World) -> BrpResult {
    let raw = p.clone();
    let p: protocol::MutateComponentsParams = required(p)?;
    let entity = Entity::try_from_bits(p.entity.0).ok_or_else(|| invalid(format!("{} is not an entity", p.entity.0)))?;
    match ts_component(world, &p.component) {
        Some(c) => {
            write_ts(world, entity, &c, &p.path, &p.value)?;
            Ok(Value::Null)
        }
        None => process_remote_mutate_components_request(In(raw), world),
    }
}
