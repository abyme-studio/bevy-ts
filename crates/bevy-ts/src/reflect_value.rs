//! The one owner of a reflected value's JSON shape: [`read`] gives it, [`write`] takes it, and
//! writing what `read` gave over a default gives back an equal value. TSX props (`bevy-ts-tsx`'s
//! reflected components and layout-tier holes), the protocol's reflected reads and edits, and
//! script commands all go through here; `docs/reflection.md` lists every kind.
//!
//! A written value is the JSON over a base, normally the slot's value in a fresh default of its
//! component: keys left out take the base's value, never the slot's previous one. Per kind:
//! - `Color`: read in Bevy's shape (`{ Srgba: {...} }`); written in every [`crate::color::parse`]
//!   form.
//! - `Handle<T>`: its asset path, `null` for a handle with none; writing `null` keeps the base.
//! - `Option<T>`: `null` is `None`; anything else is `Some`, merged into the current `Some` or `T`'s
//!   default.
//! - An enum: a string names a unit variant; `{ Variant: data }` picks a variant with data (a
//!   newtype variant's data is its inner value, a tuple variant's an array). A newly chosen variant
//!   starts from its default; a struct or tuple variant with no default must be given whole.
//! - A struct: an object of its fields, written as a merge; an unknown key is an error.
//! - A newtype (one-field tuple struct): its inner value. A whole newtype component is
//!   `{ value }` ([`read_component`], [`write_component`]).
//! - Tuples, tuple structs and arrays: arrays, written element by element.
//! - Lists and sets: arrays (a set sorted by each element's JSON text); maps with string keys:
//!   objects; other maps: `[key, value]` pairs sorted like sets. All written whole.
//! - glam types: glam's serde shape (vectors as arrays of lanes, matrices flat, column-major).
//! - `Duration`: seconds; also written from `{ secs, nanos }`.
//! - 64- and 128-bit integers: a number within ±(2^53 - 1), else a decimal string; written from
//!   either. Non-finite floats: `"Infinity"`, `"-Infinity"`, `"NaN"`.
//! - `Entity`: `Entity::to_bits` as a number. `Name`: its string.
//! - Any other opaque type: its reflected serde.
//!
//! A type on [`UNCOVERED`] fails to read and to write with the list's reason; a struct field of
//! one reads as absent.

use crate::color;
use bevy::asset::{AssetServer, ReflectHandle, UntypedHandle};
use bevy::ecs::reflect::ReflectFromWorld;
use bevy::prelude::*;
use bevy::reflect::enums::{DynamicEnum, DynamicVariant, VariantInfo, VariantType};
use bevy::reflect::serde::{TypedReflectDeserializer, TypedReflectSerializer};
use bevy::reflect::structs::DynamicStruct;
use bevy::reflect::tuple::DynamicTuple;
use bevy::reflect::{PartialReflect, ReflectFromReflect, ReflectMut, ReflectRef, TypeInfo, TypeRegistry};
use serde::de::DeserializeSeed;
use serde_json::{Map, Value, json};
use std::any::{Any, TypeId};
use std::time::Duration;

/// The types `read` and `write` cannot cover, each with its reason: the only home of a gap. An
/// entry ending in `<` names every instance of a generic type. `tests/it/reflect_coverage.rs` fails
/// on a gap not listed here and on an entry no registered type uses.
pub const UNCOVERED: &[(&str, &str)] = &[
    ("core::any::TypeId", "a type id is not data, opaque with no reflected serde"),
    ("wgpu_types::BlendState", "opaque with no reflected serde"),
    ("bevy_camera::camera::CameraMainTextureUsages", "wgpu texture usages, opaque with no reflected serde"),
    ("bevy_render::camera::CameraRenderGraph", "an interned render graph label, opaque with no reflected serde"),
    ("bevy_a11y::AccessibilityRequested", "shared state behind an Arc<AtomicBool>, opaque with no reflected serde"),
    ("bevy_picking::events::PointerState", "press and click times are Instants, opaque with no reflected serde"),
    ("bevy_time::time::Time<bevy_time::real::Real>", "its clock readings are Instants, opaque with no reflected serde"),
    ("bevy_scene::scene_component::SceneComponentInfo", "computed by bevy_scene; holds a &'static str"),
    ("bevy_animation::graph::AnimationGraph", "its graph is a petgraph DiGraph, opaque with no default or reflected serde (building graphs is animation-control's)"),
    ("bevy_image::image::Image", "reflect-assets: its wgpu texture and view descriptors are opaque with no reflected serde"),
    ("bevy_render::storage::ShaderBuffer", "opaque with no reflected serde"),
    ("bevy_mesh::mesh::Mesh", "script-assets: no default; attributes and indices are reflect(ignore); engine-core builds meshes"),
];

/// Components Bevy computes: read like any other, never written from TSX or scripts.
pub const COMPUTED: &[&str] = &[
    "bevy_transform::components::global_transform::GlobalTransform",
    "bevy_ui::ui_node::ComputedNode",
    "bevy_text::pipeline::TextLayoutInfo",
];

/// The reason a type is on [`UNCOVERED`], if it is.
pub fn uncovered(type_path: &str) -> Option<&'static str> {
    UNCOVERED.iter().find(|(p, _)| *p == type_path || (p.ends_with('<') && type_path.starts_with(p))).map(|(_, why)| *why)
}

/// An error naming the component when Bevy computes it.
pub fn refuse_computed(type_path: &str) -> Result<(), String> {
    match COMPUTED.contains(&type_path) {
        true => Err(format!("{type_path} is computed by Bevy, so it cannot be written")),
        false => Ok(()),
    }
}

/// What a write may reach: the registry, the asset server for `Handle<T>` slots (`None` when
/// checking: a handle's path is then only checked to be a string), and the hook for handles given
/// as anything but a path.
#[derive(Clone, Copy)]
pub struct Cx<'a> {
    pub registry: &'a TypeRegistry,
    pub server: Option<&'a AssetServer>,
    pub handles: Option<&'a dyn HandleSlots>,
}

