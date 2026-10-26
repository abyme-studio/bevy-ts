//! Reflected values in TS systems (reflect-coverage decision 7): a binary codec compiled once per
//! type from its `TypeInfo`, the read targets a generation's systems name, and the two host calls
//! JS makes: `reflect_read` on a tick-cache miss and `reflect_write` on a write.
//!
//! A codec is a tree of ops ([`Node`]). Rust walks a value along it by reflection into bytes;
//! `core.js` generates a decoder from the same tree. The decoder's output equals `JSON.parse` of
//! [`reflect_value::read`]'s JSON for the same value: a kind the ops cannot express (sets, maps
//! with other keys, handles, opaque serde types) is that JSON, embedded.
//!
//! Wire format, little-endian, unaligned: numbers in their own width (`f32` and `f64` as IEEE
//! floats, integers of 32 bits or fewer as themselves, `bool` as one byte); a wide integer as a
//! tag byte then an `f64` (0) or a string (1); a string as a `u32` byte length then UTF-8; an
//! `Entity` as two `u32`s (index, generation), as an `Entity` column holds it; a list or a
//! string-keyed map as a `u32` count then its items (a map's as key then value); an enum as a
//! `u32` variant index then its fields; an `Option` as a flag byte then the value; embedded JSON
//! as a string.
//!
//! Writes cross as JSON text and go through [`reflect_value::write`] over the slot type's
//! default, so a script writes every form TSX takes (a partial struct, a colour string,
//! `{ secs, nanos }`); the binary form only carries complete values.
//!
//! A TS component's heap field (`label: string`, `crate::heap`) is a target too: no reflection,
//! its op built from its `FieldType`, read from and written to its slot at the call's place
//! (`places.rs`: an entity, a resource, or the running observer's value). Its writes cross as the
//! codec's bytes, not JSON: both ends are bevy-ts's.

use crate::manifest::{ResolvedHole, ResolvedReflect};
use crate::places::Place;
use crate::reflect_value;
use bevy::ecs::change_detection::{DetectChangesMut, Tick};
use bevy::ecs::component::ComponentId;
use bevy::ecs::reflect::ReflectComponent;
use bevy::ecs::world::unsafe_world_cell::UnsafeWorldCell;
use bevy::math::{Affine2, Affine3A, DAffine2, DAffine3, DMat2, DMat3, DMat4, DQuat, DVec2, DVec3, DVec4, IVec2, IVec3, IVec4, Mat2, Mat3, Mat3A, Mat4, UVec2, UVec3, UVec4};
use bevy::prelude::*;
use bevy::reflect::{PartialReflect, ReflectRef, TypeInfo, TypeRegistry};
use serde::Serialize;
use std::any::TypeId;
use std::collections::{BTreeMap, HashMap};
use std::time::Duration;

/// A number's width and kind, as JS reads it from a `DataView`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Num {
    F32,
    F64,
    I8,
    U8,
    I16,
    U16,
    I32,
    U32,
    Bool,
}

/// One op of a codec. JS reads the tree from the resolution (`k` names the op).
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(tag = "k", rename_all = "camelCase")]
pub enum Node {
    Num { t: Num },
    /// `i64`, `u64`, `i128`, `u128`, `isize`, `usize`: a number within ±(2^53 - 1), else a string.
    Wide,
    Str,
    /// A glam vector, quaternion, matrix or affine: its lanes, matrices column-major.
    Lanes { t: Num, n: u8 },
    /// Seconds as an `f64`.
    Duration,
    Entity,
    Struct { fields: Vec<(String, Node)> },
    /// A tuple struct of one field nested in a value: its inner value.
    Newtype { of: Box<Node> },
    /// A tuple, or a tuple struct of other than one field: an array.
    Tuple { items: Vec<Node> },
    Array { of: Box<Node>, n: u32 },
    List { of: Box<Node> },
    /// A map with `String` keys: an object.
    StrMap { of: Box<Node> },
    Enum { variants: Vec<Variant> },
    Option { of: Box<Node> },
    /// `reflect_value::read`'s JSON, as text.
    Json,
}

/// One variant of an enum op. `fields` names a struct variant's fields; `items` holds the data's
/// ops (one for a newtype variant, whose data is its inner value).
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Variant {
    pub name: String,
    pub shape: Shape,
    pub items: Vec<Node>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fields: Option<Vec<String>>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Shape {
    Unit,
    Newtype,
    Tuple,
    Struct,
}

/// A whole value's codec: `wrap` when it is a newtype component, which reads as `{ value }` with
/// `node` the inner value's op.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Codec {
    pub wrap: bool,
    pub node: Node,
}

/// A number type by `TypeId`.
fn num(id: TypeId) -> Option<Num> {
    [
        (TypeId::of::<f32>(), Num::F32),
        (TypeId::of::<f64>(), Num::F64),
        (TypeId::of::<i8>(), Num::I8),
        (TypeId::of::<u8>(), Num::U8),
        (TypeId::of::<i16>(), Num::I16),
        (TypeId::of::<u16>(), Num::U16),
        (TypeId::of::<i32>(), Num::I32),
        (TypeId::of::<u32>(), Num::U32),
        (TypeId::of::<bool>(), Num::Bool),
    ]
    .into_iter()
    .find_map(|(t, n)| (t == id).then_some(n))
}

fn wide(id: TypeId) -> bool {
    [TypeId::of::<i64>(), TypeId::of::<u64>(), TypeId::of::<i128>(), TypeId::of::<u128>(), TypeId::of::<isize>(), TypeId::of::<usize>()].contains(&id)
}

/// A glam type's lanes and how to take them from a value.
type LaneFn = fn(&dyn PartialReflect, &mut Vec<u8>) -> bool;

macro_rules! lanes_table {
    ($($t:ty, $num:expr, $n:expr, $arr:ident, $put:ident);* $(;)?) => {
        fn lanes(id: TypeId) -> Option<(Num, u8, LaneFn)> {
            $(if id == TypeId::of::<$t>() {
                let f: LaneFn = |v, out| match v.try_downcast_ref::<$t>() {
                    Some(x) => {
                        for l in x.$arr() {
                            $put(l, out);
                        }
                        true
                    }
                    None => false,
                };
                return Some(($num, $n, f));
            })*
            None
        }
    };
}

