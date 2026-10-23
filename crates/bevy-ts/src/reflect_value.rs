//! The one reflected-value writer: JSON written into a reflected slot, walked against the slot's
//! type. TSX props (`bevy-ts-tsx`'s reflected components and layout-tier holes) and recipe params
//! (`camera3d`'s `projection`) all write through [`write`].
//!
//! A written value is the JSON over a base, normally the slot's value in a fresh default of its
//! component: keys left out take the base's value, never the slot's previous one. Per slot type:
//! - `Color`: [`crate::color::parse`].
//! - `Handle<T>`: an asset path.
//! - `Option<T>`: `null` is `None`; anything else is `Some`, merged into the current `Some` or `T`'s
//!   default.
//! - An enum: a string names a unit variant; `{ Variant: data }` picks a variant with data. A newly
//!   chosen variant starts from its default (a newtype variant from its inner type's default); a
//!   struct or tuple variant with no default must be given whole.
//! - A struct given an object: each key merged into its field; an unknown key is an error.
//! - Anything else: read whole by reflection's deserializer.
//!
//! New slot kinds (lists, maps, tuples, `Entity`, `Duration`) are one more arm in [`merge`].

use crate::color;
use bevy::asset::{AssetServer, ReflectHandle};
use bevy::ecs::reflect::ReflectFromWorld;
use bevy::prelude::*;
use bevy::reflect::enums::{DynamicEnum, DynamicVariant, VariantInfo};
use bevy::reflect::serde::TypedReflectDeserializer;
use bevy::reflect::structs::DynamicStruct;
use bevy::reflect::tuple::DynamicTuple;
use bevy::reflect::{PartialReflect, ReflectMut, TypeInfo, TypeRegistry};
use serde::de::DeserializeSeed;
use serde_json::{Map, Value};
use std::any::TypeId;

/// What a write may reach: the registry, and the asset server for `Handle<T>` slots (`None` when
/// checking: a handle's path is then only checked to be a string).
#[derive(Clone, Copy)]
pub struct Cx<'a> {
    pub registry: &'a TypeRegistry,
    pub server: Option<&'a AssetServer>,
}

/// Writes `json` over `base` (the slot's own value when `None`) into `slot`.
pub fn write(registry: &TypeRegistry, server: Option<&AssetServer>, slot: &mut dyn PartialReflect, base: Option<&dyn PartialReflect>, json: &Value) -> Result<(), String> {
    let cx = Cx { registry, server };
    let mut value = copy(base.unwrap_or(slot))?;
    merge(cx, value.as_mut(), json)?;
    slot.try_apply(value.as_ref()).map_err(|e| format!("{}: {e}", type_path(slot)))
}

/// A value of a type to start from: its `ReflectDefault`, else its `ReflectFromWorld` run on an
/// empty world (a `FromWorld` that needs resources gives none).
pub fn default_of(registry: &TypeRegistry, id: TypeId) -> Option<Box<dyn PartialReflect>> {
    let reg = registry.get(id)?;
    if let Some(d) = reg.data::<ReflectDefault>() {
        return Some(d.default().into_partial_reflect());
    }
    let fw = reg.data::<ReflectFromWorld>()?.clone();
    std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| fw.from_world(&mut World::new()))).ok().map(<dyn Reflect>::into_partial_reflect)
}

fn type_path(v: &dyn PartialReflect) -> String {
    v.get_represented_type_info().map_or_else(|| v.reflect_type_path().to_string(), |i| i.type_path().to_string())
}

/// An owned copy to merge into: a concrete clone, else a dynamic one.
fn copy(v: &dyn PartialReflect) -> Result<Box<dyn PartialReflect>, String> {
    match v.reflect_clone() {
        Ok(c) => Ok(c.into_partial_reflect()),
        Err(_) => v.to_dynamic().map_err(|e| format!("{}: {e}", type_path(v))),
    }
}