/// The hook for `Handle<T>` slots beyond a path (reflect-assets: asset refs, values and loads).
/// Without one, a handle is its path or `null`.
pub trait HandleSlots {
    /// A handle of asset type `asset` for JSON that is neither a string nor `null`.
    fn write(&self, asset: TypeId, json: &Value) -> Result<UntypedHandle, String>;
    /// A handle's JSON when the hook knows it; else `read` gives its path, or `null`.
    fn read(&self, handle: &UntypedHandle) -> Option<Value>;
}

/// The largest integer a JS number holds exactly.
const SAFE: i128 = (1 << 53) - 1;

/// Reads a reflected value as JSON in its kind's shape.
pub fn read(registry: &TypeRegistry, value: &dyn PartialReflect) -> Result<Value, String> {
    read_with(registry, None, value)
}

/// [`read`] with a [`HandleSlots`] hook.
pub fn read_with(registry: &TypeRegistry, handles: Option<&dyn HandleSlots>, value: &dyn PartialReflect) -> Result<Value, String> {
    let read = |registry: &TypeRegistry, v: &dyn PartialReflect| read_with(registry, handles, v);
    let path = type_path(value);
    if let Some(why) = uncovered(&path) {
        return Err(format!("{path} is not covered: {why}"));
    }
    if let Some(r) = value.try_as_reflect() {
        let any = r.as_any();
        if let Some(v) = read_scalar(any) {
            return Ok(v);
        }
        if any.is::<Color>() {
            return serialized(registry, value, &path);
        }
        if let Some(h) = registry.get_type_data::<ReflectHandle>(any.type_id()) {
            let handle = h.downcast_handle_untyped(any).ok_or_else(|| format!("{path}: not a handle"))?;
            if let Some(v) = handles.and_then(|hs| hs.read(&handle)) {
                return Ok(v);
            }
            return Ok(handle.path().map_or(Value::Null, |p| Value::String(p.to_string())));
        }
    }
    let items = |it: &mut dyn Iterator<Item = &dyn PartialReflect>| -> Result<Vec<Value>, String> {
        it.enumerate().map(|(i, v)| read(registry, v).map_err(|e| format!("[{i}]: {e}"))).collect()
    };
    match value.reflect_ref() {
        ReflectRef::Struct(_) | ReflectRef::TupleStruct(_) if path.starts_with("glam::") => serialized(registry, value, &path),
        ReflectRef::Struct(s) => {
            let mut out = Map::new();
            for i in 0..s.field_len() {
                let (name, f) = (s.name_at(i).expect("a field"), s.field_at(i).expect("a field"));
                match read(registry, f) {
                    Ok(v) => {
                        out.insert(name.to_string(), v);
                    }
                    // A field of an uncovered type is absent.
                    Err(_) if uncovered(&type_path(f)).is_some() => {}
                    Err(e) => return Err(format!("{name}: {e}")),
                }
            }
            Ok(Value::Object(out))
        }
        ReflectRef::TupleStruct(t) if t.field_len() == 1 => read(registry, t.field(0).expect("one field")),
        ReflectRef::TupleStruct(t) => items(&mut t.iter_fields()).map(Value::Array),
        ReflectRef::Tuple(t) => items(&mut t.iter_fields()).map(Value::Array),
        ReflectRef::Array(a) => items(&mut a.iter()).map(Value::Array),
        ReflectRef::List(l) => items(&mut l.iter()).map(Value::Array),
        ReflectRef::Set(s) => {
            let mut out = items(&mut s.iter())?;
            out.sort_by_cached_key(Value::to_string);
            Ok(Value::Array(out))
        }
        ReflectRef::Map(m) => {
            let keyed = match value.get_represented_type_info() {
                Some(TypeInfo::Map(info)) => string_key(info.key_ty().path()),
                _ => false,
            };
            let mut pairs = Vec::with_capacity(m.len());
            for (k, v) in m.iter() {
                let key = read(registry, k).map_err(|e| format!("a key: {e}"))?;
                let v = read(registry, v).map_err(|e| format!("[{key}]: {e}"))?;
                pairs.push((key, v));
            }
            if keyed {
                let mut out = Map::new();
                for (k, v) in pairs {
                    let Value::String(k) = k else { return Err(format!("{path}: a key that is not a string: {k}")) };
                    out.insert(k, v);
                }
                return Ok(Value::Object(out));
            }
            let mut out: Vec<Value> = pairs.into_iter().map(|(k, v)| json!([k, v])).collect();
            out.sort_by_cached_key(Value::to_string);
            Ok(Value::Array(out))
        }
        ReflectRef::Enum(e) => {
            let name = e.variant_name();
            if path.starts_with("core::option::Option<") {
                return match e.field_at(0) {
                    Some(inner) => read(registry, inner),
                    None => Ok(Value::Null),
                };
            }
            let data = match e.variant_type() {
                VariantType::Unit => return Ok(Value::String(name.to_string())),
                VariantType::Tuple if e.field_len() == 1 => read(registry, e.field_at(0).expect("one field"))?,
                VariantType::Tuple => Value::Array(items(&mut e.iter_fields().map(|f| f.value()))?),
                VariantType::Struct => {
                    let mut out = Map::new();
                    for f in e.iter_fields() {
                        let field = f.name().expect("a struct variant's field");
                        match read(registry, f.value()) {
                            Ok(v) => {
                                out.insert(field.to_string(), v);
                            }
                            Err(_) if uncovered(&type_path(f.value())).is_some() => {}
                            Err(err) => return Err(format!("{name}.{field}: {err}")),
                        }
                    }
                    Value::Object(out)
                }
            };
            Ok(json!({ name: data }))
        }
        ReflectRef::Opaque(_) => serialized(registry, value, &path),
        #[allow(unreachable_patterns)]
        _ => Err(format!("{path}: a kind with no JSON shape")),
    }
}