fn put_f32(v: f32, out: &mut Vec<u8>) {
    out.extend_from_slice(&v.to_le_bytes());
}
fn put_f64(v: f64, out: &mut Vec<u8>) {
    out.extend_from_slice(&v.to_le_bytes());
}
fn put_i32(v: i32, out: &mut Vec<u8>) {
    out.extend_from_slice(&v.to_le_bytes());
}
fn put_u32(v: u32, out: &mut Vec<u8>) {
    out.extend_from_slice(&v.to_le_bytes());
}

lanes_table! {
    Vec2, Num::F32, 2, to_array, put_f32;
    Vec3, Num::F32, 3, to_array, put_f32;
    Vec3A, Num::F32, 3, to_array, put_f32;
    Vec4, Num::F32, 4, to_array, put_f32;
    Quat, Num::F32, 4, to_array, put_f32;
    DVec2, Num::F64, 2, to_array, put_f64;
    DVec3, Num::F64, 3, to_array, put_f64;
    DVec4, Num::F64, 4, to_array, put_f64;
    DQuat, Num::F64, 4, to_array, put_f64;
    IVec2, Num::I32, 2, to_array, put_i32;
    IVec3, Num::I32, 3, to_array, put_i32;
    IVec4, Num::I32, 4, to_array, put_i32;
    UVec2, Num::U32, 2, to_array, put_u32;
    UVec3, Num::U32, 3, to_array, put_u32;
    UVec4, Num::U32, 4, to_array, put_u32;
    Mat2, Num::F32, 4, to_cols_array, put_f32;
    Mat3, Num::F32, 9, to_cols_array, put_f32;
    Mat3A, Num::F32, 9, to_cols_array, put_f32;
    Mat4, Num::F32, 16, to_cols_array, put_f32;
    DMat2, Num::F64, 4, to_cols_array, put_f64;
    DMat3, Num::F64, 9, to_cols_array, put_f64;
    DMat4, Num::F64, 16, to_cols_array, put_f64;
    Affine2, Num::F32, 6, to_cols_array, put_f32;
    Affine3A, Num::F32, 12, to_cols_array, put_f32;
    DAffine2, Num::F64, 6, to_cols_array, put_f64;
    DAffine3, Num::F64, 12, to_cols_array, put_f64;
}

/// Types nested deeper than this are embedded JSON (a recursive type stops here).
const MAX_DEPTH: usize = 16;

/// Compiles a whole component's or resource's codec.
pub fn compile_component(registry: &TypeRegistry, info: &TypeInfo) -> Codec {
    // `Name` is a newtype `read` takes whole, as a string: `{ value }` of the whole value's JSON.
    if info.type_id() == TypeId::of::<Name>() {
        return Codec { wrap: true, node: Node::Json };
    }
    match info {
        TypeInfo::TupleStruct(t) if t.field_len() == 1 => {
            let inner = t.field_at(0).and_then(|f| f.type_info());
            Codec { wrap: true, node: inner.map_or(Node::Json, |i| compile(registry, i, 1)) }
        }
        _ => Codec { wrap: false, node: compile(registry, info, 0) },
    }
}

/// Compiles one type's op tree.
pub fn compile(registry: &TypeRegistry, info: &TypeInfo, depth: usize) -> Node {
    let id = info.type_id();
    if let Some(t) = num(id) {
        return Node::Num { t };
    }
    if wide(id) {
        return Node::Wide;
    }
    if id == TypeId::of::<String>() {
        return Node::Str;
    }
    if id == TypeId::of::<Duration>() {
        return Node::Duration;
    }
    if id == TypeId::of::<Entity>() {
        return Node::Entity;
    }
    if let Some((t, n, _)) = lanes(id) {
        return Node::Lanes { t, n };
    }
    let path = info.type_path();
    // An uncovered type is `read`'s error; other glam types (`BVec3`, `I64Vec2`, ...) read as
    // their serde shape; handles may read through a hook.
    // `read` takes `Name` and numeric ranges whole, as scalars.
    let whole = id == TypeId::of::<Name>() || path.starts_with("core::ops::Range");
    if depth >= MAX_DEPTH || whole || reflect_value::uncovered(path).is_some() || path.starts_with("glam::") || registry.get(id).is_some_and(|r| r.data::<bevy::asset::ReflectHandle>().is_some()) {
        return Node::Json;
    }
    let sub = |i: Option<&'static TypeInfo>| i.map_or(Node::Json, |i| compile(registry, i, depth + 1));
    // `read` leaves out a field of an uncovered type.
    let covered = |f: &bevy::reflect::NamedField| reflect_value::uncovered(f.type_path()).is_none();
    match info {
        TypeInfo::Enum(e) if path.starts_with("core::option::Option<") => {
            let some = e.variant("Some").and_then(|v| match v {
                bevy::reflect::enums::VariantInfo::Tuple(t) => t.field_at(0).map(|f| f.type_info()),
                _ => None,
            });
            match some {
                Some(i) => Node::Option { of: Box::new(sub(i)) },
                None => Node::Json,
            }
        }
        TypeInfo::Enum(e) => {
            use bevy::reflect::enums::VariantInfo;
            let variants = e
                .iter()
                .map(|v| match v {
                    VariantInfo::Unit(u) => Variant { name: u.name().into(), shape: Shape::Unit, items: vec![], fields: None },
                    VariantInfo::Tuple(t) if t.field_len() == 1 => {
                        Variant { name: t.name().into(), shape: Shape::Newtype, items: vec![sub(t.field_at(0).and_then(|f| f.type_info()))], fields: None }
                    }
                    VariantInfo::Tuple(t) => Variant { name: t.name().into(), shape: Shape::Tuple, items: t.iter().map(|f| sub(f.type_info())).collect(), fields: None },
                    VariantInfo::Struct(s) => Variant {
                        name: s.name().into(),
                        shape: Shape::Struct,
                        items: s.iter().filter(|f| covered(f)).map(|f| sub(f.type_info())).collect(),
                        fields: Some(s.iter().filter(|f| covered(f)).map(|f| f.name().to_string()).collect()),
                    },
                })
                .collect();
            Node::Enum { variants }
        }
        TypeInfo::Struct(s) => Node::Struct { fields: s.iter().filter(|f| covered(f)).map(|f| (f.name().to_string(), sub(f.type_info()))).collect() },
        // A newtype nested in a value is its inner value.
        TypeInfo::TupleStruct(t) if t.field_len() == 1 => Node::Newtype { of: Box::new(sub(t.field_at(0).and_then(|f| f.type_info()))) },
        TypeInfo::TupleStruct(t) => Node::Tuple { items: t.iter().map(|f| sub(f.type_info())).collect() },
        TypeInfo::Tuple(t) => Node::Tuple { items: t.iter().map(|f| sub(f.type_info())).collect() },
        TypeInfo::Array(a) => Node::Array { of: Box::new(sub(a.item_info())), n: a.capacity() as u32 },
        TypeInfo::List(l) => Node::List { of: Box::new(sub(l.item_info())) },
        TypeInfo::Map(m) if m.key_info().is_some_and(|k| k.type_id() == TypeId::of::<String>()) => Node::StrMap { of: Box::new(sub(m.value_info())) },
        // Sets and other maps read sorted by JSON text; opaque types by serde.
        _ => Node::Json,
    }
}