/// Merges `json` into `node` in place.
fn merge(cx: Cx, node: &mut dyn PartialReflect, json: &Value) -> Result<(), String> {
    let info = node.get_represented_type_info().ok_or_else(|| format!("{}: no type info", node.reflect_type_path()))?;
    let path = info.type_path();
    if info.type_id() == TypeId::of::<Color>() {
        let c = color::parse(json)?;
        return node.try_apply(&c).map_err(|e| format!("{path}: {e}"));
    }
    let reg = cx.registry.get(info.type_id());
    if let Some(h) = reg.and_then(|r| r.data::<ReflectHandle>()) {
        let asset = json.as_str().ok_or_else(|| format!("{path}: expected an asset path, got {json}"))?;
        let Some(server) = cx.server else { return Ok(()) };
        let untyped = server.load_builder().load_erased(h.asset_type_id(), asset.to_string());
        return node.try_apply(h.typed(untyped).as_partial_reflect()).map_err(|e| format!("{path}: {e}"));
    }
    match (info, json) {
        (TypeInfo::Enum(_), Value::Null) if path.starts_with("core::option::Option<") => set_variant(cx, node, info, "None", None),
        (TypeInfo::Enum(_), _) if path.starts_with("core::option::Option<") => set_variant(cx, node, info, "Some", Some(json)),
        (TypeInfo::Enum(_), Value::String(v)) => set_variant(cx, node, info, v, None),
        (TypeInfo::Enum(_), Value::Object(o)) if o.len() == 1 => {
            let (v, data) = o.iter().next().expect("one key");
            set_variant(cx, node, info, v, Some(data))
        }
        (TypeInfo::Enum(e), _) => Err(format!("{path} is \"Variant\" or {{ Variant: ... }} (variants: {}), not {json}", e.variant_names().join(", "))),
        (TypeInfo::Struct(s), Value::Object(o)) => {
            let ReflectMut::Struct(st) = node.reflect_mut() else { unreachable!("a struct") };
            for (k, v) in o {
                let field = st.field_mut(k).ok_or_else(|| format!("{path} has no field {k} (fields: {})", s.field_names().join(", ")))?;
                merge(cx, field, v)?;
            }
            Ok(())
        }
        (TypeInfo::TupleStruct(t), v) if t.field_len() == 1 && !v.is_array() => {
            let ReflectMut::TupleStruct(ts) = node.reflect_mut() else { unreachable!("a tuple struct") };
            merge(cx, ts.field_mut(0).expect("one field"), v)
        }
        _ => {
            let reg = reg.ok_or_else(|| format!("{path} is not registered"))?;
            let value = TypedReflectDeserializer::new(reg, cx.registry).deserialize(json.clone()).map_err(|e| format!("{path}: {e}"))?;
            node.try_apply(value.as_ref()).map_err(|e| format!("{path}: {e}"))
        }
    }
}

/// The current variant's fields, copied, when `node` is variant `name`.
fn current_fields(node: &dyn PartialReflect, name: &str) -> Result<Option<Vec<Box<dyn PartialReflect>>>, String> {
    let bevy::reflect::ReflectRef::Enum(e) = node.reflect_ref() else { return Ok(None) };
    if e.variant_name() != name {
        return Ok(None);
    }
    (0..e.field_len()).map(|i| copy(e.field_at(i).expect("field"))).collect::<Result<Vec<_>, _>>().map(Some)
}