/// A whole component's JSON: [`read`], with a newtype as `{ value }`, the field TSX names.
pub fn read_component(registry: &TypeRegistry, value: &dyn PartialReflect) -> Result<Value, String> {
    let v = read(registry, value)?;
    Ok(if newtype(value) { json!({ "value": v }) } else { v })
}

/// [`write`] for a whole component: a newtype takes `{ value }` as [`read_component`] gives it.
pub fn write_component(registry: &TypeRegistry, server: Option<&AssetServer>, slot: &mut dyn PartialReflect, base: Option<&dyn PartialReflect>, json: &Value) -> Result<(), String> {
    let json = match json {
        Value::Object(o) if newtype(slot) && o.len() == 1 && o.contains_key("value") => &o["value"],
        _ => json,
    };
    write(registry, server, slot, base, json)
}

fn newtype(v: &dyn PartialReflect) -> bool {
    matches!(v.reflect_ref(), ReflectRef::TupleStruct(t) if t.field_len() == 1)
}

/// Map key types whose JSON is a string: such a map is an object.
pub fn string_key(path: &str) -> bool {
    matches!(path, "alloc::string::String" | "alloc::borrow::Cow<str>" | "&str" | "bevy_ecs::name::Name" | "smol_str::SmolStr")
}

/// A value by reflection's serializer: glam's serde shape, a colour in Bevy's, an opaque type's own.
fn serialized(registry: &TypeRegistry, value: &dyn PartialReflect, path: &str) -> Result<Value, String> {
    serde_json::to_value(TypedReflectSerializer::new(value, registry)).map_err(|e| format!("{path}: {e}"))
}

fn integer(x: i128) -> Value {
    if x.abs() <= SAFE { Value::from(x as i64) } else { Value::String(x.to_string()) }
}

fn float(x: f64) -> Value {
    match serde_json::Number::from_f64(x) {
        Some(n) => Value::Number(n),
        None if x.is_nan() => Value::from("NaN"),
        None if x > 0.0 => Value::from("Infinity"),
        None => Value::from("-Infinity"),
    }
}

/// Numbers, strings, `Duration`, `Entity` and `Name`, read without reflection.
fn read_scalar(any: &dyn Any) -> Option<Value> {
    macro_rules! direct {
        ($($t:ty),*) => { $( if let Some(x) = any.downcast_ref::<$t>() { return Some(Value::from(*x)); } )* };
    }
    macro_rules! wide {
        ($($t:ty),*) => { $( if let Some(x) = any.downcast_ref::<$t>() { return Some(integer(*x as i128)); } )* };
    }
    direct!(bool, i8, i16, i32, u8, u16, u32);
    wide!(i64, isize, u64, usize, i128);
    if let Some(x) = any.downcast_ref::<u128>() {
        return Some(if *x <= SAFE as u128 { Value::from(*x as u64) } else { Value::String(x.to_string()) });
    }
    if let Some(x) = any.downcast_ref::<f32>() {
        // The f64 of the f32 (`0.10000000149011612`), as a JS decoder of f32 lanes gives it.
        return Some(float(f64::from(*x)));
    }
    if let Some(x) = any.downcast_ref::<f64>() {
        return Some(float(*x));
    }
    if let Some(x) = any.downcast_ref::<String>() {
        return Some(Value::String(x.clone()));
    }
    if let Some(d) = any.downcast_ref::<Duration>() {
        return Some(float(d.as_secs_f64()));
    }
    if let Some(e) = any.downcast_ref::<Entity>() {
        return Some(Value::from(e.to_bits()));
    }
    if let Some(n) = any.downcast_ref::<Name>() {
        return Some(Value::String(n.as_str().to_string()));
    }
    // Numeric ranges are opaque with no serde in Bevy 0.20: `{ start, end }`.
    macro_rules! ranges {
        ($($t:ty),*) => { $(
            if let Some(r) = any.downcast_ref::<std::ops::Range<$t>>() {
                return Some(json!({ "start": read_scalar(&r.start)?, "end": read_scalar(&r.end)? }));
            }
            if let Some(r) = any.downcast_ref::<std::ops::RangeInclusive<$t>>() {
                return Some(json!({ "start": read_scalar(r.start())?, "end": read_scalar(r.end())? }));
            }
        )* };
    }
    ranges!(f32, f64, i8, i16, i32, i64, u8, u16, u32, u64, usize, isize);
    // A camera's exposure is opaque with no serde in Bevy 0.20: `{ ev100 }`.
    #[cfg(feature = "pbr")]
    if let Some(x) = any.downcast_ref::<bevy::camera::Exposure>() {
        return Some(json!({ "ev100": float(f64::from(x.ev100)) }));
    }
    None
}