fn put_str(s: &str, out: &mut Vec<u8>) {
    put_u32(s.len() as u32, out);
    out.extend_from_slice(s.as_bytes());
}

fn put_json(registry: &TypeRegistry, v: &dyn PartialReflect, out: &mut Vec<u8>) -> Result<(), String> {
    let json = reflect_value::read(registry, v)?;
    let text = serde_json::to_string(&json).map_err(|e| e.to_string())?;
    put_str(&text, out);
    Ok(())
}

fn type_path(v: &dyn PartialReflect) -> String {
    v.get_represented_type_info().map_or_else(|| v.reflect_type_path().to_string(), |i| i.type_path().to_string())
}

fn mismatch(node: &Node, v: &dyn PartialReflect) -> String {
    format!("{}: the value does not have the shape its codec was compiled for ({node:?})", type_path(v))
}

/// Encodes `v` along `codec`.
pub fn encode(registry: &TypeRegistry, codec: &Codec, v: &dyn PartialReflect, out: &mut Vec<u8>) -> Result<(), String> {
    // Embedded JSON of a newtype is `read` of the whole value: the same as of its field, except
    // for a type `read` takes whole (`Name`, a newtype of its hashed string).
    if codec.wrap && codec.node == Node::Json {
        return put_json(registry, v, out);
    }
    if codec.wrap {
        let ReflectRef::TupleStruct(t) = v.reflect_ref() else { return Err(mismatch(&codec.node, v)) };
        return put(registry, &codec.node, t.field(0).ok_or_else(|| mismatch(&codec.node, v))?, out);
    }
    put(registry, &codec.node, v, out)
}

/// Encodes one value along its node.
pub fn put(registry: &TypeRegistry, node: &Node, v: &dyn PartialReflect, out: &mut Vec<u8>) -> Result<(), String> {
    let bad = || mismatch(node, v);
    match node {
        Node::Num { t } => {
            let ok = match t {
                Num::F32 => v.try_downcast_ref::<f32>().map(|x| out.extend_from_slice(&x.to_le_bytes())),
                Num::F64 => v.try_downcast_ref::<f64>().map(|x| out.extend_from_slice(&x.to_le_bytes())),
                Num::I8 => v.try_downcast_ref::<i8>().map(|x| out.extend_from_slice(&x.to_le_bytes())),
                Num::U8 => v.try_downcast_ref::<u8>().map(|x| out.push(*x)),
                Num::I16 => v.try_downcast_ref::<i16>().map(|x| out.extend_from_slice(&x.to_le_bytes())),
                Num::U16 => v.try_downcast_ref::<u16>().map(|x| out.extend_from_slice(&x.to_le_bytes())),
                Num::I32 => v.try_downcast_ref::<i32>().map(|x| out.extend_from_slice(&x.to_le_bytes())),
                Num::U32 => v.try_downcast_ref::<u32>().map(|x| out.extend_from_slice(&x.to_le_bytes())),
                Num::Bool => v.try_downcast_ref::<bool>().map(|x| out.push(*x as u8)),
            };
            ok.ok_or_else(bad)
        }
        Node::Wide => {
            let n: i128 = if let Some(x) = v.try_downcast_ref::<i64>() {
                *x as i128
            } else if let Some(x) = v.try_downcast_ref::<u64>() {
                *x as i128
            } else if let Some(x) = v.try_downcast_ref::<isize>() {
                *x as i128
            } else if let Some(x) = v.try_downcast_ref::<usize>() {
                *x as i128
            } else if let Some(x) = v.try_downcast_ref::<i128>() {
                *x
            } else if let Some(x) = v.try_downcast_ref::<u128>() {
                // Past i128's range: always a string.
                if *x > i128::MAX as u128 {
                    out.push(1);
                    put_str(&x.to_string(), out);
                    return Ok(());
                }
                *x as i128
            } else {
                return Err(bad());
            };
            const SAFE: i128 = (1 << 53) - 1;
            if (-SAFE..=SAFE).contains(&n) {
                out.push(0);
                put_f64(n as f64, out);
            } else {
                out.push(1);
                put_str(&n.to_string(), out);
            }
            Ok(())
        }
        Node::Str => {
            put_str(v.try_downcast_ref::<String>().ok_or_else(bad)?, out);
            Ok(())
        }
        Node::Lanes { .. } => {
            let id = v.try_as_reflect().map(|r| r.as_any().type_id()).ok_or_else(bad)?;
            let (_, _, f) = lanes(id).ok_or_else(bad)?;
            f(v, out).then_some(()).ok_or_else(bad)
        }
        Node::Duration => {
            put_f64(v.try_downcast_ref::<Duration>().ok_or_else(bad)?.as_secs_f64(), out);
            Ok(())
        }
        Node::Entity => {
            let bits = v.try_downcast_ref::<Entity>().ok_or_else(bad)?.to_bits();
            put_u32(bits as u32, out);
            put_u32((bits >> 32) as u32, out);
            Ok(())
        }
        Node::Struct { fields } => {
            let ReflectRef::Struct(s) = v.reflect_ref() else { return Err(bad()) };
            for (name, n) in fields {
                put(registry, n, s.field(name).ok_or_else(bad)?, out)?;
            }
            Ok(())
        }
        Node::Newtype { of } => {
            let ReflectRef::TupleStruct(t) = v.reflect_ref() else { return Err(bad()) };
            put(registry, of, t.field(0).ok_or_else(bad)?, out)
        }
        Node::Tuple { items } => {
            match v.reflect_ref() {
                ReflectRef::TupleStruct(t) => {
                    for (i, n) in items.iter().enumerate() {
                        put(registry, n, t.field(i).ok_or_else(bad)?, out)?;
                    }
                }
                ReflectRef::Tuple(t) => {
                    for (i, n) in items.iter().enumerate() {
                        put(registry, n, t.field(i).ok_or_else(bad)?, out)?;
                    }
                }
                _ => return Err(bad()),
            }
            Ok(())
        }
        Node::Array { of, n } => {
            let ReflectRef::Array(a) = v.reflect_ref() else { return Err(bad()) };
            if a.len() != *n as usize {
                return Err(bad());
            }
            for x in a.iter() {
                put(registry, of, x, out)?;
            }
            Ok(())
        }
        Node::List { of } => {
            let ReflectRef::List(l) = v.reflect_ref() else { return Err(bad()) };
            put_u32(l.len() as u32, out);
            for x in l.iter() {
                put(registry, of, x, out)?;
            }
            Ok(())
        }
        Node::StrMap { of } => {
            let ReflectRef::Map(m) = v.reflect_ref() else { return Err(bad()) };
            put_u32(m.len() as u32, out);
            for (k, x) in m.iter() {
                put_str(k.try_downcast_ref::<String>().ok_or_else(bad)?, out);
                put(registry, of, x, out)?;
            }
            Ok(())
        }
        Node::Enum { variants } => {
            let ReflectRef::Enum(e) = v.reflect_ref() else { return Err(bad()) };
            let i = e.variant_index();
            let var = variants.get(i).filter(|x| x.name == e.variant_name()).ok_or_else(bad)?;
            put_u32(i as u32, out);
            for (f, n) in var.items.iter().enumerate() {
                let field = match &var.fields {
                    Some(names) => e.field(&names[f]),
                    None => e.field_at(f),
                };
                put(registry, n, field.ok_or_else(bad)?, out)?;
            }
            Ok(())
        }
        Node::Option { of } => {
            let ReflectRef::Enum(e) = v.reflect_ref() else { return Err(bad()) };
            match e.field_at(0) {
                Some(x) if e.variant_name() == "Some" => {
                    out.push(1);
                    put(registry, of, x, out)
                }
                _ => {
                    out.push(0);
                    Ok(())
                }
            }
        }
        Node::Json => put_json(registry, v, out),
    }
}

