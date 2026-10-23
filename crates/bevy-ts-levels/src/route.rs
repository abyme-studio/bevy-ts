//! The `bsn` route (viewport V1, design decisions 7 and 9): which level frame an edit to a field
//! of a level-placed entity goes to, and the write that sends it there. Viewport calls these and
//! drains [`crate::edit::WriteResults`].

use crate::compose::{Schema, ValueType, VariantFields};
use crate::edit::{WriteRequest, write_level};
use crate::engine::{BsnLevel, LevelInstance, Levels, WorldSchema};
use crate::writer::{Op, WValue};
use bevy::prelude::*;
use bevy_ts::layout::FieldType;
use bevy_ts_tsx::{Provenance, TsxNode};
use serde_json::{Value, json};

/// `levels.prefabEdits` (operator, 2026-10-19): where an edit inside a level-placed prefab goes.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum PrefabEdits {
    /// A per-instance override in the outermost level file; TSX only through "Apply to prefab".
    #[default]
    Override,
    /// A prefab-internal literal is rewritten in TSX by the editor; everything else overrides.
    Source,
}

impl PrefabEdits {
    fn name(self) -> &'static str {
        match self {
            PrefabEdits::Override => "override",
            PrefabEdits::Source => "source",
        }
    }
}

/// The entity's level, its key path in it, and the nearest level-made ancestor's frames.
fn level_of(world: &World, entity: Entity) -> Option<(String, Vec<String>, Option<&LevelInstance>, bool)> {
    let node = world.get::<TsxNode>(entity)?;
    let level = node.root.strip_prefix("level:")?.to_string();
    let own = world.get::<LevelInstance>(entity);
    let mut instance = own;
    let mut e = entity;
    while instance.is_none() {
        e = world.get::<ChildOf>(e)?.parent();
        instance = world.get::<LevelInstance>(e);
    }
    Some((level, node.key_path.clone(), instance, own.is_some()))
}

/// The `bsn` route of `component.field` on `entity`, as viewport's `EditRoute`; `None` when levels
/// has no say (not a level entity, a bound field, or a prefab literal under `source`), and the
/// viewport routes `tsx`, `locked` or `transient` as before.
pub fn route(world: &World, entity: Entity, component: &str, field: &str) -> Option<Value> {
    let (level, key_path, instance, level_made) = level_of(world, entity)?;
    let node = world.get::<TsxNode>(entity)?;
    let top = field.split('.').next().unwrap_or(field);
    let provenance = node.components.get(component).and_then(|c| c.fields.get(top)).map(|s| s.provenance);
    if provenance == Some(Provenance::Bound) {
        return None;
    }
    let policy = world.get_resource::<Levels>().map_or(PrefabEdits::Override, |l| l.prefab_edits);
    // Under `source`, a prefab-internal literal is the editor's; the instance root stays the level's.
    if policy == PrefabEdits::Source && !level_made && provenance == Some(Provenance::Controlled) {
        return None;
    }
    let owners: Vec<Value> = instance.map(|i| i.owners.iter().map(|o| json!({ "file": o.file, "namePath": o.name_path, "line": o.line })).collect()).unwrap_or_default();
    Some(json!({
        "kind": "bsn",
        "level": level,
        "instance": key_path.join("/"),
        "namePath": key_path,
        "path": format!("{component}.{field}"),
        "owners": owners,
        "target": { "file": level, "namePath": key_path },
        "policy": policy.name(),
    }))
}

/// The writer's value for `v` in a field of type `ty`.
fn wvalue(ty: &ValueType, v: &Value) -> Result<WValue, String> {
    Ok(match ty {
        ValueType::String | ValueType::Handle => WValue::Str(v.as_str().ok_or_else(|| format!("expected a string, got {v}"))?.to_string()),
        ValueType::Unsupported(t) => return Err(format!("{t} fields cannot be written to a level")),
        ValueType::Variants(variants) => {
            let (name, inner) = match v {
                Value::String(s) => (s.as_str(), None),
                Value::Object(o) if o.len() == 1 => o.iter().next().map(|(k, x)| (k.as_str(), Some(x))).expect("one entry"),
                _ => return Err(format!("expected a variant, got {v}")),
            };
            let schema = variants.iter().find(|x| x.name == name).ok_or_else(|| format!("unknown variant {name}"))?;
            match (&schema.fields, inner) {
                (VariantFields::Unit, None) => WValue::Path(name.to_string()),
                (VariantFields::Tuple(tys), Some(x)) => {
                    let values: Vec<&Value> = if tys.len() == 1 { vec![x] } else { x.as_array().ok_or_else(|| format!("{name}: expected {} values", tys.len()))?.iter().collect() };
                    if values.len() != tys.len() {
                        return Err(format!("{name}: expected {} values", tys.len()));
                    }
                    let fields = tys.iter().zip(values).map(|(t, x)| Ok(wvalue(t, x)?.print())).collect::<Result<Vec<_>, String>>()?;
                    WValue::Path(format!("{name}({})", fields.join(", ")))
                }
                (VariantFields::Named(tys), Some(Value::Object(o))) => {
                    let fields = tys
                        .iter()
                        .map(|(n, t)| Ok(format!("{n}: {}", wvalue(t, o.get(n).ok_or_else(|| format!("{name}: no field {n}"))?)?.print())))
                        .collect::<Result<Vec<_>, String>>()?;
                    WValue::Path(format!("{name} {{ {} }}", fields.join(", ")))
                }
                _ => return Err(format!("{name}: expected {v} to match its fields")),
            }
        }
        ValueType::Field(FieldType::F32) => WValue::Float { v: v.as_f64().ok_or("expected a number")?, f32: true },
        ValueType::Field(FieldType::F64) => WValue::Float { v: v.as_f64().ok_or("expected a number")?, f32: false },
        ValueType::Field(FieldType::Bool) => WValue::Bool(v.as_bool().ok_or("expected a bool")?),
        ValueType::Field(FieldType::Enum { .. }) => WValue::Path(v.as_str().ok_or("expected a variant name")?.to_string()),
        ValueType::Field(FieldType::Vec2 | FieldType::Vec3 | FieldType::Vec4 | FieldType::Quat | FieldType::Entity | FieldType::Array { .. }) => {
            return Err("a whole vector is written lane by lane".into());
        }
        ValueType::Field(FieldType::Timer | FieldType::Stopwatch) => return Err("a whole timer is written field by field".into()),
        ValueType::Field(_) => WValue::Int(v.as_f64().ok_or("expected an integer")? as i64),
    })
}

