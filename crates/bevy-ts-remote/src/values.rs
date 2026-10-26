//! Component schemas, reads and edits: TS components from their native layout, reflected Rust
//! components through `bevy_ts::reflect_value`, in the shape TSX takes (docs/reflection.md).

use crate::{Subscribers, invalid, params, protocol, required};
use bevy::ecs::component::ComponentId;
use bevy::prelude::*;
use bevy::ecs::reflect::ReflectComponent;
use bevy::reflect::{ParsedPath, ReflectPath};
use bevy::remote::builtin_methods::export_registry_types;
use bevy::remote::{BrpError, BrpResult, error_codes};
use bevy_ts::components::{ComponentState, TsComponent, TsComponents};
use bevy_ts::layout::FieldType;
use bevy_ts::reflect_asset::{ScriptAssets, Slots};
use bevy_ts::value::{FieldValue, check_hints, decode_component, resolve_path};
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
        FieldType::Enum { variants, .. } => json!({ "type": "string", "enum": variants }),
        FieldType::Str => json!({ "type": "string" }),
        FieldType::List { of } => json!({ "type": "array", "items": field_schema(of, None) }),
        FieldType::Array { of, len } => json!({ "type": "array", "items": field_schema(of, None), "minItems": len, "maxItems": len }),
        FieldType::Struct { .. } => {
            let l = ty.struct_layout().unwrap();
            let properties: Map<String, Value> = l.fields.iter().map(|f| (f.name.clone(), field_schema(&f.ty, None))).collect();
            json!({ "type": "object", "properties": properties, "required": l.fields.iter().map(|f| &f.name).collect::<Vec<_>>(), "additionalProperties": false })
        }
        FieldType::Timer | FieldType::Stopwatch => {
            let l = ty.fields().unwrap();
            let properties: Map<String, Value> = l.fields.iter().map(|f| (f.name.clone(), field_schema(&f.ty, None))).collect();
            json!({ "type": "object", "title": ty.ts_name(), "properties": properties, "required": l.fields.iter().map(|f| &f.name).collect::<Vec<_>>() })
        }
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
        "x-bevy-ts-editor": crate::editor::ts(c),
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
            let native = crate::editor::native_crate(world);
            let registry = world.resource::<AppTypeRegistry>().read();
            for (path, mut s) in all {
                let short = s.get("shortPath").and_then(Value::as_str).unwrap_or_default().to_string();
                if matches(&p.names, &path, &short) {
                    if let Some(reg) = registry.get_with_type_path(&path) {
                        s["x-bevy-ts-editor"] = json!(crate::editor::reflected(reg, world, native.as_deref()));
                    }
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
    // SAFETY: a live column value of `layout`.
    Some(unsafe { decode_component(&c.layout, bytes) })
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
    for name in rust {
        match read_reflected(world, entity, &name) {
            Ok(v) => {
                components.insert(name, v);
            }
            Err(e) if p.strict => return Err(invalid(e)),
            Err(e) => {
                errors.insert(name, component_error(error_codes::COMPONENT_ERROR, e));
            }
        }
    }
    Ok(json!({ "components": components, "errors": errors }))
}

/// A reflected component's registration and `ReflectComponent`, by type path.
fn reflected<'r>(registry: &'r bevy::reflect::TypeRegistry, name: &str) -> Result<(&'r bevy::reflect::TypeRegistration, &'r ReflectComponent), String> {
    let reg = registry.get_with_type_path(name).ok_or_else(|| format!("unknown component {name}"))?;
    let rc = reg.data::<ReflectComponent>().ok_or_else(|| format!("{name} does not reflect Component"))?;
    Ok((reg, rc))
}

/// A reflected component of the entity through `reflect_value::read_component`.
fn read_reflected(world: &World, entity: Entity, name: &str) -> Result<Value, String> {
    let registry = world.resource::<AppTypeRegistry>().read();
    let (_, rc) = reflected(&registry, name)?;
    let e = world.get_entity(entity).map_err(|_| format!("{entity} is not an entity"))?;
    let value = rc.reflect(e).ok_or_else(|| format!("entity {entity} has no {name}"))?;
    // A handle a script made reads as its asset ref.
    let assets = world.get_resource::<ScriptAssets>().cloned().unwrap_or_default();
    let slots = Slots { registry: &registry, server: world.get_resource::<AssetServer>(), assets: &assets, world: Some(world), check: false };
    bevy_ts::reflect_value::read_component_with(&registry, Some(&slots), value.as_partial_reflect())
}