/// What one read target reads: a whole reflect-tier component or resource, or one hole of a
/// layout-tier one.
#[derive(Clone, Debug)]
pub struct Target {
    pub component: ComponentId,
    pub type_id: TypeId,
    /// The component's type path, for errors and the computed check.
    pub type_path: String,
    /// The hole's top-level field: its name, and its index for a tuple struct.
    pub hole: Option<(String, Option<usize>)>,
    pub codec: Codec,
    /// The slot's own type, for its default when written.
    pub slot_type: TypeId,
    /// A TS component's heap field: its offset and kind. `type_id` and `slot_type` are then `()`.
    pub heap: Option<(usize, crate::heap::HeapKind)>,
    /// A TS message's or event's heap field: read at a message or observer place only, never an
    /// entity's; `component` is then no component's.
    pub payload: Option<Payload>,
}

/// What a payload heap target's layout belongs to; `Target::type_path` names it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Payload {
    Message,
    Event,
}

/// The read targets every generation so far has named, by index. Kept across generations (a
/// target's meaning never changes), so JS may hold indices; its caches are dropped on reload.
#[derive(Resource, Default)]
pub struct ReflectTargets {
    pub targets: Vec<Target>,
    by_key: HashMap<(ComponentId, Option<String>), u32>,
    /// Heap targets by their layout's owner name, field, offset and kind: a reload that moves a
    /// field gets a new target.
    by_heap: HashMap<(String, String, usize, crate::heap::HeapKind), u32>,
}

impl ReflectTargets {
    pub fn get(&self, i: u32) -> Option<&Target> {
        self.targets.get(i as usize)
    }

    /// The target of a whole reflected component, compiled the first time it is named.
    pub fn component(&mut self, registry: &TypeRegistry, component: ComponentId, type_id: TypeId) -> Result<u32, String> {
        if let Some(i) = self.by_key.get(&(component, None)) {
            return Ok(*i);
        }
        let reg = registry.get(type_id).ok_or("not registered for reflection")?;
        if reg.data::<ReflectComponent>().is_none() {
            return Err(format!("{} has no ReflectComponent", reg.type_info().type_path()));
        }
        let codec = compile_component(registry, reg.type_info());
        Ok(self.push((component, None), Target { component, type_id, type_path: reg.type_info().type_path().into(), hole: None, codec, slot_type: type_id, heap: None, payload: None }))
    }

    /// The target of one hole: the component's top-level field `name` (`value` for a newtype's,
    /// an index for a tuple struct's).
    pub fn hole(&mut self, registry: &TypeRegistry, component: ComponentId, type_id: TypeId, name: &str) -> Result<u32, String> {
        let key = (component, Some(name.to_string()));
        if let Some(i) = self.by_key.get(&key) {
            return Ok(*i);
        }
        let reg = registry.get(type_id).ok_or("not registered for reflection")?;
        let info = reg.type_info();
        let (index, field) = match info {
            TypeInfo::Struct(s) => (None, s.field(name).map(|f| (f.type_id(), f.type_info()))),
            TypeInfo::TupleStruct(t) => {
                let i = if t.field_len() == 1 && name == "value" { Some(0) } else { name.parse().ok() };
                (i, i.and_then(|i| t.field_at(i)).map(|f| (f.type_id(), f.type_info())))
            }
            _ => (None, None),
        };
        let (slot_type, field) = field.ok_or_else(|| format!("{} has no field {name}", info.type_path()))?;
        let node = field.map_or(Node::Json, |i| compile(registry, i, 1));
        let target = Target {
            component,
            type_id,
            type_path: info.type_path().into(),
            hole: Some((name.to_string(), index)),
            codec: Codec { wrap: false, node },
            slot_type,
            heap: None,
            payload: None,
        };
        Ok(self.push(key, target))
    }