/// The value a scalar slot of type `id` takes from `json`, when `id` is one `read_scalar` reads;
/// `None` for every other type, and for a `Duration` given `{ secs, nanos }` (read by serde).
fn write_scalar(id: TypeId, json: &Value, path: &str) -> Result<Option<Box<dyn PartialReflect>>, String> {
    macro_rules! int {
        ($($t:ty),*) => { $( if id == TypeId::of::<$t>() {
            let x: Option<$t> = match json {
                Value::Number(n) => n.as_i64().map(i128::from).or_else(|| n.as_u64().map(i128::from)).and_then(|i| <$t>::try_from(i).ok()),
                Value::String(s) => s.parse::<$t>().ok(),
                _ => None,
            };
            let x = x.ok_or_else(|| format!("{path} is an integer in range (a number, or a decimal string past 2^53), not {json}"))?;
            return Ok(Some(Box::new(x)));
        } )* };
    }
    int!(i8, i16, i32, i64, i128, isize, u8, u16, u32, u64, u128, usize);
    let number = || -> Result<f64, String> {
        match json {
            Value::Number(n) => Ok(n.as_f64().expect("a JSON number")),
            Value::String(s) if s == "Infinity" => Ok(f64::INFINITY),
            Value::String(s) if s == "-Infinity" => Ok(f64::NEG_INFINITY),
            Value::String(s) if s == "NaN" => Ok(f64::NAN),
            _ => Err(format!("{path} is a number, not {json}")),
        }
    };
    if id == TypeId::of::<f32>() {
        return Ok(Some(Box::new(number()? as f32)));
    }
    if id == TypeId::of::<f64>() {
        return Ok(Some(Box::new(number()?)));
    }
    if id == TypeId::of::<Duration>() {
        let Value::Number(n) = json else { return Ok(None) };
        let secs = n.as_f64().expect("a JSON number");
        let d = Duration::try_from_secs_f64(secs).map_err(|_| format!("{path} is seconds, zero or more, not {json}"))?;
        return Ok(Some(Box::new(d)));
    }
    if id == TypeId::of::<Entity>() {
        let e = json.as_u64().and_then(Entity::try_from_bits).ok_or_else(|| format!("{path} is an entity, not {json}"))?;
        return Ok(Some(Box::new(e)));
    }
    if id == TypeId::of::<Name>() {
        let s = json.as_str().ok_or_else(|| format!("{path} is a string, not {json}"))?;
        return Ok(Some(Box::new(Name::new(s.to_string()))));
    }
    macro_rules! ranges {
        ($($t:ty),*) => { $(
            let (exclusive, inclusive) = (id == TypeId::of::<std::ops::Range<$t>>(), id == TypeId::of::<std::ops::RangeInclusive<$t>>());
            if exclusive || inclusive {
                let end = |k: &str| -> Result<$t, String> {
                    let v = json.get(k).ok_or_else(|| format!("{path} is {{ start, end }}, not {json}"))?;
                    let b = write_scalar(TypeId::of::<$t>(), v, path)?.expect("a number type");
                    Ok(*b.try_downcast::<$t>().map_err(|_| format!("{path}: not a number"))?)
                };
                let (start, stop) = (end("start")?, end("end")?);
                return Ok(Some(if exclusive { Box::new(start..stop) } else { Box::new(start..=stop) }));
            }
        )* };
    }
    ranges!(f32, f64, i8, i16, i32, i64, u8, u16, u32, u64, usize, isize);
    #[cfg(feature = "pbr")]
    if id == TypeId::of::<bevy::camera::Exposure>() {
        let ev100 = match json {
            Value::Object(o) if o.keys().all(|k| k == "ev100") => match o.get("ev100") {
                Some(v) => *write_scalar(TypeId::of::<f32>(), v, path)?.expect("f32").try_downcast::<f32>().map_err(|_| format!("{path}: not a number"))?,
                None => bevy::camera::Exposure::default().ev100,
            },
            _ => return Err(format!("{path} is {{ ev100 }}, not {json}")),
        };
        return Ok(Some(Box::new(bevy::camera::Exposure { ev100 })));
    }
    Ok(None)
}

/// Writes `json` over `base` (the slot's own value when `None`) into `slot`.
pub fn write(registry: &TypeRegistry, server: Option<&AssetServer>, slot: &mut dyn PartialReflect, base: Option<&dyn PartialReflect>, json: &Value) -> Result<(), String> {
    write_with(Cx { registry, server, handles: None }, slot, base, json)
}

/// [`write`] with everything a [`Cx`] carries.
pub fn write_with(cx: Cx, slot: &mut dyn PartialReflect, base: Option<&dyn PartialReflect>, json: &Value) -> Result<(), String> {
    let mut value = copy(base.unwrap_or(slot))?;
    merge(cx, value.as_mut(), json)?;
    replace(cx, slot, value)
}

/// Sets `slot` to `value` whole, so a list written shorter loses its tail (`try_apply` would keep
/// it). A dynamic value is made concrete first; one that cannot be is applied.
fn replace(cx: Cx, slot: &mut dyn PartialReflect, value: Box<dyn PartialReflect>) -> Result<(), String> {
    let path = type_path(slot);
    let value = match value.try_into_reflect() {
        Ok(v) => v,
        Err(v) => {
            let concrete = slot.get_represented_type_info().and_then(|i| cx.registry.get_type_data::<ReflectFromReflect>(i.type_id())).and_then(|f| f.from_reflect(v.as_ref()));
            match concrete {
                Some(c) => c,
                None => return slot.try_apply(v.as_ref()).map_err(|e| format!("{path}: {e}")),
            }
        }
    };
    let value = match slot.try_as_reflect_mut() {
        Some(s) => match s.set(value) {
            Ok(()) => return Ok(()),
            // A `reflect_remote` wrapper and its remote type share a type path but not a type id.
            Err(v) => v,
        },
        None => value,
    };
    slot.try_apply(value.as_partial_reflect()).map_err(|e| format!("{path}: {e}"))
}