fn lanes(v: &Value) -> Option<Vec<f64>> {
    match v {
        Value::Array(a) => a.iter().map(Value::as_f64).collect(),
        Value::Object(o) => ["x", "y", "z", "w"].iter().map_while(|k| o.get(*k)).map(Value::as_f64).collect(),
        _ => None,
    }
}

fn vector_type(ty: &FieldType) -> Option<&'static str> {
    match ty {
        FieldType::Vec2 => Some("Vec2"),
        FieldType::Vec3 => Some("Vec3"),
        FieldType::Vec4 => Some("Vec4"),
        FieldType::Quat => Some("Quat"),
        _ => None,
    }
}

/// Writes `after` into `component.field` of `entity`'s level, as `set` operations on the lanes
/// that differ from what the entity has now: a drag along x writes `x` alone. The outcome
/// arrives in [`crate::edit::WriteResults`] under `edit_id`. `Err` is a refusal before any
/// write: not a level entity, or a field levels cannot write.
pub fn write(world: &mut World, edit_id: u64, entity: Entity, component: &str, field: &str, after: Value) -> Result<(), String> {
    write_from(world, edit_id, entity, component, field, None, after)
}

/// [`write`] given the value before the edit (the gizmo's start): only lanes that differ from it
/// are written, so a drag along x writes `x` even when a system had moved `y`.
pub fn write_from(world: &mut World, edit_id: u64, entity: Entity, component: &str, field: &str, before: Option<Value>, after: Value) -> Result<(), String> {
    let r = route(world, entity, component, field).ok_or("levels does not own this field")?;
    let file = r["target"]["file"].as_str().ok_or("no target")?.to_string();
    let path: Vec<String> = serde_json::from_value(r["target"]["namePath"].clone()).map_err(|e| e.to_string())?;
    let top = field.split('.').next().unwrap_or(field).to_string();
    let before = match before {
        Some(v) => serde_json::Map::from_iter([(top, v)]),
        None => world
            .get::<TsxNode>(entity)
            .and_then(|n| n.components.get(component))
            .map(|c| c.fields.iter().map(|(k, s)| (k.clone(), s.applied.clone())).collect::<serde_json::Map<_, _>>())
            .unwrap_or_default(),
    };
    let ops = set_ops(world, &path, component, field, &after, &before)?;
    if ops.is_empty() {
        return Ok(());
    }
    let base = loaded_hash(world, &file).ok_or("the level is not loaded")?;
    write_level(world, WriteRequest { id: edit_id, file, base, ops });
    Ok(())
}

/// The loaded asset's hash of a level file: what the world shows.
pub fn loaded_hash(world: &World, file: &str) -> Option<u64> {
    let server = world.resource::<AssetServer>();
    server.get_handle::<BsnLevel>(file.to_string()).and_then(|h| world.resource::<Assets<BsnLevel>>().get(&h).map(|a| a.hash))
}