    /// The target of a heap field of the layout `name` owns: a TS component or resource's
    /// (`component`), or a message's or event's (`payload`, `component` unused).
    pub fn ts_heap(&mut self, component: ComponentId, payload: Option<Payload>, name: &str, field: &crate::layout::FieldLayout) -> u32 {
        let kind = crate::heap::HeapKind::of(&field.ty).expect("a heap field");
        let key = (name.to_string(), field.name.clone(), field.offset, kind);
        if let Some(i) = self.by_heap.get(&key) {
            return *i;
        }
        let node = field_node(&field.ty);
        let target = Target {
            component,
            type_id: TypeId::of::<()>(),
            type_path: name.to_string(),
            hole: Some((field.name.clone(), None)),
            codec: Codec { wrap: false, node },
            slot_type: TypeId::of::<()>(),
            heap: Some((field.offset, kind)),
            payload,
        };
        let i = self.targets.len() as u32;
        self.targets.push(target);
        self.by_heap.insert(key, i);
        i
    }

    fn push(&mut self, key: (ComponentId, Option<String>), t: Target) -> u32 {
        let i = self.targets.len() as u32;
        self.targets.push(t);
        self.by_key.insert(key, i);
        i
    }
}

/// A TS field type's op: a heap field's, or a list item's. `core.js`'s `fieldNode` makes the same.
pub fn field_node(ty: &crate::layout::FieldType) -> Node {
    use crate::layout::FieldType as F;
    match ty {
        F::F32 => Node::Num { t: Num::F32 },
        F::F64 => Node::Num { t: Num::F64 },
        F::I8 => Node::Num { t: Num::I8 },
        F::U8 => Node::Num { t: Num::U8 },
        F::I16 => Node::Num { t: Num::I16 },
        F::U16 => Node::Num { t: Num::U16 },
        F::I32 => Node::Num { t: Num::I32 },
        F::U32 => Node::Num { t: Num::U32 },
        F::Bool => Node::Num { t: Num::Bool },
        F::Str => Node::Str,
        F::Vec2 | F::Vec3 | F::Vec4 | F::Quat => Node::Lanes { t: Num::F32, n: ty.lanes().unwrap() as u8 },
        F::Entity => Node::Entity,
        F::Enum { variants, .. } => Node::Enum { variants: variants.iter().map(|v| Variant { name: v.clone(), shape: Shape::Unit, items: vec![], fields: None }).collect() },
        F::Struct { fields } => Node::Struct { fields: fields.iter().map(|f| (f.name.clone(), field_node(&f.ty))).collect() },
        F::List { of } => Node::List { of: Box::new(field_node(of)) },
        other => unreachable!("{} is no heap field or list item", other.ts_name()),
    }
}

/// The hole's slot in a component value.
fn field<'a>(v: &'a dyn PartialReflect, hole: &(String, Option<usize>)) -> Option<&'a dyn PartialReflect> {
    match (v.reflect_ref(), hole.1) {
        (ReflectRef::Struct(s), _) => s.field(&hole.0),
        (ReflectRef::TupleStruct(t), Some(i)) => t.field(i),
        _ => None,
    }
}

fn field_mut<'a>(v: &'a mut dyn PartialReflect, hole: &(String, Option<usize>)) -> Option<&'a mut dyn PartialReflect> {
    match (v.reflect_mut(), hole.1) {
        (bevy::reflect::ReflectMut::Struct(s), _) => s.field_mut(&hole.0),
        (bevy::reflect::ReflectMut::TupleStruct(t), Some(i)) => t.field_mut(i),
        _ => None,
    }
}

/// Encodes target `t` of `entity` into `out`.
pub fn read(world: &World, t: &Target, entity: Entity, out: &mut Vec<u8>) -> Result<(), String> {
    if let Some((offset, kind)) = t.heap {
        let p = world.get_entity(entity).map_err(|_| format!("{entity} does not exist"))?.get_by_id(t.component).map_err(|_| format!("{entity} has no {}", t.type_path))?;
        // SAFETY: the component's column holds a live value with a heap value of `kind` there.
        unsafe { crate::heap::put_codec_at(p.as_ptr(), offset, kind, out) };
        return Ok(());
    }
    let registry = world.resource::<AppTypeRegistry>().read();
    let rc = registry.get_type_data::<ReflectComponent>(t.type_id).ok_or_else(|| format!("{}: no ReflectComponent", t.type_path))?;
    let e = world.get_entity(entity).map_err(|_| format!("{entity} does not exist"))?;
    let value = rc.reflect(e).ok_or_else(|| format!("{entity} has no {}", t.type_path))?;
    match &t.hole {
        None => encode(&registry, &t.codec, value.as_partial_reflect(), out),
        Some(h) => {
            let slot = field(value.as_partial_reflect(), h).ok_or_else(|| format!("{} has no field {}", t.type_path, h.0))?;
            put(&registry, &t.codec.node, slot, out)
        }
    }
}

/// Writes `json` over the slot type's default into target `t` of `entity`, marking it changed at
/// `tick`.
///
/// # Safety
/// The caller has exclusive access to the component for the duration of the call, and no
/// reference into it is live (a JS view over a layout-tier column is raw memory, not one).
pub unsafe fn write(cell: UnsafeWorldCell, t: &Target, entity: Entity, json: &serde_json::Value, tick: Tick) -> Result<(), String> {
    if t.heap.is_some() {
        return Err(format!("{}: a heap field is written as codec bytes", t.type_path));
    }
    reflect_value::refuse_computed(&t.type_path)?;
    // SAFETY: shared reads of the registry and the asset server, which nothing writes mid-run.
    let world = unsafe { cell.world() };
    let registry = world.resource::<AppTypeRegistry>().read();
    let server = world.get_resource::<AssetServer>();
    let rc = registry.get_type_data::<ReflectComponent>(t.type_id).ok_or_else(|| format!("{}: no ReflectComponent", t.type_path))?;
    let e = cell.get_entity(entity).map_err(|_| format!("{entity} does not exist"))?;
    // SAFETY: exclusive access to the component (the caller's contract).
    let mut value = unsafe { rc.reflect_unchecked_mut(e) }.ok_or_else(|| format!("{entity} has no {}", t.type_path))?;
    let base = reflect_value::default_of(&registry, t.slot_type);
    {
        let whole = value.bypass_change_detection().as_partial_reflect_mut();
        let slot = match &t.hole {
            None => whole,
            Some(h) => field_mut(whole, h).ok_or_else(|| format!("{} has no field {}", t.type_path, h.0))?,
        };
        // A `Handle<T>` slot takes an asset ref or `T.of(...)`, as a TSX prop does.
        let r = match world.get_resource::<crate::reflect_asset::ScriptAssets>() {
            Some(assets) => {
                // No world: the component is borrowed mutably meanwhile.
                let slots = crate::reflect_asset::Slots { registry: &registry, server, assets, world: None, check: false };
                reflect_value::write_with(slots.cx(), slot, base.as_deref(), json)
            }
            None => reflect_value::write(&registry, server, slot, base.as_deref(), json),
        };
        r.map_err(|e| match &t.hole {
            Some(h) => format!("{}.{}: {e}", t.type_path, h.0),
            None => e,
        })?;
    }
    value.set_last_changed(tick);
    Ok(())
}