/// A value of a type to start from: its `ReflectDefault`, else its `ReflectFromWorld` run on an
/// empty world (a `FromWorld` that needs resources gives none).
pub fn default_of(registry: &TypeRegistry, id: TypeId) -> Option<Box<dyn PartialReflect>> {
    if id == TypeId::of::<Entity>() {
        return Some(Box::new(Entity::PLACEHOLDER));
    }
    // Numeric ranges reflect with no `Default`: `0..0`.
    macro_rules! ranges {
        ($($t:ty),*) => { $(
            if id == TypeId::of::<std::ops::Range<$t>>() {
                return Some(Box::new(<std::ops::Range<$t>>::default()));
            }
            if id == TypeId::of::<std::ops::RangeInclusive<$t>>() {
                return Some(Box::new(<$t>::default()..=<$t>::default()));
            }
        )* };
    }
    ranges!(f32, f64, i8, i16, i32, i64, u8, u16, u32, u64, usize, isize);
    let reg = registry.get(id)?;
    if let Some(h) = reg.data::<ReflectHandle>() {
        // `Handle::default()`: the asset type's default UUID handle.
        let untyped = UntypedHandle::Uuid { type_id: h.asset_type_id(), uuid: bevy::asset::AssetId::<bevy::asset::LoadedFolder>::DEFAULT_UUID };
        return Some(h.typed(untyped).into_partial_reflect());
    }
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

/// A new value of type `id` from `json`: over its default, or read whole when it has none.
fn fresh(cx: Cx, id: TypeId, json: &Value) -> Result<Box<dyn PartialReflect>, String> {
    match default_of(cx.registry, id) {
        Some(mut v) => {
            merge(cx, v.as_mut(), json)?;
            Ok(v)
        }
        None => whole(cx, id, json),
    }
}

/// A new value of type `id` from `json`, as a list item or map entry is made.
pub fn value_of(registry: &TypeRegistry, id: TypeId, json: &Value) -> Result<Box<dyn PartialReflect>, String> {
    fresh(Cx { registry, server: None, handles: None }, id, json)
}

/// Merges `json` into `node` in place.
fn merge(cx: Cx, node: &mut dyn PartialReflect, json: &Value) -> Result<(), String> {
    let info = node.get_represented_type_info().ok_or_else(|| format!("{}: no type info", node.reflect_type_path()))?;
    let path = info.type_path();
    if let Some(why) = uncovered(path) {
        return Err(format!("{path} is not covered: {why}"));
    }
    if info.type_id() == TypeId::of::<Color>() {
        let c = color::parse(json)?;
        return node.try_apply(&c).map_err(|e| format!("{path}: {e}"));
    }
    if let Some(v) = write_scalar(info.type_id(), json, path)? {
        return node.try_apply(v.as_ref()).map_err(|e| format!("{path}: {e}"));
    }
    let reg = cx.registry.get(info.type_id());
    if let Some(h) = reg.and_then(|r| r.data::<ReflectHandle>()) {
        if json.is_null() {
            // No path: the base's handle stays.
            return Ok(());
        }
        let Some(asset) = json.as_str() else {
            let hook = cx.handles.ok_or_else(|| format!("{path}: expected an asset path, got {json}"))?;
            let untyped = hook.write(h.asset_type_id(), json).map_err(|e| format!("{path}: {e}"))?;
            return node.try_apply(h.typed(untyped).as_partial_reflect()).map_err(|e| format!("{path}: {e}"));
        };
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
                merge(cx, field, v).map_err(|e| format!("{k}: {e}"))?;
            }
            Ok(())
        }
        (TypeInfo::TupleStruct(t), v) if t.field_len() == 1 => {
            let ReflectMut::TupleStruct(ts) = node.reflect_mut() else { unreachable!("a tuple struct") };
            merge(cx, ts.field_mut(0).expect("one field"), v)
        }
        (TypeInfo::TupleStruct(_) | TypeInfo::Tuple(_) | TypeInfo::Array(_), Value::Array(a)) => {
            let len = match node.reflect_ref() {
                ReflectRef::TupleStruct(t) => t.field_len(),
                ReflectRef::Tuple(t) => t.field_len(),
                ReflectRef::Array(x) => x.len(),
                _ => unreachable!("a tuple or array"),
            };
            if a.len() != len {
                return Err(format!("{path} takes an array of {len}, not {json}"));
            }
            for (i, v) in a.iter().enumerate() {
                let element = match node.reflect_mut() {
                    ReflectMut::TupleStruct(t) => t.field_mut(i),
                    ReflectMut::Tuple(t) => t.field_mut(i),
                    ReflectMut::Array(x) => x.get_mut(i),
                    _ => None,
                };
                merge(cx, element.expect("an element"), v).map_err(|e| format!("[{i}]: {e}"))?;
            }
            Ok(())
        }
        (TypeInfo::List(l), Value::Array(a)) => {
            let items = a.iter().enumerate().map(|(i, v)| fresh(cx, l.item_ty().id(), v).map_err(|e| format!("[{i}]: {e}"))).collect::<Result<Vec<_>, _>>()?;
            let ReflectMut::List(list) = node.reflect_mut() else { unreachable!("a list") };
            list.drain();
            for item in items {
                list.push(item);
            }
            Ok(())
        }
        (TypeInfo::Set(s), Value::Array(a)) => {
            let items = a.iter().enumerate().map(|(i, v)| fresh(cx, s.value_ty().id(), v).map_err(|e| format!("[{i}]: {e}"))).collect::<Result<Vec<_>, _>>()?;
            let ReflectMut::Set(set) = node.reflect_mut() else { unreachable!("a set") };
            set.drain();
            for item in items {
                set.insert_boxed(item);
            }
            Ok(())
        }
        (TypeInfo::Map(m), Value::Object(_) | Value::Array(_)) => {
            let given: Vec<(Value, &Value)> = match json {
                Value::Object(o) if string_key(m.key_ty().path()) => o.iter().map(|(k, v)| (Value::String(k.clone()), v)).collect(),
                Value::Array(a) => a
                    .iter()
                    .map(|p| match p.as_array().map(Vec::as_slice) {
                        Some([k, v]) => Ok((k.clone(), v)),
                        _ => Err(format!("{path} takes [key, value] pairs, not {p}")),
                    })
                    .collect::<Result<_, _>>()?,
                _ => return Err(format!("{path} has keys of {}, so it takes [key, value] pairs, not {json}", m.key_ty().path())),
            };
            let mut pairs = Vec::with_capacity(given.len());
            for (k, v) in given {
                let key = fresh(cx, m.key_ty().id(), &k).map_err(|e| format!("a key {k}: {e}"))?;
                let value = fresh(cx, m.value_ty().id(), v).map_err(|e| format!("[{k}]: {e}"))?;
                pairs.push((key, value));
            }
            let ReflectMut::Map(map) = node.reflect_mut() else { unreachable!("a map") };
            map.drain();
            for (k, v) in pairs {
                map.insert_boxed(k, v);
            }
            Ok(())
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

/// A value read whole, for a type with no default: built from its type info and merged, so it
/// takes this module's shapes at every depth; a scalar's own form; else (an opaque type with no
/// default) reflection's deserializer.
fn whole(cx: Cx, id: TypeId, json: &Value) -> Result<Box<dyn PartialReflect>, String> {
    let reg = cx.registry.get(id).ok_or("a field type that is not registered")?;
    let path = reg.type_info().type_path();
    if let Some(v) = write_scalar(id, json, path)? {
        return Ok(v);
    }
    if let Some(mut v) = blank(cx.registry, id) {
        merge(cx, v.as_mut(), json)?;
        return Ok(match reg.data::<ReflectFromReflect>().and_then(|f| f.from_reflect(v.as_ref())) {
            Some(c) => c.into_partial_reflect(),
            None => v,
        });
    }
    TypedReflectDeserializer::new(reg, cx.registry).deserialize(json.clone()).map_err(|e| format!("{path}: {e}"))
}

/// A value of a type to merge into: its default, else one built from its type info (structs of
/// their fields' values, an enum's first variant, empty collections), made concrete when the type
/// can be. `None` for an opaque type with no default.
pub fn blank(registry: &TypeRegistry, id: TypeId) -> Option<Box<dyn PartialReflect>> {
    blank_at(registry, id, 0)
}

fn blank_at(registry: &TypeRegistry, id: TypeId, depth: usize) -> Option<Box<dyn PartialReflect>> {
    use bevy::reflect::array::DynamicArray;
    use bevy::reflect::list::DynamicList;
    use bevy::reflect::map::DynamicMap;
    use bevy::reflect::set::DynamicSet;
    use bevy::reflect::tuple_struct::DynamicTupleStruct;
    if let Some(v) = default_of(registry, id) {
        return Some(v);
    }
    if depth > 16 {
        return None;
    }
    let reg = registry.get(id)?;
    let info = reg.type_info();
    let field = |id: TypeId| blank_at(registry, id, depth + 1);
    let dynamic: Box<dyn PartialReflect> = match info {
        TypeInfo::Struct(s) => {
            let mut d = DynamicStruct::default();
            for f in s.iter() {
                d.insert_boxed(f.name(), field(f.type_id())?);
            }
            d.set_represented_type(Some(info));
            Box::new(d)
        }
        TypeInfo::TupleStruct(s) => {
            let mut d = DynamicTupleStruct::default();
            for f in s.iter() {
                d.insert_boxed(field(f.type_id())?);
            }
            d.set_represented_type(Some(info));
            Box::new(d)
        }
        TypeInfo::Tuple(t) => {
            let mut d = DynamicTuple::default();
            for f in t.iter() {
                d.insert_boxed(field(f.type_id())?);
            }
            d.set_represented_type(Some(info));
            Box::new(d)
        }
        TypeInfo::Enum(e) => {
            let v = e.variant_at(0)?;
            let variant = match v {
                VariantInfo::Unit(_) => DynamicVariant::Unit,
                VariantInfo::Tuple(t) => {
                    let mut d = DynamicTuple::default();
                    for f in t.iter() {
                        d.insert_boxed(field(f.type_id())?);
                    }
                    DynamicVariant::Tuple(d)
                }
                VariantInfo::Struct(s) => {
                    let mut d = DynamicStruct::default();
                    for f in s.iter() {
                        d.insert_boxed(f.name(), field(f.type_id())?);
                    }
                    DynamicVariant::Struct(d)
                }
            };
            let mut d = DynamicEnum::new(v.name(), variant);
            d.set_represented_type(Some(info));
            Box::new(d)
        }
        TypeInfo::Array(a) => {
            let items: Vec<Box<dyn PartialReflect>> = (0..a.capacity()).map(|_| field(a.item_ty().id())).collect::<Option<_>>()?;
            let mut d = DynamicArray::from_iter(items);
            d.set_represented_type(Some(info));
            Box::new(d)
        }
        TypeInfo::List(_) => {
            let mut d = DynamicList::default();
            d.set_represented_type(Some(info));
            Box::new(d)
        }
        TypeInfo::Set(_) => {
            let mut d = DynamicSet::default();
            d.set_represented_type(Some(info));
            Box::new(d)
        }
        TypeInfo::Map(_) => {
            let mut d = DynamicMap::default();
            d.set_represented_type(Some(info));
            Box::new(d)
        }
        _ => return None,
    };
    match reg.data::<ReflectFromReflect>().and_then(|f| f.from_reflect(dynamic.as_ref())) {
        Some(c) => Some(c.into_partial_reflect()),
        None => Some(dynamic),
    }
}

// The tests use Bevy's camera and render types.
#[cfg(all(test, feature = "pbr"))]
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

    #[derive(Reflect, Default, Debug, Clone, PartialEq)]
    #[reflect(Default)]
    struct Inner {
        a: f32,
        b: Option<u8>,
    }

    #[derive(Reflect, Default, Debug, Clone, PartialEq)]
    #[reflect(Default)]
    struct Meters(f32);

    #[derive(Reflect, Default, Debug, Clone, PartialEq)]
    #[reflect(Default)]
    struct Pair(i32, bool);

    #[derive(Reflect, Default, Debug, Clone, PartialEq)]
    #[reflect(Default)]
    enum Shape {
        #[default]
        Empty,
        Radius(f32),
        Span(f32, f32),
        Box { w: f32, h: f32 },
    }

    /// One field of every kind `read` and `write` own.
    #[derive(Reflect, Default, Debug, Clone, PartialEq)]
    #[reflect(Default)]
    struct Every {
        inner: Inner,
        meters: Meters,
        pair: Pair,
        tuple: (u8, String),
        array: [f32; 3],
        list: Vec<String>,
        deque: std::collections::VecDeque<u16>,
        set: bevy::platform::collections::HashSet<u32>,
        names: bevy::platform::collections::HashMap<String, u32>,
        by_entity: bevy::platform::collections::HashMap<Entity, f32>,
        btree: std::collections::BTreeMap<u8, bool>,
        unit: Shape,
        newtype_variant: Shape,
        tuple_variant: Shape,
        struct_variant: Shape,
        option: Option<Inner>,
        none: Option<u32>,
        tint: Color,
        vec3: Vec3,
        ivec2: IVec2,
        bvec: BVec3,
        dvec: bevy::math::DVec2,
        quat: Quat,
        mat: Mat4,
        affine: bevy::math::Affine2,
        dir: Option<Dir3>,
        rot: Rot2,
        iso: Isometry3d,
        cooldown: Duration,
        big: u64,
        signed: i64,
        huge: u128,
        index: usize,
        entity: Option<Entity>,
        name: String,
        letter: char,
        far: f32,
        span: std::ops::Range<f32>,
        closed: Option<std::ops::RangeInclusive<u32>>,
    }

    fn every() -> Every {
        Every {
            inner: Inner { a: 0.5, b: Some(3) },
            meters: Meters(2.5),
            pair: Pair(-4, true),
            tuple: (7, "seven".into()),
            array: [1.0, 2.0, 3.0],
            list: vec!["a".into(), "b".into()],
            deque: [3, 1, 2].into_iter().collect(),
            set: [5, 1, 9].into_iter().collect(),
            names: [("one".to_string(), 1), ("two".to_string(), 2)].into_iter().collect(),
            by_entity: [(Entity::from_raw_u32(3).unwrap(), 0.25), (Entity::from_raw_u32(1).unwrap(), 1.5)].into_iter().collect(),
            btree: [(2, true), (1, false)].into_iter().collect(),
            unit: Shape::Empty,
            newtype_variant: Shape::Radius(4.0),
            tuple_variant: Shape::Span(1.0, 2.0),
            struct_variant: Shape::Box { w: 3.0, h: 0.5 },
            option: Some(Inner { a: 1.0, b: None }),
            none: None,
            tint: Color::hsla(30.0, 1.0, 0.5, 0.75),
            vec3: Vec3::new(1.0, -2.0, 3.5),
            ivec2: IVec2::new(-3, 8),
            bvec: BVec3::new(true, false, true),
            dvec: bevy::math::DVec2::new(0.1, 1e300),
            quat: Quat::from_rotation_y(0.7),
            mat: Mat4::from_scale_rotation_translation(Vec3::splat(2.0), Quat::from_rotation_x(0.3), Vec3::new(1.0, 2.0, 3.0)),
            affine: bevy::math::Affine2::from_angle_translation(0.4, Vec2::new(5.0, 6.0)),
            dir: Some(Dir3::new(Vec3::new(1.0, 1.0, 0.0)).unwrap()),
            rot: Rot2::degrees(30.0),
            iso: Isometry3d::new(Vec3::new(1.0, 0.0, 2.0), Quat::from_rotation_z(1.0)),
            cooldown: Duration::from_millis(1500),
            big: 1 << 60,
            signed: -(1 << 55),
            huge: u128::MAX,
            index: 42,
            entity: Some(Entity::from_raw_u32(17).unwrap()),
            name: "laser".into(),
            letter: 'x',
            far: f32::INFINITY,
            span: 0.25..0.75,
            closed: Some(2..=9),
        }
    }

    fn every_registry() -> TypeRegistry {
        let mut r = TypeRegistry::default();
        r.register::<Every>();
        r
    }

    #[test]
    fn every_kind_round_trips() {
        let r = every_registry();
        let original = every();
        let json = read(&r, &original).unwrap();
        let mut back = Every::default();
        write(&r, None, &mut back, None, &json).unwrap();
        assert_eq!(back, original, "{json:#}");
        // `Range` reflects without PartialEq, so reflection cannot compare the whole: never unequal.
        assert_ne!(back.reflect_partial_eq(&original), Some(false));
        // Reading again gives the same text: sets and keyed maps are ordered.
        assert_eq!(read(&r, &back).unwrap().to_string(), json.to_string());
    }

    #[test]
    fn shapes_of_each_kind() {
        let r = every_registry();
        let j = read(&r, &every()).unwrap();
        assert_eq!(j["inner"], json!({ "a": 0.5, "b": 3 }));
        assert_eq!(j["meters"], json!(2.5));
        assert_eq!(j["pair"], json!([-4, true]));
        assert_eq!(j["tuple"], json!([7, "seven"]));
        assert_eq!(j["deque"], json!([3, 1, 2]));
        assert_eq!(j["set"], json!([1, 5, 9]));
        assert_eq!(j["names"], json!({ "one": 1, "two": 2 }));
        assert_eq!(j["btree"], json!([[1, false], [2, true]]));
        assert_eq!(j["unit"], json!("Empty"));
        assert_eq!(j["newtype_variant"], json!({ "Radius": 4.0 }));
        assert_eq!(j["tuple_variant"], json!({ "Span": [1.0, 2.0] }));
        assert_eq!(j["struct_variant"], json!({ "Box": { "w": 3.0, "h": 0.5 } }));
        assert_eq!(j["option"], json!({ "a": 1.0, "b": null }));
        assert_eq!(j["none"], json!(null));
        assert_eq!(j["tint"], json!({ "Hsla": { "hue": 30.0, "saturation": 1.0, "lightness": 0.5, "alpha": 0.75 } }));
        assert_eq!(j["vec3"], json!([1.0, -2.0, 3.5]));
        assert_eq!(j["ivec2"], json!([-3, 8]));
        assert_eq!(j["mat"].as_array().map(Vec::len), Some(16));
        assert_eq!(j["cooldown"], json!(1.5));
        assert_eq!(j["big"], json!("1152921504606846976"));
        assert_eq!(j["signed"], json!("-36028797018963968"));
        assert_eq!(j["index"], json!(42));
        assert_eq!(j["entity"], json!(Entity::from_raw_u32(17).unwrap().to_bits()));
        assert_eq!(j["far"], json!("Infinity"));
        assert_eq!(j["span"], json!({ "start": 0.25, "end": 0.75 }));
        assert_eq!(j["closed"], json!({ "start": 2, "end": 9 }));
    }

    #[test]
    fn big_integers_from_strings_and_numbers() {
        let r = every_registry();
        let mut v = Every::default();
        write(&r, None, &mut v, None, &json!({ "big": "1152921504606846976", "index": 7, "signed": -5 })).unwrap();
        assert_eq!((v.big, v.index, v.signed), (1 << 60, 7, -5));
        write(&r, None, &mut v, None, &json!({ "big": 9007199254740991u64 })).unwrap();
        assert_eq!(v.big, 9007199254740991);
        let e = write(&r, None, &mut v, None, &json!({ "big": -1 })).unwrap_err();
        assert!(e.contains("big") && e.contains("u64"), "{e}");
    }

    #[test]
    fn a_list_is_replaced_not_merged() {
        let r = every_registry();
        let mut v = Every { list: vec!["a".into(), "b".into()], ..default() };
        write(&r, None, &mut v, None, &json!({ "list": ["c"] })).unwrap();
        assert_eq!(v.list, vec!["c".to_string()]);
        // Over the slot's own value too: the list is still whole.
        let mut list = vec!["a".to_string(), "b".to_string()];
        write(&r, None, &mut list, None, &json!(["z"])).unwrap();
        assert_eq!(list, vec!["z".to_string()]);
    }

    #[test]
    fn a_map_with_entity_keys() {
        let r = every_registry();
        let a = Entity::from_raw_u32(9).unwrap();
        let mut v = Every::default();
        write(&r, None, &mut v, None, &json!({ "by_entity": [[a.to_bits(), 2.0]] })).unwrap();
        assert_eq!(v.by_entity.get(&a), Some(&2.0));
        assert_eq!(v.by_entity.len(), 1);
        assert_eq!(read(&r, &v).unwrap()["by_entity"], json!([[a.to_bits(), 2.0]]));
        let e = write(&r, None, &mut v, None, &json!({ "by_entity": { "9": 1 } })).unwrap_err();
        assert!(e.contains("pairs"), "{e}");
    }

    #[test]
    fn durations_take_seconds_or_secs_and_nanos() {
        let r = every_registry();
        let mut v = Every::default();
        write(&r, None, &mut v, None, &json!({ "cooldown": 0.25 })).unwrap();
        assert_eq!(v.cooldown, Duration::from_millis(250));
        write(&r, None, &mut v, None, &json!({ "cooldown": { "secs": 2, "nanos": 5 } })).unwrap();
        assert_eq!(v.cooldown, Duration::new(2, 5));
    }

    #[test]
    fn newtype_components_are_value() {
        let r = every_registry();
        assert_eq!(read_component(&r, &Meters(3.0)).unwrap(), json!({ "value": 3.0 }));
        let mut m = Meters(1.0);
        write_component(&r, None, &mut m, None, &json!({ "value": 4.0 })).unwrap();
        assert_eq!(m, Meters(4.0));
    }

    #[test]
    fn colour_grading_round_trips_with_its_midtones_range() {
        use bevy::render::view::ColorGrading;
        let mut r = TypeRegistry::default();
        r.register::<ColorGrading>();
        let mut grading = ColorGrading::default();
        grading.global.midtones_range = 0.1..0.6;
        grading.global.exposure = 1.5;
        let json = read(&r, &grading).unwrap();
        assert_eq!(json["global"]["midtones_range"], json!({ "start": f64::from(0.1f32), "end": f64::from(0.6f32) }));
        let mut back = ColorGrading::default();
        write(&r, None, &mut back, None, &json).unwrap();
        assert!(back.reflect_partial_eq(&grading).unwrap_or(false), "{json:#}");
    }

    /// A struct with no `Default`: list items of it are built from type info, not stock serde.
    #[derive(Reflect, Debug, Clone, PartialEq)]
    struct Cooldown {
        wait: Duration,
        serial: u64,
        span: std::ops::Range<f32>,
    }

    #[derive(Reflect, Default, Debug, Clone, PartialEq)]
    #[reflect(Default)]
    struct Timers {
        all: Vec<Cooldown>,
        pick: Option<Cooldown>,
    }

    #[test]
    fn items_of_a_type_with_no_default_take_this_modules_shapes() {
        let mut r = TypeRegistry::default();
        r.register::<Timers>();
        let one = Cooldown { wait: Duration::from_millis(250), serial: 1 << 60, span: 0.5..2.0 };
        let original = Timers { all: vec![one.clone(), one.clone()], pick: Some(one) };
        let json = read(&r, &original).unwrap();
        assert_eq!(json["all"][0], json!({ "wait": 0.25, "serial": "1152921504606846976", "span": { "start": 0.5, "end": 2.0 } }));
        let mut back = Timers::default();
        write(&r, None, &mut back, None, &json).unwrap();
        assert_eq!(back, original);
    }

    #[test]
    fn exposure_is_ev100() {
        let r = every_registry();
        let json = read(&r, &bevy::camera::Exposure { ev100: 12.5 }).unwrap();
        assert_eq!(json, json!({ "ev100": 12.5 }));
        let mut back = bevy::camera::Exposure::default();
        write(&r, None, &mut back, None, &json).unwrap();
        assert_eq!(back.ev100, 12.5);
    }

    #[test]
    fn timers_and_stopwatches_read_their_durations_as_seconds() {
        use bevy::time::{Stopwatch, Timer, TimerMode};
        let mut r = TypeRegistry::default();
        r.register::<Timer>();
        let mut timer = Timer::from_seconds(1.5, TimerMode::Repeating);
        timer.tick(Duration::from_millis(250));
        let json = read(&r, &timer).unwrap();
        assert_eq!(json["duration"], json!(1.5), "{json:#}");
        assert_eq!(json["stopwatch"]["elapsed"], json!(0.25), "{json:#}");
        let mut back = Timer::default();
        write(&r, None, &mut back, None, &json).unwrap();
        assert_eq!(back, timer);
        let mut watch = Stopwatch::new();
        watch.tick(Duration::from_secs(3));
        assert_eq!(read(&r, &watch).unwrap()["elapsed"], json!(3.0));
    }

    #[test]
    fn computed_components_refuse_writes() {
        let e = refuse_computed("bevy_transform::components::global_transform::GlobalTransform").unwrap_err();
        assert!(e.contains("GlobalTransform") && e.contains("computed by Bevy"), "{e}");
        assert!(refuse_computed("bevy_transform::components::transform::Transform").is_ok());
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