/// Switches `node` to variant `name` (keeping its fields when it already is that variant) and
/// merges `data` into the variant's fields.
fn set_variant(cx: Cx, node: &mut dyn PartialReflect, info: &TypeInfo, name: &str, data: Option<&Value>) -> Result<(), String> {
    let path = info.type_path();
    let TypeInfo::Enum(e) = info else { unreachable!("an enum") };
    let variant = e.variant(name).ok_or_else(|| format!("{path} has no variant {name} (variants: {})", e.variant_names().join(", ")))?;
    let current = current_fields(node, name)?;
    let dynamic = match variant {
        VariantInfo::Unit(_) => {
            if data.is_some_and(|d| !d.is_null() && d.as_object().is_none_or(|o| !o.is_empty())) {
                return Err(format!("{path}::{name} carries no data"));
            }
            DynamicVariant::Unit
        }
        VariantInfo::Tuple(t) => {
            let given: Vec<Value> = match data {
                // A newtype variant's data is its inner value.
                Some(d) if t.field_len() == 1 => vec![d.clone()],
                Some(Value::Array(a)) if a.len() == t.field_len() => a.clone(),
                None if t.field_len() == 0 => vec![],
                _ => return Err(format!("{path}::{name} takes {} values as an array", t.field_len())),
            };
            let mut fields = match current {
                Some(f) => f,
                None => match (0..t.field_len()).map(|i| default_of(cx.registry, t.field_at(i).expect("field").type_id())).collect::<Option<Vec<_>>>() {
                    Some(f) => f,
                    // No default: each value is read whole.
                    None => (0..t.field_len())
                        .map(|i| whole(cx, t.field_at(i).expect("field").type_id(), &given[i]))
                        .collect::<Result<Vec<_>, _>>()?,
                },
            };
            for (f, v) in fields.iter_mut().zip(&given) {
                merge(cx, f.as_mut(), v)?;
            }
            let mut tuple = DynamicTuple::default();
            for f in fields {
                tuple.insert_boxed(f);
            }
            DynamicVariant::Tuple(tuple)
        }
        VariantInfo::Struct(s) => {
            let empty = Map::new();
            let given = match data {
                Some(Value::Object(o)) => o,
                None => &empty,
                Some(d) => return Err(format!("{path}::{name} takes an object of its fields, not {d}")),
            };
            if let Some(k) = given.keys().find(|k| s.field(k).is_none()) {
                return Err(format!("{path}::{name} has no field {k} (fields: {})", s.field_names().join(", ")));
            }
            let fields = match current {
                Some(f) => f,
                None => {
                    // A newly chosen struct variant has no default of its own: it is given whole.
                    let missing: Vec<&str> = s.field_names().iter().copied().filter(|f| !given.contains_key(*f)).collect();
                    if !missing.is_empty() {
                        return Err(format!("{path}::{name} is given whole: missing {}", missing.join(", ")));
                    }
                    match s.iter().map(|f| default_of(cx.registry, f.type_id())).collect::<Option<Vec<_>>>() {
                        Some(d) => d,
                        None => s.iter().map(|f| whole(cx, f.type_id(), &given[f.name()])).collect::<Result<Vec<_>, _>>()?,
                    }
                }
            };
            let mut st = DynamicStruct::default();
            for (f, mut v) in s.iter().zip(fields) {
                if let Some(d) = given.get(f.name()) {
                    merge(cx, v.as_mut(), d)?;
                }
                st.insert_boxed(f.name(), v);
            }
            DynamicVariant::Struct(st)
        }
    };
    let value = DynamicEnum::new(name, dynamic);
    node.try_apply(&value).map_err(|e| format!("{path}::{name}: {e}"))
}