/// Writes a heap field from the codec's bytes, marking the component changed at `tick`.
///
/// # Safety
/// As [`write`].
pub unsafe fn write_heap(cell: UnsafeWorldCell, t: &Target, entity: Entity, mut bytes: &[u8], tick: Tick) -> Result<(), String> {
    let (offset, kind) = t.heap.ok_or("not a heap field")?;
    let field = t.hole.as_ref().map_or("", |h| h.0.as_str());
    let v = crate::heap::HeapValue::read_codec(kind, &mut bytes).map_err(|e| format!("{}.{field}: {e}", t.type_path))?;
    if !bytes.is_empty() {
        return Err(format!("{}.{field}: bytes left after the value", t.type_path));
    }
    let e = cell.get_entity(entity).map_err(|_| format!("{entity} does not exist"))?;
    // SAFETY: exclusive access to the component (the caller's contract).
    let mut value = unsafe { e.get_mut_by_id(t.component) }.map_err(|_| format!("{entity} has no {}", t.type_path))?;
    // SAFETY: the column holds a live value with a heap value of `kind` at `offset`.
    unsafe { crate::heap::replace(value.bypass_change_detection().as_ptr(), offset, v) };
    value.set_last_changed(tick);
    Ok(())
}

/// The read targets of a TS component's heap fields, or (with `payload`, `id` unused) a
/// message's or event's, as the resolution hands them to JS.
pub fn ts_heap_holes(world: &mut World, name: &str, id: ComponentId, payload: Option<Payload>, layout: &crate::layout::ComponentLayout) -> Vec<ResolvedHole> {
    let mut targets = world.get_resource_or_init::<ReflectTargets>();
    layout
        .fields
        .iter()
        .filter(|f| f.ty.is_heap())
        .map(|f| {
            let t = targets.ts_heap(id, payload, name, f);
            let codec = serde_json::to_value(&targets.targets[t as usize].codec).expect("a codec is JSON");
            ResolvedHole { name: f.name.clone(), target: t, codec, heap: true }
        })
        .collect()
}

/// What a generation's JS needs for the reflected values its systems read: per reflect-tier
/// class its target and codec, and per layout-tier class its holes'. Hole targets are made here.
pub fn resolve(world: &mut World, systems: &crate::systems::TsSystems) -> (BTreeMap<String, ResolvedReflect>, BTreeMap<String, Vec<ResolvedHole>>) {
    use crate::systems::RunParam;
    let mut reflect = BTreeMap::new();
    let mut layout = BTreeMap::new();
    let native = world.resource::<crate::layout::NativeLayouts>().clone();
    for sys in systems.iter() {
        for p in &sys.params {
            match p {
                RunParam::Query(q) => {
                    for c in &q.columns {
                        match c.reflect {
                            Some(t) => {
                                reflect.insert(c.name.clone(), t);
                            }
                            None => {
                                if let Some(n) = native.by_id(c.id) {
                                    layout.insert(c.name.clone(), (n.id, n.layout.fields.iter().map(|f| f.name.clone()).collect::<Vec<_>>()));
                                }
                            }
                        }
                    }
                }
                RunParam::ReflectRes { name, target, .. } => {
                    reflect.insert(name.clone(), *target);
                }
                RunParam::Res { name, id, .. } => {
                    if let Some(n) = native.by_id(*id) {
                        layout.insert(name.clone(), (n.id, n.layout.fields.iter().map(|f| f.name.clone()).collect()));
                    }
                }
                _ => {}
            }
        }
    }
    let registry = world.resource::<AppTypeRegistry>().clone();
    let registry = registry.read();
    let mut targets = world.remove_resource::<ReflectTargets>().unwrap_or_default();
    let resolved = reflect
        .into_iter()
        .filter_map(|(name, t)| {
            let target = targets.get(t)?;
            let fields = match (&target.codec, registry.get(target.type_id).map(|r| r.type_info())) {
                (Codec { wrap: false, .. }, Some(TypeInfo::Struct(s))) => Some(s.iter().map(|f| f.name().to_string()).collect()),
                _ => None,
            };
            Some((name, ResolvedReflect { target: t, codec: serde_json::to_value(&target.codec).ok()?, fields }))
        })
        .collect();
    let mut holes = BTreeMap::new();
    for (name, (id, columns)) in layout {
        let Some(type_id) = world.components().get_info(id).and_then(|i| i.type_id()) else { continue };
        let Some(info) = registry.get(type_id).map(|r| r.type_info()) else { continue };
        let names: Vec<String> = match info {
            TypeInfo::Struct(s) => s.iter().map(|f| f.name().to_string()).collect(),
            TypeInfo::TupleStruct(t) if t.field_len() > 1 => (0..t.field_len()).map(|i| i.to_string()).collect(),
            _ => vec![],
        };
        let list: Vec<ResolvedHole> = names
            .into_iter()
            .filter(|f| !columns.contains(f))
            .filter_map(|f| {
                let t = targets.hole(&registry, id, type_id, &f).ok()?;
                Some(ResolvedHole { name: f, target: t, codec: serde_json::to_value(&targets.get(t)?.codec).ok()?, heap: false })
            })
            .collect();
        if !list.is_empty() {
            holes.insert(name, list);
        }
    }
    world.insert_resource(targets);
    (resolved, holes)
}

/// Host function ids `core.js` calls for reflected values; exported params take ids from 0.
pub const READ: u32 = 0xFFFF_0000;
pub const WRITE: u32 = 0xFFFF_0001;
/// `read`'s JSON text instead of the codec's bytes: the bench's baseline, and nothing else.
pub const READ_JSON: u32 = 0xFFFF_0002;