/// Writes `v` at the reflection path `path` of the entity's component, over the default's value
/// there (`write_component` for the whole component).
fn write_reflected(world: &mut World, entity: Entity, name: &str, path: &str, v: &Value) -> Result<(), BrpError> {
    bevy_ts::reflect_value::refuse_computed(name).map_err(invalid)?;
    let registry = world.resource::<AppTypeRegistry>().clone();
    let server = world.get_resource::<AssetServer>().cloned();
    let registry = registry.read();
    let (reg, rc) = reflected(&registry, name).map_err(invalid)?;
    let base = bevy_ts::reflect_value::default_of(&registry, reg.type_id());
    let parsed = (!path.is_empty()).then(|| ParsedPath::parse(path).map_err(|e| invalid(format!("{name}: path {path}: {e}")))).transpose()?;
    // Asset refs and values go in handle slots; a value's new asset goes in after the write.
    let assets = world.get_resource::<ScriptAssets>().cloned().unwrap_or_default();
    let slots = Slots { registry: &registry, server: server.as_ref(), assets: &assets, world: None, check: false };
    let wrote = {
        let mut e = world.get_entity_mut(entity).map_err(|_| BrpError::entity_not_found(entity))?;
        let mut m = rc.reflect_mut(&mut e).ok_or_else(|| invalid(format!("entity {entity} has no {name}")))?;
        let target = m.as_partial_reflect_mut();
        match &parsed {
            None => bevy_ts::reflect_value::write_component_with(slots.cx(), target, base.as_deref(), v),
            Some(at) => {
                let slot = at.reflect_element_mut(target).map_err(|e| invalid(format!("{name}: path {path}: {e}")))?;
                let b = base.as_deref().and_then(|b| at.reflect_element(b).ok());
                bevy_ts::reflect_value::write_with(slots.cx(), slot, b, v)
            }
        }
    };
    bevy_ts::reflect_asset::flush(world);
    wrote.map_err(|e| BrpError { code: error_codes::INVALID_PARAMS, message: format!("{name}{path}: {e}"), data: Some(json!({ "path": path })) })
}

/// What a `get_components+watch` subscriber was last sent, by params and [`crate::Turn::slot`].
#[derive(Default)]
struct WatchState {
    tick: u32,
    present: HashSet<String>,
}

#[derive(Resource, Default)]
struct ComponentWatches(HashMap<(String, usize), WatchState>);

fn component_id(world: &World, name: &str) -> Option<ComponentId> {
    if let Some(c) = world.resource::<TsComponents>().get(name) {
        return Some(c.id);
    }
    let registry = world.resource::<AppTypeRegistry>().read();
    let reg = registry.get_with_type_path(name)?;
    world.components().get_id(reg.type_id())
}

pub(crate) fn get_components_watch(In(p): In<Option<Value>>, world: &mut World) -> BrpResult<Option<Value>> {
    let turn = world.resource_mut::<Subscribers>().turn("get_components", &p);
    let fresh = turn.cursor.is_none();
    let key = (p.as_ref().map(Value::to_string).unwrap_or_default(), turn.slot);
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
    let check = FieldValue::encode(path, &target.ty, v).and_then(|fv| check_hints(path, &target.ty, &hints, v).map(|_| fv));
    let fv = check.map_err(|e| BrpError { code: error_codes::INVALID_PARAMS, message: e.to_string(), data: Some(json!({ "path": e.path, "expected": e.expected })) })?;
    let mut e = world.get_entity_mut(entity).map_err(|_| BrpError::entity_not_found(entity))?;
    let mut m = e
        .get_mut_by_id(c.id)
        .map_err(|_| invalid(format!("entity {entity} has no {}", c.decl.name)))?;
    if let Some(item) = &target.item {
        return write_item(m.bypass_change_detection().as_ptr(), item, path, v).map(|()| m.set_changed());
    }
    // Marks the component changed for watchers and `Changed`.
    let ptr = m.as_mut();
    // SAFETY: `target` lies inside the component's live value.
    unsafe { fv.write_at(ptr.as_ptr(), target.offset) };
    Ok(())
}

/// Sets one item of a heap list, or its member, in place (`.scores[i]`, `.hits[i].hp`): one
/// `set` edit, as an in-place edit from a script.
fn write_item(p: *mut u8, item: &bevy_ts::value::ListItem, path: &str, v: &Value) -> Result<(), BrpError> {
    let kind = bevy_ts::heap::HeapKind::of(&item.list).ok_or_else(|| invalid(format!("{path}: not a list")))?;
    // SAFETY: the slot holds a live list of `kind` (the component's layout).
    let old = bevy_ts::value::heap_json(&unsafe { bevy_ts::heap::read(p, item.slot, kind) });
    let len = old.as_array().map_or(0, Vec::len);
    let new = item.set(&old, v).ok_or_else(|| invalid(format!("{path}: [{}] is past the end: the list has {len} items", item.index)))?;
    let one = json!([new[item.index]]);
    let set = bevy_ts::value::heap_value(path, &item.list, &one).map_err(|e| invalid(e.to_string()))?;
    // SAFETY: as above; `apply` checks the index.
    unsafe { bevy_ts::heap::apply(p, item.slot, kind, vec![bevy_ts::heap::Edit::Set(item.index, set)]) }.map_err(|e| invalid(format!("{path}: {e}")))
}

pub(crate) fn mutate_components(In(p): In<Option<Value>>, world: &mut World) -> BrpResult {
    let p: protocol::MutateComponentsParams = required(p)?;
    let entity = Entity::try_from_bits(p.entity.0).ok_or_else(|| invalid(format!("{} is not an entity", p.entity.0)))?;
    match ts_component(world, &p.component) {
        Some(c) => {
            write_ts(world, entity, &c, &p.path, &p.value)?;
            Ok(Value::Null)
        }
        None => {
            write_reflected(world, entity, &p.component, &p.path, &p.value)?;
            Ok(Value::Null)
        }
    }
}