/// A value read whole by reflection's deserializer.
fn whole(cx: Cx, id: TypeId, json: &Value) -> Result<Box<dyn PartialReflect>, String> {
    let reg = cx.registry.get(id).ok_or("a field type that is not registered")?;
    TypedReflectDeserializer::new(reg, cx.registry).deserialize(json.clone()).map_err(|e| format!("{}: {e}", reg.type_info().type_path()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::camera::{OrthographicProjection, Projection, ScalingMode};
    use serde_json::json;

    #[derive(Reflect, Default, Debug, PartialEq)]
    #[reflect(Default)]
    struct Settings {
        viewport: Option<Rect>,
        tint: Color,
        level: f32,
    }

    fn registry() -> TypeRegistry {
        let mut r = TypeRegistry::new();
        r.register::<Projection>();
        r.register::<Settings>();
        r
    }

    fn written<T: Reflect + Default + FromReflect>(json: Value) -> Result<T, String> {
        let r = registry();
        let mut v = T::default();
        write(&r, None, &mut v, None, &json)?;
        Ok(v)
    }

    fn ortho(p: &Projection) -> &OrthographicProjection {
        match p {
            Projection::Orthographic(o) => o,
            other => panic!("not orthographic: {other:?}"),
        }
    }

    #[test]
    fn perspective_to_orthographic_with_one_field() {
        let p: Projection = written(json!({ "Orthographic": { "scale": 2 } })).unwrap();
        let o = ortho(&p);
        assert_eq!(o.scale, 2.0);
        let d = OrthographicProjection::default_3d();
        assert_eq!((o.near, o.far, o.viewport_origin), (d.near, d.far, d.viewport_origin));
    }

    #[test]
    fn nested_scaling_mode_unit_and_data_variants() {
        let p: Projection = written(json!({ "Orthographic": { "scaling_mode": { "FixedVertical": { "viewport_height": 12 } } } })).unwrap();
        assert!(matches!(ortho(&p).scaling_mode, ScalingMode::FixedVertical { viewport_height: 12.0 }));
        let p: Projection = written(json!({ "Orthographic": { "scaling_mode": "WindowSize" } })).unwrap();
        assert!(matches!(ortho(&p).scaling_mode, ScalingMode::WindowSize));
        // A struct variant that is not the default's must be given whole.
        let e = written::<Projection>(json!({ "Orthographic": { "scaling_mode": { "Fixed": { "width": 2 } } } })).unwrap_err();
        assert!(e.contains("Fixed") && e.contains("height"), "{e}");
    }

    #[test]
    fn options_take_null_and_a_value() {
        let s: Settings = written(json!({ "viewport": { "min": [0, 0], "max": [2, 1] } })).unwrap();
        assert_eq!(s.viewport, Some(Rect::new(0.0, 0.0, 2.0, 1.0)));
        let r = registry();
        let mut s = Settings { viewport: Some(Rect::default()), ..default() };
        write(&r, None, &mut s, None, &json!({ "viewport": null })).unwrap();
        assert_eq!(s.viewport, None);
    }

    #[test]
    fn colours_in_any_form() {
        let s: Settings = written(json!({ "tint": "#ff8000", "level": 1 })).unwrap();
        assert_eq!((s.tint.to_srgba().red, s.level), (1.0, 1.0));
        let s: Settings = written(json!({ "tint": { "Hsla": { "hue": 30, "saturation": 1, "lightness": 0.5 } } })).unwrap();
        assert!(matches!(s.tint, Color::Hsla(_)));
    }

    #[test]
    fn a_dropped_field_returns_to_its_default() {
        let r = registry();
        let mut p: Projection = written(json!({ "Orthographic": { "scale": 2 } })).unwrap();
        let base = Projection::default();
        // Written over the default's value, not the slot's: Perspective → Orthographic again.
        write(&r, None, &mut p, Some(&base), &json!({ "Orthographic": {} })).unwrap();
        assert_eq!(ortho(&p).scale, 1.0);
        let mut s = Settings { level: 5.0, ..default() };
        write(&r, None, &mut s, Some(&Settings::default()), &json!({ "tint": "#fff" })).unwrap();
        assert_eq!(s.level, 0.0);
    }

    #[test]
    fn typos_and_unknown_variants_are_named() {
        let e = written::<Projection>(json!({ "Orthographic": { "scael": 2 } })).unwrap_err();
        assert!(e.contains("scael") && e.contains("OrthographicProjection"), "{e}");
        let e = written::<Projection>(json!({ "Isometric": {} })).unwrap_err();
        assert!(e.contains("Projection") && e.contains("Perspective, Orthographic, Custom"), "{e}");
        let e = written::<Settings>(json!({ "levle": 1 })).unwrap_err();
        assert!(e.contains("levle") && e.contains("Settings"), "{e}");
    }
}