/// Reflected host calls made so far (reads and writes), for tests and the bench.
pub static CALLS: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

/// `reflect_read(place, target)` and `reflect_write(place, target, len)`: arguments as `f64`
/// slots in the place layout (`places.rs`: slot 0 and 1 the place, slot 2 the target, then the
/// call's own), the bytes in slice 0. A read answers its byte length in slot 0; when the slice was
/// too short nothing is copied and JS calls again with one at least that long.
pub fn call(cell: UnsafeWorldCell, tick: Tick, id: u32, args: &[u8], ret: &mut [u8], slices: &mut [&mut [u8]]) -> Result<usize, String> {
    thread_local! {
        /// Bytes a read encodes into before they are copied to JS, kept between calls.
        static SCRATCH: std::cell::RefCell<Vec<u8>> = const { std::cell::RefCell::new(Vec::new()) };
    }
    SCRATCH.with_borrow_mut(|scratch| call_with(cell, tick, scratch, id, args, ret, slices))
}

/// The bytes of a value at a place that is no entity's: the running observer's value, or a
/// queued message.
fn value_at(world: &World, t: &Target, place: Place) -> Result<*const u8, String> {
    let payloads = || world.get_resource::<crate::messages::TsPayloads>().ok_or("no messages or events");
    match place {
        Place::Observer => {
            let v = world.get_resource::<crate::places::RunPlace>().and_then(|r| r.observer).ok_or_else(|| format!("{place}: no observer is running"))?;
            let ours = match (t.payload, v.of) {
                (None, crate::places::ValueOf::Component(c)) => c == t.component,
                (Some(Payload::Event), crate::places::ValueOf::Event(i)) => payloads()?.events.get(i).is_some_and(|e| e.name == t.type_path),
                _ => false,
            };
            if !ours {
                return Err(format!("{place} is no {}", t.type_path));
            }
            Ok(v.ptr)
        }
        Place::Message(seq) if t.payload == Some(Payload::Message) => {
            let p = payloads()?;
            let q = p.message(&t.type_path).map(|i| &p.messages[i]).ok_or_else(|| format!("{} is not a declared message", t.type_path))?;
            q.record(seq).ok_or_else(|| format!("{place} of {} is gone (messages live two updates)", t.type_path))
        }
        _ => Err(format!("{place} holds no {}", t.type_path)),
    }
}

/// A read's answer: its byte length in slot 0, and the bytes when the slice holds them.
fn answer(scratch: &[u8], buf: &mut [u8], ret: &mut [u8]) -> Result<usize, String> {
    if scratch.len() <= buf.len() {
        buf[..scratch.len()].copy_from_slice(scratch);
    }
    let mut w = crate::host::SlotWriter::new(ret);
    w.push(scratch.len() as f64);
    w.finish()
}

/// A heap read or write of the running system's `Local` `i`, field `field` of its layout. A
/// `Local` has no ticks, so a write marks nothing.
#[allow(clippy::too_many_arguments)]
fn local_call(world: &World, i: u32, field: u32, id: u32, r: &mut crate::host::SlotReader, scratch: &mut Vec<u8>, ret: &mut [u8], slices: &mut [&mut [u8]]) -> Result<usize, String> {
    let place = Place::Local(i);
    let locals = world.get_resource::<crate::places::RunPlace>().and_then(|r| r.locals).ok_or_else(|| format!("{place}: no system is running"))?;
    // SAFETY: the run that set the locals is in progress; JS holds the store's bytes only as
    // the frame's raw pointer, which this call does not overlap with a reference.
    let store = unsafe { locals.get(i) }.ok_or_else(|| format!("{place} does not exist"))?;
    let f = store.layout.fields.get(field as usize).ok_or_else(|| format!("{place} has no field {field}"))?;
    let (offset, name) = (f.offset, f.name.clone());
    let kind = crate::heap::HeapKind::of(&f.ty).ok_or_else(|| format!("Local.{name} is no heap field"))?;
    let buf = slices.first_mut().ok_or("no buffer")?;
    match id {
        READ => {
            scratch.clear();
            // SAFETY: the store is a live value of its layout, with a `kind` value at `offset`.
            unsafe { crate::heap::put_codec_at(store.data().as_ptr(), offset, kind, scratch) };
            answer(scratch, buf, ret)
        }
        WRITE => {
            let len = r.slot()? as usize;
            let mut bytes = buf.get(..len).ok_or("write: length past the buffer")?;
            let v = crate::heap::HeapValue::read_codec(kind, &mut bytes).map_err(|e| format!("Local.{name}: {e}"))?;
            if !bytes.is_empty() {
                return Err(format!("Local.{name}: bytes left after the value"));
            }
            // SAFETY: as above.
            unsafe { crate::heap::replace(store.data_mut().as_mut_ptr(), offset, v) };
            Ok(0)
        }
        _ => Err(format!("no reflected host function {id} at {place}")),
    }
}