/// `set` operations writing `after` into `component.field` at `path`: for a whole vector, one per
/// lane that differs from `before` (the component's current fields).
pub fn set_ops(world: &World, path: &[String], component: &str, field: &str, after: &Value, before: &serde_json::Map<String, Value>) -> Result<Vec<Op>, String> {
    let schema = {
        let registry = world.resource::<AppTypeRegistry>().read();
        WorldSchema::live(world, &registry).component(component)?
    };
    let mut segs: Vec<String> = field.split('.').map(String::from).collect();
    let top = segs[0].clone();
    let fs = schema.fields.iter().find(|f| f.name == top).ok_or_else(|| format!("{component} has no field {top}"))?;
    let path = path.to_vec();
    let mut ops = Vec::new();
    match &fs.ty {
        ValueType::Field(t) if vector_type(t).is_some() && segs.len() == 1 => {
            let ty = vector_type(t).expect("a vector");
            let new = lanes(after).ok_or_else(|| format!("expected a {ty}, got {after}"))?;
            let old = before.get(&top).and_then(lanes).unwrap_or_default();
            for (i, v) in new.iter().enumerate() {
                if old.get(i) != Some(v) {
                    let lane = ["x", "y", "z", "w"][i].to_string();
                    ops.push(Op::Set { path: path.clone(), component: component.into(), field: vec![top.clone(), lane], value: WValue::Float { v: *v, f32: true }, containers: vec![ty.into()] });
                }
            }
        }
        ValueType::Field(t) if vector_type(t).is_some() => {
            let ty = vector_type(t).expect("a vector");
            segs.truncate(2);
            ops.push(Op::Set { path, component: component.into(), field: segs, value: WValue::Float { v: after.as_f64().ok_or("expected a number")?, f32: true }, containers: vec![ty.into()] });
        }
        // A timer or stopwatch: one `set` per field that differs, inside `Timer { .. }`.
        ValueType::Field(t) if t.fields().is_some() => {
            let (layout, name) = (t.fields().expect("a struct"), t.ts_name());
            let mut set = |sub: &str, v: &Value| -> Result<(), String> {
                let f = layout.field(sub).ok_or_else(|| format!("{name} has no field {sub}"))?;
                let value = wvalue(&ValueType::Field(f.ty.clone()), v)?;
                ops.push(Op::Set { path: path.clone(), component: component.into(), field: vec![top.clone(), sub.into()], value, containers: vec![name.clone()] });
                Ok(())
            };
            if segs.len() == 1 {
                let new = after.as_object().ok_or_else(|| format!("expected a {name}, got {after}"))?;
                let old = before.get(&top).and_then(Value::as_object);
                for (k, v) in new {
                    if old.and_then(|o| o.get(k)) != Some(v) {
                        set(k, v)?;
                    }
                }
            } else {
                set(&segs[1], after)?;
            }
        }
        ty => ops.push(Op::Set { path, component: component.into(), field: vec![top], value: wvalue(ty, after)?, containers: Vec::new() }),
    }
    Ok(ops)
}

/// Writes an edit to an `app.scene` entity into the scene's sidecar level (decision 11), for an
/// edit the editor rejected as `computed` (no literal to rewrite). The sidecar
/// (`levels/<scene>.bsn`, or `app.scene`'s `level` option) is created on first write and layered
/// over the scene by key path. Vectors are written whole: an overlay has no prefab value below it
/// for missing lanes to come from.
pub fn write_sidecar(world: &mut World, edit_id: u64, entity: Entity, component: &str, field: &str, after: Value) -> Result<(), String> {
    let node = world.get::<TsxNode>(entity).ok_or("not a tsx entity")?;
    if node.root.starts_with("level:") {
        return Err("a level entity: use write".into());
    }
    let (scene, path) = (node.root.clone(), node.key_path.clone());
    if !world.contains_resource::<Levels>() {
        return Err("levels is not in this app".into());
    }
    let file = bevy_ts_tsx::scene_level(world, &scene).ok_or_else(|| format!("{scene} is not an app.scene root"))?;
    let ops = set_ops(world, &path, component, field, &after, &serde_json::Map::new())?;
    // The first write creates the file and opens it over the scene (`write_level`).
    let base = match loaded_hash(world, &file) {
        Some(h) => h,
        None => crate::engine::content_hash(std::fs::read_to_string(world.resource::<Levels>().asset_root.join(&file)).unwrap_or_default().as_bytes()),
    };
    write_level(world, WriteRequest { id: edit_id, file, base, ops });
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::compose::VariantSchema;

    #[test]
    fn variants_with_data_print_as_a_level_writes_them() {
        let unit = |n: &str| VariantSchema { name: n.into(), fields: VariantFields::Unit };
        let ty = ValueType::Variants(vec![
            unit("Never"),
            VariantSchema { name: "Count".into(), fields: VariantFields::Tuple(vec![ValueType::Field(FieldType::U32)]) },
            VariantSchema { name: "At".into(), fields: VariantFields::Named(vec![("x".into(), ValueType::Field(FieldType::F32))]) },
        ]);
        let print = |v: Value| wvalue(&ty, &v).map(|w| w.print());
        assert_eq!(print(json!({ "Count": 3 })).unwrap(), "Count(3)");
        assert_eq!(print(json!("Never")).unwrap(), "Never");
        assert_eq!(print(json!({ "At": { "x": 1.5 } })).unwrap(), "At { x: 1.5 }");
        assert!(print(json!({ "Twice": 2 })).unwrap_err().contains("unknown variant Twice"));
    }
}