fn call_with(cell: UnsafeWorldCell, tick: Tick, scratch: &mut Vec<u8>, id: u32, args: &[u8], ret: &mut [u8], slices: &mut [&mut [u8]]) -> Result<usize, String> {
    CALLS.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let mut r = crate::host::SlotReader::new(args);
    let place = Place::decode(r.slot()?, r.slot()?)?;
    let index = r.slot()? as u32;
    // SAFETY: shared read of the resource that holds the targets; nothing writes it mid-run.
    let world = unsafe { cell.world() };
    if let Place::Local(i) = place {
        return local_call(world, i, index, id, &mut r, scratch, ret, slices);
    }
    let targets = world.get_resource::<ReflectTargets>().ok_or("no reflected targets")?;
    let t = targets.get(index).ok_or_else(|| format!("no reflected target {index}"))?;
    let entity = match place {
        Place::Entity(_) | Place::Resource if t.payload.is_some() => return Err(format!("{place} holds no {}", t.type_path)),
        Place::Entity(e) => Some(e),
        // The target's resource, wherever it lives.
        Place::Resource => Some(crate::systems::resource_entity(world, t.component).map(|(e, _)| e).ok_or_else(|| format!("resource {} does not exist", t.type_path))?),
        _ => None,
    };
    let buf = slices.first_mut().ok_or("no buffer")?;
    let Some(entity) = entity else {
        // A place that is no entity's holds heap values only, and reads them only: a message or
        // an event is `&M` in Bevy, a lifecycle value is a copy.
        let (offset, kind) = t.heap.ok_or_else(|| format!("{place}: {} is not read through reflection there", t.type_path))?;
        return match id {
            READ => {
                let p = value_at(world, t, place)?;
                scratch.clear();
                // SAFETY: the place's value is a live value of the target's layout.
                unsafe { crate::heap::put_codec_at(p, offset, kind, scratch) };
                answer(scratch, buf, ret)
            }
            WRITE => Err(format!("{}.{}: read-only at {place}", t.type_path, t.hole.as_ref().map_or("", |h| h.0.as_str()))),
            _ => Err(format!("no reflected host function {id} at {place}")),
        };
    };
    match id {
        READ => {
            scratch.clear();
            read(world, t, entity, scratch)?;
            answer(scratch, buf, ret)
        }
        READ_JSON => {
            let registry = world.resource::<AppTypeRegistry>().read();
            let rc = registry.get_type_data::<ReflectComponent>(t.type_id).ok_or("no ReflectComponent")?;
            let e = world.get_entity(entity).map_err(|_| format!("{entity} does not exist"))?;
            let v = rc.reflect(e).ok_or_else(|| format!("{entity} has no {}", t.type_path))?.as_partial_reflect();
            let json = match &t.hole {
                None => reflect_value::read_component(&registry, v)?,
                Some(h) => reflect_value::read(&registry, field(v, h).ok_or("no such field")?)?,
            };
            *scratch = serde_json::to_vec(&json).map_err(|e| e.to_string())?;
            answer(scratch, buf, ret)
        }
        WRITE if t.heap.is_some() => {
            let len = r.slot()? as usize;
            let bytes = buf.get(..len).ok_or("write: length past the buffer")?;
            let t = t.clone();
            // SAFETY: called from a host call during an exclusive run: no other access is live.
            unsafe { write_heap(cell, &t, entity, bytes, tick) }?;
            Ok(0)
        }
        WRITE => {
            let len = r.slot()? as usize;
            let json: serde_json::Value = serde_json::from_slice(buf.get(..len).ok_or("write: length past the buffer")?).map_err(|e| format!("write: {e}"))?;
            let t = t.clone();
            // SAFETY: called from a host call during an exclusive run: no other access is live.
            unsafe { write(cell, &t, entity, &json, tick) }?;
            Ok(0)
        }
        _ => Err(format!("no reflected host function {id}")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::camera::{OrthographicProjection, PerspectiveProjection, Projection};

    #[derive(Reflect, Default)]
    struct Fixture {
        a: f32,
        n: u64,
        big: u64,
        s: String,
        v: Vec3,
        list: Vec<Entity>,
        opt: Option<u8>,
        d: Duration,
        t: (i16, bool),
        arr: [u16; 3],
    }

    #[derive(Reflect, Default)]
    struct Wrapper(Vec<u32>);

    fn registry() -> TypeRegistry {
        let mut r = TypeRegistry::new();
        r.register::<Projection>();
        r.register::<Fixture>();
        r.register::<Wrapper>();
        r
    }

    fn node_of<T: Reflect + bevy::reflect::Typed>(r: &TypeRegistry) -> Codec {
        compile_component(r, T::type_info())
    }

    #[test]
    fn ops_follow_type_info() {
        let r = registry();
        let c = node_of::<Fixture>(&r);
        let Node::Struct { fields } = &c.node else { panic!("{c:?}") };
        let kinds: Vec<_> = fields.iter().map(|(n, k)| (n.as_str(), k.clone())).collect();
        assert_eq!(kinds[0], ("a", Node::Num { t: Num::F32 }));
        assert_eq!(kinds[1].1, Node::Wide);
        assert_eq!(kinds[3].1, Node::Str);
        assert_eq!(kinds[4].1, Node::Lanes { t: Num::F32, n: 3 });
        assert_eq!(kinds[5].1, Node::List { of: Box::new(Node::Entity) });
        assert_eq!(kinds[6].1, Node::Option { of: Box::new(Node::Num { t: Num::U8 }) });
        assert_eq!(kinds[7].1, Node::Duration);
        assert_eq!(kinds[8].1, Node::Tuple { items: vec![Node::Num { t: Num::I16 }, Node::Num { t: Num::Bool }] });
        assert_eq!(kinds[9].1, Node::Array { of: Box::new(Node::Num { t: Num::U16 }), n: 3 });
        let w = node_of::<Wrapper>(&r);
        assert!(w.wrap);
        assert_eq!(w.node, Node::List { of: Box::new(Node::Num { t: Num::U32 }) });
    }

    #[test]
    fn projection_variants_and_bytes() {
        let r = registry();
        let c = node_of::<Projection>(&r);
        let Node::Enum { variants } = &c.node else { panic!("{c:?}") };
        let names: Vec<_> = variants.iter().map(|v| (v.name.as_str(), v.shape)).collect();
        assert_eq!(names[..2], [("Perspective", Shape::Newtype), ("Orthographic", Shape::Newtype)]);
        let mut out = Vec::new();
        encode(&r, &c, &Projection::Perspective(PerspectiveProjection::default()), &mut out).unwrap();
        assert_eq!(&out[..4], &0u32.to_le_bytes());
        let mut o2 = Vec::new();
        encode(&r, &c, &Projection::Orthographic(OrthographicProjection::default_3d()), &mut o2).unwrap();
        assert_eq!(&o2[..4], &1u32.to_le_bytes());
    }

    #[test]
    fn wide_integers_past_2_53_are_strings() {
        let r = registry();
        let c = node_of::<Fixture>(&r);
        let f = Fixture { n: 7, big: u64::MAX, ..default() };
        let mut out = Vec::new();
        encode(&r, &c, &f, &mut out).unwrap();
        // a: f32, then n: tag 0 and 7.0, then big: tag 1 and its digits.
        assert_eq!(out[4], 0);
        assert_eq!(f64::from_le_bytes(out[5..13].try_into().unwrap()), 7.0);
        assert_eq!(out[13], 1);
        let len = u32::from_le_bytes(out[14..18].try_into().unwrap()) as usize;
        assert_eq!(std::str::from_utf8(&out[18..18 + len]).unwrap(), u64::MAX.to_string());
    }
}
