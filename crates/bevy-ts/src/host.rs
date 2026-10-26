//! Exported system params (native decision 8) over the sixth seam operation, call host function
//! (N2). A Rust crate exports a `SystemParam` with `#[bevy_ts_native::export]` and
//! `app.ts_param::<P>()`; each `pub fn` becomes a host function JS calls synchronously during a
//! run. The param may write (`ResMut`, `Query<&mut T>`) and may defer (`Commands`, `Gizmos`).
//!
//! Every call fetches the param with `SystemParam::get_param` on the run's `UnsafeWorldCell`, at
//! the calling TS system's own `(last_run, tick)` ([`RunTicks`]): no call spends a world tick, a
//! write carries the run's tick as a TS `Mut` write does, and `is_changed` means "since this TS
//! system last ran". A method sees the system's writes so far; a write it makes in place is
//! seen by the system's next read. Deferred state applies after the run, after TS's own output,
//! in the system's parameter order; a run that throws drops it. One state per exported param is
//! shared by every TS system that takes it, `Local`s included.
//!
//! JS holds raw pointers into columns and resource rows for the run, so structural changes go
//! only through deferred params, a param that writes every component or resource is refused at
//! registration, and a slice argument overlapping a run's column is refused when the slice is
//! `&mut` or the param writes that column.
//!
//! Wire format, engine-neutral: arguments and results are little-endian `f64` slots (scalars,
//! `bool` as 0/1, `Entity` as its bits, vectors as their lanes, records field by field, a
//! fieldless enum as its variant's index, `Option` as a flag slot then the value). A string is its
//! UTF-8 length in one slot, then its bytes packed into the slots after it. A colour is a tag slot,
//! then four channels or a string; a list is a count slot, then its elements; a JSON value is its
//! text as a string. Slices cross in place: up to four views over JS memory. A point list is a
//! slice of `f32`: a `Float32Array` as is, an array packed by the glue into a scratch one.
//!
//! A method may return a builder (`MethodSpec::chain`): JS gets the param object back, typed as
//! the builder, whose methods are the settings (`MethodSpec::of`). What a setting does is the
//! export's business; the host only routes the call and returns `this`.

use bevy::ecs::change_detection::Tick;
use bevy::ecs::component::ComponentId;
use bevy::ecs::system::{SystemAccess, SystemMeta, SystemParam, SystemParamItem, SystemState};
use bevy::ecs::world::unsafe_world_cell::UnsafeWorldCell;
use bevy::prelude::*;
use std::any::Any;
use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write as _;
use std::sync::{Mutex, OnceLock};

/// The type of one argument or result, for codecs, JS glue and TS declarations.
#[derive(Clone, Debug, PartialEq)]
pub enum Kind {
    /// A number: `f32`, `f64`, an integer.
    Num(&'static str),
    Bool,
    Entity,
    Vec2,
    Vec3,
    Quat,
    /// A `NativeLayout` record: its TS name and fields.
    Record(&'static str, Vec<(String, Kind)>),
    /// A fieldless enum: its TS name and variant names, crossing as the variant's index.
    Enum(&'static str, Vec<&'static str>),
    Option(Box<Kind>),
    /// A UTF-8 string: `&str` or `String` as an argument, `String` as a result.
    Str,
    /// A Bevy `Color` in any form `bevy_ts::color` takes.
    Color,
    /// An `Isometry2d` from a `Vec2`, a `Rot2` or `{ rotation, translation }`.
    Isometry2d,
    /// An `Isometry3d` from a `Vec3`, a `Quat` or `{ rotation, translation }`.
    Isometry3d,
    /// A `Transform` or a `GlobalTransform` value, for Bevy's `impl TransformPoint`.
    TransformPoint,
    UVec2,
    UVec3,
    /// A list of tuples of these kinds (`[Vec3, Color][]`), crossing in the call's slots.
    List(Vec<Kind>),
    /// A JSON value with its TS type, crossing as its text.
    Json(&'static str),
    Unit,
}

/// Bevy's `Color` variants in declaration order, a colour's tag, and their channels in reflected
/// field order. Tag [`COLOR_HEX`] is a string, [`COLOR_JSON`] any other form, as JSON text.
pub const COLOR_VARIANTS: [(&str, [&str; 4]); 13] = [
    ("Srgba", ["red", "green", "blue", "alpha"]),
    ("LinearRgba", ["red", "green", "blue", "alpha"]),
    ("Hsla", ["hue", "saturation", "lightness", "alpha"]),
    ("Hsva", ["hue", "saturation", "value", "alpha"]),
    ("Hwba", ["hue", "whiteness", "blackness", "alpha"]),
    ("Laba", ["lightness", "a", "b", "alpha"]),
    ("Lcha", ["lightness", "chroma", "hue", "alpha"]),
    ("Oklaba", ["lightness", "a", "b", "alpha"]),
    ("Oklcha", ["lightness", "chroma", "hue", "alpha"]),
    ("Xyza", ["x", "y", "z", "alpha"]),
    ("Okhsla", ["hue", "saturation", "lightness", "alpha"]),
    ("Okhsva", ["hue", "saturation", "value", "alpha"]),
    ("Okhwba", ["hue", "whiteness", "blackness", "alpha"]),
];
/// A colour's tag for a string, parsed in Rust.
pub const COLOR_HEX: f64 = 13.0;
/// A colour's tag for any other form, its JSON text parsed in Rust.
pub const COLOR_JSON: f64 = 14.0;

impl Kind {
    /// Slots the value takes; a string's are its length slot, as an empty string's.
    pub fn slots(&self) -> usize {
        match self {
            Kind::Num(_) | Kind::Bool | Kind::Entity | Kind::Enum(..) | Kind::Str | Kind::List(_) | Kind::Json(_) => 1,
            Kind::Vec2 | Kind::UVec2 => 2,
            Kind::Vec3 | Kind::UVec3 => 3,
            Kind::Quat | Kind::Isometry2d => 4,
            Kind::Color => 5,
            Kind::Isometry3d => 7,
            Kind::TransformPoint => 13,
            Kind::Record(_, fields) => fields.iter().map(|(_, k)| k.slots()).sum(),
            Kind::Option(k) => 1 + k.slots(),
            Kind::Unit => 0,
        }
    }

    fn ts_in(&self) -> String {
        match self {
            Kind::Num(_) => "number".into(),
            Kind::Bool => "boolean".into(),
            Kind::Entity => "Entity".into(),
            Kind::Vec2 => "{ readonly x: number; readonly y: number } | readonly [number, number]".into(),
            Kind::Vec3 => "{ readonly x: number; readonly y: number; readonly z: number } | readonly [number, number, number]".into(),
            Kind::Quat => "{ readonly x: number; readonly y: number; readonly z: number; readonly w: number } | readonly [number, number, number, number]".into(),
            Kind::Record(name, _) => format!("Partial<{name}>"),
            Kind::Enum(name, _) => name.to_string(),
            Kind::Option(k) => format!("{} | null | undefined", k.ts_in()),
            Kind::Str => "string".into(),
            Kind::Color => "ColorValue".into(),
            Kind::Isometry2d => format!("{} | {{ readonly cos: number; readonly sin: number }} | {{ readonly rotation?: {{ readonly cos: number; readonly sin: number }}; readonly translation?: {} }}", Kind::Vec2.ts_in(), Kind::Vec2.ts_in()),
            Kind::Isometry3d => format!("{} | {} | {{ readonly rotation?: {}; readonly translation?: {} }}", Kind::Vec3.ts_in(), Kind::Quat.ts_in(), Kind::Quat.ts_in(), Kind::Vec3.ts_in()),
            Kind::TransformPoint => format!(
                "{{ readonly translation?: {}; readonly rotation?: {}; readonly scale?: {} }} | {{ readonly value: readonly number[] }}",
                Kind::Vec3.ts_in(),
                Kind::Quat.ts_in(),
                Kind::Vec3.ts_in()
            ),
            Kind::UVec2 => Kind::Vec2.ts_in(),
            Kind::UVec3 => Kind::Vec3.ts_in(),
            Kind::List(ks) => format!("readonly (readonly [{}])[]", ks.iter().map(Kind::ts_in).collect::<Vec<_>>().join(", ")),
            Kind::Json(t) => t.to_string(),
            Kind::Unit => "void".into(),
        }
    }

    fn ts_out(&self) -> String {
        match self {
            Kind::Num(_) => "number".into(),
            Kind::Bool => "boolean".into(),
            Kind::Entity => "Entity".into(),
            Kind::Vec2 => "{ readonly x: number; readonly y: number }".into(),
            Kind::Vec3 => "{ readonly x: number; readonly y: number; readonly z: number }".into(),
            Kind::Quat => "{ readonly x: number; readonly y: number; readonly z: number; readonly w: number }".into(),
            Kind::Record(name, _) | Kind::Enum(name, _) => name.to_string(),
            Kind::Option(k) => format!("{} | undefined", k.ts_out()),
            Kind::Str => "string".into(),
            Kind::Json(t) => t.to_string(),
            Kind::UVec2 => Kind::Vec2.ts_out(),
            Kind::UVec3 => Kind::Vec3.ts_out(),
            // Arguments only: their `HostValue`s refuse to be written as results.
            Kind::Color | Kind::Isometry2d | Kind::Isometry3d | Kind::TransformPoint | Kind::List(_) => "never".into(),
            Kind::Unit => "void".into(),
        }
    }

    /// Whether this kind's TS type names `Entity`, which the declarations import from `bevy-ts`.
    fn names_entity(&self) -> bool {
        match self {
            Kind::Entity => true,
            Kind::Record(_, fields) => fields.iter().any(|(_, k)| k.names_entity()),
            Kind::Option(k) => k.names_entity(),
            Kind::List(ks) => ks.iter().any(Kind::names_entity),
            _ => false,
        }
    }

    /// The TS declarations of the records and enums this kind names, by name.
    fn declare(&self, out: &mut BTreeMap<&'static str, String>) {
        match self {
            Kind::Record(name, fields) => {
                let mut s = format!("export interface {name} {{\n");
                for (f, k) in fields {
                    let _ = writeln!(s, "  readonly {f}: {};", k.ts_out());
                }
                s.push_str("}\n");
                out.insert(name, s);
                fields.iter().for_each(|(_, k)| k.declare(out));
            }
            Kind::Enum(name, variants) => {
                let names: Vec<String> = variants.iter().map(|v| format!("{v:?}")).collect();
                out.insert(name, format!("export type {name} = {};\n", names.join(" | ")));
            }
            Kind::Option(k) => k.declare(out),
            Kind::List(ks) => ks.iter().for_each(|k| k.declare(out)),
            _ => {}
        }
    }
}

/// Reads `f64` slots.
pub struct SlotReader<'a> {
    bytes: &'a [u8],
    at: usize,
}

impl<'a> SlotReader<'a> {
    pub fn new(bytes: &'a [u8]) -> Self {
        SlotReader { bytes, at: 0 }
    }

    pub fn slot(&mut self) -> Result<f64, String> {
        let b = self.bytes.get(self.at..self.at + 8).ok_or("too few arguments")?;
        self.at += 8;
        Ok(f64::from_le_bytes(b.try_into().unwrap()))
    }

    /// The bytes read so far: a call's arguments, to replay them.
    pub fn taken(&self) -> &'a [u8] {
        &self.bytes[..self.at.min(self.bytes.len())]
    }

    /// `n` bytes packed into the slots from here, advancing past every slot they touch.
    pub fn bytes(&mut self, n: usize) -> Result<&'a [u8], String> {
        let end = self.at.checked_add(n).ok_or("a string runs past the arguments")?;
        let b = self.bytes.get(self.at..end).ok_or("a string runs past the arguments")?;
        self.at += n.div_ceil(8) * 8;
        Ok(b)
    }
}

/// Writes `f64` slots into a buffer of fixed capacity.
pub struct SlotWriter<'a> {
    bytes: &'a mut [u8],
    at: usize,
    overflow: bool,
}

impl<'a> SlotWriter<'a> {
    pub fn new(bytes: &'a mut [u8]) -> Self {
        SlotWriter { bytes, at: 0, overflow: false }
    }

    pub fn push(&mut self, v: f64) {
        match self.bytes.get_mut(self.at..self.at + 8) {
            Some(b) => b.copy_from_slice(&v.to_le_bytes()),
            None => self.overflow = true,
        }
        self.at += 8;
    }

    /// Packs `b` into the slots from here, the last one zero-padded.
    pub fn push_bytes(&mut self, b: &[u8]) {
        let end = self.at + b.len().div_ceil(8) * 8;
        match self.bytes.get_mut(self.at..end) {
            Some(dst) => {
                dst[..b.len()].copy_from_slice(b);
                dst[b.len()..].fill(0);
            }
            None => self.overflow = true,
        }
        self.at = end;
    }

    /// Slots written, or an error when the result did not fit.
    pub fn finish(self) -> Result<usize, String> {
        if self.overflow { Err("the result is too large for one call; return it through a &mut slice".into()) } else { Ok(self.at / 8) }
    }
}

/// A value that crosses as slots: an argument or a result of an exported method.
#[diagnostic::on_unimplemented(
    message = "`{Self}` cannot cross to TS as an argument or result of an exported method",
    note = "exported methods take and return numbers, bool, Entity, Vec2, Vec3, Quat, UVec2, UVec3, NativeLayout records, strings (&str or String in, String out), Option of these, and &[T] / &mut [T] slices; they also take Color, Isometry2d, Isometry3d, Transform, HostTransform, HostList<(A, B)>, HostJson and HostPoints<T>"
)]
pub trait HostValue: Sized {
    fn kind() -> Kind;
    fn read(r: &mut SlotReader) -> Result<Self, String>;
    fn write(&self, w: &mut SlotWriter);
}

macro_rules! numbers {
    ($($t:ty),*) => {
        $(impl HostValue for $t {
            fn kind() -> Kind {
                Kind::Num(stringify!($t))
            }
            fn read(r: &mut SlotReader) -> Result<Self, String> {
                Ok(r.slot()? as $t)
            }
            fn write(&self, w: &mut SlotWriter) {
                w.push(*self as f64);
            }
        })*
    };
}
numbers!(f32, f64, i8, u8, i16, u16, i32, u32);

impl HostValue for bool {
    fn kind() -> Kind {
        Kind::Bool
    }
    fn read(r: &mut SlotReader) -> Result<Self, String> {
        Ok(r.slot()? != 0.0)
    }
    fn write(&self, w: &mut SlotWriter) {
        w.push(if *self { 1.0 } else { 0.0 });
    }
}

impl HostValue for Entity {
    fn kind() -> Kind {
        Kind::Entity
    }
    fn read(r: &mut SlotReader) -> Result<Self, String> {
        let bits = r.slot()? as u64;
        Entity::try_from_bits(bits).ok_or_else(|| format!("{bits} is not an entity"))
    }
    fn write(&self, w: &mut SlotWriter) {
        w.push(self.to_bits() as f64);
    }
}

impl HostValue for Vec2 {
    fn kind() -> Kind {
        Kind::Vec2
    }
    fn read(r: &mut SlotReader) -> Result<Self, String> {
        Ok(Vec2::new(r.slot()? as f32, r.slot()? as f32))
    }
    fn write(&self, w: &mut SlotWriter) {
        self.to_array().iter().for_each(|v| w.push(*v as f64));
    }
}

impl HostValue for Vec3 {
    fn kind() -> Kind {
        Kind::Vec3
    }
    fn read(r: &mut SlotReader) -> Result<Self, String> {
        Ok(Vec3::new(r.slot()? as f32, r.slot()? as f32, r.slot()? as f32))
    }
    fn write(&self, w: &mut SlotWriter) {
        self.to_array().iter().for_each(|v| w.push(*v as f64));
    }
}

impl HostValue for Quat {
    fn kind() -> Kind {
        Kind::Quat
    }
    fn read(r: &mut SlotReader) -> Result<Self, String> {
        Ok(Quat::from_xyzw(r.slot()? as f32, r.slot()? as f32, r.slot()? as f32, r.slot()? as f32))
    }
    fn write(&self, w: &mut SlotWriter) {
        self.to_array().iter().for_each(|v| w.push(*v as f64));
    }
}

impl<T: HostValue> HostValue for Option<T> {
    fn kind() -> Kind {
        Kind::Option(Box::new(T::kind()))
    }
    fn read(r: &mut SlotReader) -> Result<Self, String> {
        let present = r.slot()? != 0.0;
        // The value's slots are always there, so a `None` still advances past them.
        let v = T::read(r);
        if present { v.map(Some) } else { Ok(None) }
    }
    fn write(&self, w: &mut SlotWriter) {
        match self {
            Some(v) => {
                w.push(1.0);
                v.write(w);
            }
            None => {
                w.push(0.0);
                for _ in 0..T::kind().slots() {
                    w.push(0.0);
                }
            }
        }
    }
}

impl HostValue for String {
    fn kind() -> Kind {
        Kind::Str
    }
    fn read(r: &mut SlotReader) -> Result<Self, String> {
        let n = r.slot()?;
        if n < 0.0 || n.fract() != 0.0 {
            return Err(format!("{n} is not a string length"));
        }
        let b = r.bytes(n as usize)?;
        String::from_utf8(b.to_vec()).map_err(|_| "a string argument is not UTF-8".to_string())
    }
    fn write(&self, w: &mut SlotWriter) {
        w.push(self.len() as f64);
        w.push_bytes(self.as_bytes());
    }
}

impl HostValue for Color {
    fn kind() -> Kind {
        Kind::Color
    }
    fn read(r: &mut SlotReader) -> Result<Self, String> {
        use bevy::color::{Hsla, Hsva, Hwba, Laba, Lcha, LinearRgba, Okhsla, Okhsva, Okhwba, Oklaba, Oklcha, Srgba, Xyza};
        let tag = r.slot()?;
        if tag == COLOR_HEX || tag == COLOR_JSON {
            let text = String::read(r)?;
            let v = if tag == COLOR_HEX {
                serde_json::Value::String(text)
            } else {
                serde_json::from_str(&text).map_err(|e| format!("a colour that is not JSON: {e}"))?
            };
            return crate::color::parse(&v).map_err(|e| if e.contains(crate::color::FORMS) { e } else { format!("{e}; {}", crate::color::FORMS) });
        }
        let mut c = [0f32; 4];
        for x in &mut c {
            *x = r.slot()? as f32;
        }
        let [a, b, d, e] = c;
        Ok(match tag as u32 {
            0 => Srgba::new(a, b, d, e).into(),
            1 => LinearRgba::new(a, b, d, e).into(),
            2 => Hsla::new(a, b, d, e).into(),
            3 => Hsva::new(a, b, d, e).into(),
            4 => Hwba::new(a, b, d, e).into(),
            5 => Laba::new(a, b, d, e).into(),
            6 => Lcha::new(a, b, d, e).into(),
            7 => Oklaba::new(a, b, d, e).into(),
            8 => Oklcha::new(a, b, d, e).into(),
            9 => Xyza::new(a, b, d, e).into(),
            10 => Okhsla::new(a, b, d, e).into(),
            11 => Okhsva::new(a, b, d, e).into(),
            12 => Okhwba::new(a, b, d, e).into(),
            _ => return Err(format!("{tag} is not a colour tag")),
        })
    }
    fn write(&self, w: &mut SlotWriter) {
        let (tag, c) = match *self {
            Color::Srgba(c) => (0, [c.red, c.green, c.blue, c.alpha]),
            Color::LinearRgba(c) => (1, [c.red, c.green, c.blue, c.alpha]),
            Color::Hsla(c) => (2, [c.hue, c.saturation, c.lightness, c.alpha]),
            Color::Hsva(c) => (3, [c.hue, c.saturation, c.value, c.alpha]),
            Color::Hwba(c) => (4, [c.hue, c.whiteness, c.blackness, c.alpha]),
            Color::Laba(c) => (5, [c.lightness, c.a, c.b, c.alpha]),
            Color::Lcha(c) => (6, [c.lightness, c.chroma, c.hue, c.alpha]),
            Color::Oklaba(c) => (7, [c.lightness, c.a, c.b, c.alpha]),
            Color::Oklcha(c) => (8, [c.lightness, c.chroma, c.hue, c.alpha]),
            Color::Xyza(c) => (9, [c.x, c.y, c.z, c.alpha]),
            Color::Okhsla(c) => (10, [c.hue, c.saturation, c.lightness, c.alpha]),
            Color::Okhsva(c) => (11, [c.hue, c.saturation, c.value, c.alpha]),
            Color::Okhwba(c) => (12, [c.hue, c.whiteness, c.blackness, c.alpha]),
        };
        w.push(tag as f64);
        c.iter().for_each(|v| w.push(*v as f64));
    }
}

impl HostValue for Isometry3d {
    fn kind() -> Kind {
        Kind::Isometry3d
    }
    fn read(r: &mut SlotReader) -> Result<Self, String> {
        let t = Vec3::read(r)?;
        Ok(Isometry3d::new(t, Quat::read(r)?))
    }
    fn write(&self, w: &mut SlotWriter) {
        Vec3::from(self.translation).write(w);
        self.rotation.write(w);
    }
}

impl HostValue for Isometry2d {
    fn kind() -> Kind {
        Kind::Isometry2d
    }
    fn read(r: &mut SlotReader) -> Result<Self, String> {
        let t = Vec2::read(r)?;
        let (cos, sin) = (r.slot()? as f32, r.slot()? as f32);
        if !(cos.is_finite() && sin.is_finite()) || ((cos * cos + sin * sin) - 1.0).abs() > 1e-3 {
            return Err(format!("{{ cos: {cos}, sin: {sin} }} is not a rotation: cos² + sin² must be 1 (Rot2::radians(a) gives one)"));
        }
        Ok(Isometry2d::new(t, Rot2::from_sin_cos(sin, cos)))
    }
    fn write(&self, w: &mut SlotWriter) {
        self.translation.write(w);
        w.push(self.rotation.cos as f64);
        w.push(self.rotation.sin as f64);
    }
}

/// A `Transform` or a `GlobalTransform` from TS, kept as given: Bevy's `impl TransformPoint`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum HostTransform {
    Transform(Transform),
    Global(GlobalTransform),
}

impl bevy::transform::TransformPoint for HostTransform {
    fn transform_point(&self, point: impl Into<Vec3>) -> Vec3 {
        match self {
            HostTransform::Transform(t) => t.transform_point(point.into()),
            HostTransform::Global(g) => g.transform_point(point.into()),
        }
    }
}

impl HostValue for HostTransform {
    fn kind() -> Kind {
        Kind::TransformPoint
    }
    /// A tag slot, then 12: a `Transform`'s translation, rotation, scale and two unused, or a
    /// `GlobalTransform`'s affine columns.
    fn read(r: &mut SlotReader) -> Result<Self, String> {
        let tag = r.slot()?;
        let mut v = [0f32; 12];
        for x in &mut v {
            *x = r.slot()? as f32;
        }
        match tag as u32 {
            0 => Ok(HostTransform::Transform(Transform {
                translation: Vec3::from_slice(&v[0..3]),
                rotation: Quat::from_slice(&v[3..7]),
                scale: Vec3::from_slice(&v[7..10]),
            })),
            1 => Ok(HostTransform::Global(GlobalTransform::from(bevy::math::Affine3A::from_cols_array(&v)))),
            _ => Err(format!("{tag} is not a transform tag")),
        }
    }
    fn write(&self, w: &mut SlotWriter) {
        match self {
            HostTransform::Transform(t) => {
                w.push(0.0);
                t.translation.write(w);
                t.rotation.write(w);
                t.scale.write(w);
                w.push(0.0);
                w.push(0.0);
            }
            HostTransform::Global(g) => {
                w.push(1.0);
                g.affine().to_cols_array().iter().for_each(|v| w.push(*v as f64));
            }
        }
    }
}

impl HostValue for Transform {
    fn kind() -> Kind {
        Kind::TransformPoint
    }
    /// A `GlobalTransform` is decomposed into scale, rotation and translation.
    fn read(r: &mut SlotReader) -> Result<Self, String> {
        Ok(match HostTransform::read(r)? {
            HostTransform::Transform(t) => t,
            HostTransform::Global(g) => g.compute_transform(),
        })
    }
    fn write(&self, w: &mut SlotWriter) {
        HostTransform::Transform(*self).write(w);
    }
}

fn lane(v: f64, what: &str) -> Result<u32, String> {
    if v < 0.0 || v.fract() != 0.0 || v > u32::MAX as f64 || v.is_nan() {
        return Err(format!("{what} {v} is not a whole number from 0 to {}", u32::MAX));
    }
    Ok(v as u32)
}

impl HostValue for UVec2 {
    fn kind() -> Kind {
        Kind::UVec2
    }
    fn read(r: &mut SlotReader) -> Result<Self, String> {
        Ok(UVec2::new(lane(r.slot()?, "x")?, lane(r.slot()?, "y")?))
    }
    fn write(&self, w: &mut SlotWriter) {
        self.to_array().iter().for_each(|v| w.push(*v as f64));
    }
}

impl HostValue for UVec3 {
    fn kind() -> Kind {
        Kind::UVec3
    }
    fn read(r: &mut SlotReader) -> Result<Self, String> {
        Ok(UVec3::new(lane(r.slot()?, "x")?, lane(r.slot()?, "y")?, lane(r.slot()?, "z")?))
    }
    fn write(&self, w: &mut SlotWriter) {
        self.to_array().iter().for_each(|v| w.push(*v as f64));
    }
}

/// A tuple of host values: an element of a [`HostList`].
pub trait HostTuple: Sized {
    fn kinds() -> Vec<Kind>;
    fn read(r: &mut SlotReader) -> Result<Self, String>;
    fn write(&self, w: &mut SlotWriter);
}

impl<A: HostValue, B: HostValue> HostTuple for (A, B) {
    fn kinds() -> Vec<Kind> {
        vec![A::kind(), B::kind()]
    }
    fn read(r: &mut SlotReader) -> Result<Self, String> {
        Ok((A::read(r)?, B::read(r)?))
    }
    fn write(&self, w: &mut SlotWriter) {
        self.0.write(w);
        self.1.write(w);
    }
}

impl<A: HostValue, B: HostValue, C: HostValue> HostTuple for (A, B, C) {
    fn kinds() -> Vec<Kind> {
        vec![A::kind(), B::kind(), C::kind()]
    }
    fn read(r: &mut SlotReader) -> Result<Self, String> {
        Ok((A::read(r)?, B::read(r)?, C::read(r)?))
    }
    fn write(&self, w: &mut SlotWriter) {
        self.0.write(w);
        self.1.write(w);
        self.2.write(w);
    }
}

/// A list of tuples (`[[p, color], ...]`): a count slot, then the elements, within the call's
/// [`SLOTS`]. For many plain points use [`HostPoints`], which has no bound.
#[derive(Clone, Debug, PartialEq)]
pub struct HostList<T>(pub Vec<T>);

impl<T: HostTuple> HostValue for HostList<T> {
    fn kind() -> Kind {
        Kind::List(T::kinds())
    }
    fn read(r: &mut SlotReader) -> Result<Self, String> {
        let n = lane(r.slot()?, "a list length")? as usize;
        (0..n).map(|_| T::read(r)).collect::<Result<Vec<_>, _>>().map(HostList)
    }
    fn write(&self, w: &mut SlotWriter) {
        w.push(self.0.len() as f64);
        self.0.iter().for_each(|t| t.write(w));
    }
}

/// A JSON value: its text in a string. TS types it `unknown`; a hand-written export may name a
/// type with [`Kind::Json`].
#[derive(Clone, Debug, PartialEq)]
pub struct HostJson(pub serde_json::Value);

impl HostValue for HostJson {
    fn kind() -> Kind {
        Kind::Json("unknown")
    }
    fn read(r: &mut SlotReader) -> Result<Self, String> {
        let text = String::read(r)?;
        serde_json::from_str(&text).map(HostJson).map_err(|e| format!("not JSON: {e}"))
    }
    fn write(&self, w: &mut SlotWriter) {
        self.0.to_string().write(w);
    }
}

/// A point type of a [`HostPoints`]: its lanes, `f32` each.
pub trait PointLanes: Copy + 'static {
    const LANES: usize;
    fn from_lanes(l: &[f32]) -> Self;
}

impl PointLanes for f32 {
    const LANES: usize = 1;
    fn from_lanes(l: &[f32]) -> Self {
        l[0]
    }
}

impl PointLanes for Vec2 {
    const LANES: usize = 2;
    fn from_lanes(l: &[f32]) -> Self {
        Vec2::from_slice(l)
    }
}

impl PointLanes for Vec3 {
    const LANES: usize = 3;
    fn from_lanes(l: &[f32]) -> Self {
        Vec3::from_slice(l)
    }
}

/// Points from TS (`Vec3[]` or a `Float32Array` of 3 per point), viewed in place.
#[derive(Clone, Copy, Debug)]
pub struct HostPoints<'a, T> {
    floats: &'a [f32],
    marker: std::marker::PhantomData<T>,
}

impl<'a, T: PointLanes> HostPoints<'a, T> {
    /// The points in a slice argument's bytes; a length that is not whole points is refused.
    pub fn new(bytes: &'a [u8], what: &str) -> Result<Self, String> {
        let floats = slice_ref::<f32>(bytes, what)?;
        if !floats.len().is_multiple_of(T::LANES) {
            return Err(format!("{what}: {} floats is not a whole number of {}-float points", floats.len(), T::LANES));
        }
        Ok(HostPoints { floats, marker: std::marker::PhantomData })
    }

    pub fn len(&self) -> usize {
        self.floats.len() / T::LANES
    }

    pub fn is_empty(&self) -> bool {
        self.floats.is_empty()
    }

    // `as_chunks::<{ T::LANES }>` needs a generic const expression, which stable Rust lacks.
    #[allow(clippy::chunks_exact_to_as_chunks)]
    pub fn iter(&self) -> impl Iterator<Item = T> + 'a {
        self.floats.chunks_exact(T::LANES).map(T::from_lanes)
    }
}

/// An argument of a hand-written export's method: a [`HostValue`] read from the slots, or a
/// [`HostPoints`] from the next slice. `#[export]` generates the same reads.
pub trait HostArg<'a>: Sized {
    fn spec(name: &'static str) -> ArgSpec;
    /// Reads the argument named `name`, from the slots or the next of `slices`.
    fn take(r: &mut SlotReader, slices: &mut dyn Iterator<Item = &'a [u8]>, name: &str) -> Result<Self, String>;
}

impl<'a, T: HostValue> HostArg<'a> for T {
    fn spec(name: &'static str) -> ArgSpec {
        ArgSpec::Value(name, T::kind())
    }
    fn take(r: &mut SlotReader, _: &mut dyn Iterator<Item = &'a [u8]>, name: &str) -> Result<Self, String> {
        T::read(r).map_err(|e| format!("{name}: {e}"))
    }
}

impl<'a, P: PointLanes> HostArg<'a> for HostPoints<'a, P> {
    fn spec(name: &'static str) -> ArgSpec {
        ArgSpec::Points(SliceSpec { name, view: "Float32Array", mutable: false }, P::LANES as u8)
    }
    fn take(_: &mut SlotReader, slices: &mut dyn Iterator<Item = &'a [u8]>, name: &str) -> Result<Self, String> {
        HostPoints::new(slices.next().ok_or("missing slice")?, name)
    }
}

impl HostValue for () {
    fn kind() -> Kind {
        Kind::Unit
    }
    fn read(_: &mut SlotReader) -> Result<Self, String> {
        Ok(())
    }
    fn write(&self, _: &mut SlotWriter) {}
}

/// An element type of a slice argument: plain data viewed in place.
///
/// # Safety
/// Every bit pattern of `size_of::<Self>()` bytes must be a valid `Self`.
#[diagnostic::on_unimplemented(
    message = "`{Self}` cannot be a slice element of an exported method",
    note = "slices hold numbers or NativeLayout records of numbers and vectors"
)]
pub unsafe trait HostElem: Sized + 'static {
    /// The TS view type for a slice of it.
    fn view() -> &'static str;
}

macro_rules! elems {
    ($($t:ty => $v:literal),*) => {
        $(unsafe impl HostElem for $t {
            fn view() -> &'static str {
                $v
            }
        })*
    };
}
elems!(f32 => "Float32Array", f64 => "Float64Array", i8 => "Int8Array", u8 => "Uint8Array", i16 => "Int16Array", u16 => "Uint16Array", i32 => "Int32Array", u32 => "Uint32Array", Vec2 => "Float32Array", Vec3 => "Float32Array", Vec4 => "Float32Array");

fn cast_check<T>(bytes: &[u8], what: &str) -> Result<usize, String> {
    let size = size_of::<T>().max(1);
    if !bytes.len().is_multiple_of(size) {
        return Err(format!("{what}: {} bytes is not a whole number of {size}-byte elements", bytes.len()));
    }
    if !(bytes.as_ptr() as usize).is_multiple_of(align_of::<T>()) {
        return Err(format!("{what}: the array is not aligned to {} bytes", align_of::<T>()));
    }
    Ok(bytes.len() / size)
}

/// Views JS memory as `&[T]` in place.
pub fn slice_ref<'a, T: HostElem>(bytes: &'a [u8], what: &str) -> Result<&'a [T], String> {
    // An empty JS array has no aligned address to give; it needs none.
    if bytes.is_empty() {
        return Ok(&[]);
    }
    let n = cast_check::<T>(bytes, what)?;
    // SAFETY: length and alignment checked; any bit pattern is a valid T (HostElem).
    Ok(unsafe { std::slice::from_raw_parts(bytes.as_ptr() as *const T, n) })
}

/// Views JS memory as `&mut [T]` in place.
pub fn slice_mut<'a, T: HostElem>(bytes: &'a mut [u8], what: &str) -> Result<&'a mut [T], String> {
    if bytes.is_empty() {
        return Ok(&mut []);
    }
    let n = cast_check::<T>(bytes, what)?;
    // SAFETY: as above, and the bytes are exclusively borrowed.
    Ok(unsafe { std::slice::from_raw_parts_mut(bytes.as_mut_ptr() as *mut T, n) })
}

/// The state an exported param keeps, type-erased: an [`Exported`].
pub type ParamState = Box<dyn ExportedState>;

/// What the host does with a param's state without knowing its type.
pub trait ExportedState: Any + Send + Sync {
    /// Applies deferred state (`Commands`, `Gizmos`) to the world.
    fn apply(&mut self, world: &mut World);
    /// Whether the param has deferred state at all.
    fn deferred(&self) -> bool;
    /// The world access the param registers (`init_access`).
    fn access(&self, world: &mut World) -> SystemAccess;
    fn as_any_mut(&mut self) -> &mut dyn Any;
}

impl<P: SystemParam + 'static> ExportedState for Exported<P> {
    fn apply(&mut self, world: &mut World) {
        self.state.apply(world);
    }
    fn deferred(&self) -> bool {
        self.meta.has_deferred()
    }
    fn access(&self, world: &mut World) -> SystemAccess {
        let mut meta = self.meta.clone();
        let mut access = SystemAccess::default();
        P::init_access(self.state.param_state(), &mut meta, &mut access, world);
        access
    }
    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }
}

/// An exported param's state: its `SystemState` and a meta whose `last_run` each call sets to
/// the calling TS system's.
pub struct Exported<P: SystemParam + 'static> {
    pub state: SystemState<P>,
    pub meta: SystemMeta,
}

/// The ticks of the run a call belongs to: the TS system's last run and this run's tick.
#[derive(Clone, Copy, Debug)]
pub struct RunTicks {
    pub last_run: Tick,
    pub this_run: Tick,
}

/// A column or resource row the run exposes to JS: its bytes and the component it holds.
#[derive(Clone, Copy, Debug)]
pub struct Region {
    pub start: usize,
    pub end: usize,
    pub id: ComponentId,
}

/// One method's host function.
pub type HostFn = fn(&mut ParamState, UnsafeWorldCell<'_>, RunTicks, &mut SlotReader, &mut SlotWriter, &mut [&mut [u8]]) -> Result<(), String>;

/// A param's state, for `ParamSpec::init`.
pub fn init<P: SystemParam + 'static>(world: &mut World) -> ParamState {
    let state = SystemState::<P>::new(world);
    let meta = state.meta().clone();
    Box::new(Exported { state, meta })
}

/// Fetches a param for one call at the run's ticks. Spends no world tick.
///
/// # Safety
/// `world` is the world the state was made with, the caller holds it exclusively for the run,
/// nothing else reads or writes the data `P`'s access names for the item's life, and `P`'s
/// access was checked by `init_access` (as [`init`] does).
pub unsafe fn fetch<'w, 's, P: SystemParam + 'static>(state: &'s mut ParamState, world: UnsafeWorldCell<'w>, ticks: RunTicks) -> Result<SystemParamItem<'w, 's, P>, String> {
    let s = state.as_any_mut().downcast_mut::<Exported<P>>().ok_or("exported param state has the wrong type")?;
    if !s.state.matches_world(world.id()) {
        return Err("exported param state belongs to another world".into());
    }
    s.meta.set_last_run(ticks.last_run);
    // SAFETY: the caller's contract; the state is only borrowed for this call, as a `Mut` the
    // param holds; the param state is only handed to `P` itself.
    unsafe { P::get_param(s.state.param_state_mut(), &s.meta, world, ticks.this_run) }.map_err(|e| e.to_string())
}

/// A slice argument: name, TS view type, whether Rust writes it.
#[derive(Clone, Debug)]
pub struct SliceSpec {
    pub name: &'static str,
    pub view: &'static str,
    pub mutable: bool,
}

/// One argument in declaration order.
#[derive(Clone, Debug)]
pub enum ArgSpec {
    Value(&'static str, Kind),
    Slice(SliceSpec),
    /// A point list: a slice of `f32`, `lanes` per point ([`HostPoints`]).
    Points(SliceSpec, u8),
}

impl ArgSpec {
    /// The slice this argument crosses as, if any.
    pub fn slice(&self) -> Option<&SliceSpec> {
        match self {
            ArgSpec::Slice(s) | ArgSpec::Points(s, _) => Some(s),
            ArgSpec::Value(..) => None,
        }
    }
}

#[derive(Clone)]
pub struct MethodSpec {
    /// The TS name (`castRay`).
    pub name: String,
    pub args: Vec<ArgSpec>,
    pub ret: Kind,
    pub call: HostFn,
    /// The builder this method returns (`EllipseBuilder`): JS gets the param object back, and TS
    /// types it as the builder. Its result slots are unused.
    pub chain: Option<&'static str>,
    /// The builder this method is a setting of: TS declares it on the builder, not the param,
    /// and JS returns the param object. Settings of one name share one JS method, the first
    /// declared, so every builder's setting of that name takes the same slots.
    pub of: Option<&'static str>,
}

/// What `#[export]` generates for a param.
#[derive(Clone)]
pub struct ParamSpec {
    /// The TS type (`ReadRapierContext`).
    pub ts: &'static str,
    /// The built-in module that declares it (`bevy-ts/rapier`).
    pub module: &'static str,
    pub rust: &'static str,
    /// The Bevy spelling the export stands for (`ResMut<Time<Virtual>>`), normalised: a TS
    /// param written that way is this export.
    pub stands_for: Option<&'static str>,
    pub init: fn(&mut World) -> ParamState,
    pub methods: Vec<MethodSpec>,
    /// The module's own `.d.ts` declares the param's type (one generic `Gizmos<G>` over an
    /// export per group, with [`declaration_as`]), so [`HostParams::declarations`] leaves it out.
    pub module_declares: bool,
}

/// The JS slot capacity for one call's arguments and results.
pub const SLOTS: usize = 512;
/// At most this many slice arguments per method.
pub const MAX_SLICES: usize = 4;

struct Registered {
    spec: ParamSpec,
    state: ParamState,
    /// Host function id of the first method.
    first: u32,
    /// Components and resources the param may write, sorted.
    writes: Vec<ComponentId>,
    /// Called since its deferred state was last applied or dropped.
    dirty: bool,
}

/// Every exported param, with its state and host function ids.
#[derive(Resource, Default)]
pub struct HostParams {
    params: Vec<Registered>,
    /// Host function id to (param, method).
    fns: Vec<(usize, usize)>,
    /// Calls from JS into Rust so far, for tests and stats.
    pub calls: u64,
}

/// Param kind names every engine in this process exports, for the compile step.
static KINDS: OnceLock<Mutex<Vec<String>>> = OnceLock::new();
/// Claimed spellings to the TS name of the export claiming them.
static CLAIMS: OnceLock<Mutex<Vec<(String, String)>>> = OnceLock::new();

/// The exported param type names the compile step reads as system param kinds.
pub fn param_kinds() -> Vec<String> {
    KINDS.get().map(|k| k.lock().unwrap().clone()).unwrap_or_default()
}

/// Claimed Bevy spellings (`ResMut<Time<Virtual>>`) and the TS name of the export claiming each.
pub fn claims() -> Vec<(String, String)> {
    CLAIMS.get().map(|k| k.lock().unwrap().clone()).unwrap_or_default()
}

/// A Bevy param spelling, normalised: no whitespace, `Res`/`ResMut` and generic arguments by
/// their last path segment (`bevy::time::Time<bevy::time::Virtual>` is `Time<Virtual>`).
pub fn normalise_spelling(s: &str) -> String {
    let mut out = String::new();
    let mut word = String::new();
    for c in s.chars().filter(|c| !c.is_whitespace()) {
        if c.is_alphanumeric() || c == '_' || c == ':' {
            word.push(c);
        } else {
            out.push_str(word.rsplit("::").next().unwrap_or(""));
            word.clear();
            out.push(c);
        }
    }
    out.push_str(word.rsplit("::").next().unwrap_or(""));
    out
}

/// The components and resources `access` may write, or why it is refused.
fn bounded_writes(rust: &str, access: &SystemAccess) -> Result<Vec<ComponentId>, String> {
    let refuse = || {
        format!(
            "bevy-ts: {rust} cannot be exported to TS: it may write every component or resource (EntityMut, EntityMutExcept, DeferredWorld); name the components and resources it writes"
        )
    };
    match access {
        SystemAccess::None => Ok(Vec::new()),
        SystemAccess::Exclusive => Err(refuse()),
        SystemAccess::Shared(set) => {
            let mut out: Vec<ComponentId> = set.combined_access().writes().as_finite_set().ok_or_else(refuse)?.iter().collect();
            for f in set.filtered_accesses() {
                out.extend(f.access().writes().as_finite_set().ok_or_else(refuse)?.iter());
            }
            out.sort();
            out.dedup();
            Ok(out)
        }
    }
}

impl HostParams {
    /// Registers a param. Fails on a duplicate TS name, a second claim of a spelling, or a param
    /// that may write every component or resource.
    pub fn register(&mut self, world: &mut World, spec: ParamSpec) -> Result<(), String> {
        if let Some(prev) = self.params.iter().find(|p| p.spec.ts == spec.ts) {
            return Err(format!("bevy-ts: {} is exported twice ({} and {})", spec.ts, prev.spec.rust, spec.rust));
        }
        if let Some(claim) = spec.stands_for
            && let Some(prev) = self.params.iter().find(|p| p.spec.stands_for == Some(claim))
        {
            return Err(format!("bevy-ts: {claim} is claimed by two exports ({} and {}); one export per spelling", prev.spec.rust, spec.rust));
        }
        assert!(
            spec.methods.iter().all(|m| m.args.iter().filter(|a| a.slice().is_some()).count() <= MAX_SLICES),
            "{}: at most {MAX_SLICES} slice arguments per method",
            spec.ts
        );
        let state = (spec.init)(world);
        let writes = bounded_writes(spec.rust, &state.access(world))?;
        let first = self.fns.len() as u32;
        let index = self.params.len();
        for m in 0..spec.methods.len() {
            self.fns.push((index, m));
        }
        let kinds = KINDS.get_or_init(Default::default);
        let mut k = kinds.lock().unwrap();
        if !k.iter().any(|n| n == spec.ts) {
            k.push(spec.ts.to_string());
        }
        if let Some(claim) = spec.stands_for {
            let mut c = CLAIMS.get_or_init(Default::default).lock().unwrap();
            c.retain(|(s, _)| s != claim);
            c.push((claim.to_string(), spec.ts.to_string()));
        }
        self.params.push(Registered { spec, state, first, writes, dirty: false });
        Ok(())
    }

    pub fn get(&self, ts: &str) -> Option<&ParamSpec> {
        self.params.iter().find(|p| p.spec.ts == ts).map(|p| &p.spec)
    }

    /// The index of the param TS names `ts`.
    pub fn index(&self, ts: &str) -> Option<usize> {
        self.params.iter().position(|p| p.spec.ts == ts)
    }

    /// The TS name of the export claiming `spelling`.
    pub fn claimed(&self, spelling: &str) -> Option<&'static str> {
        let n = normalise_spelling(spelling);
        self.params.iter().find(|p| p.spec.stands_for == Some(n.as_str())).map(|p| p.spec.ts)
    }

    pub fn specs(&self) -> impl Iterator<Item = &ParamSpec> {
        self.params.iter().map(|p| &p.spec)
    }

    /// Components and resources param `index` may write.
    pub fn writes(&self, index: usize) -> &[ComponentId] {
        &self.params[index].writes
    }

    /// Applies param `index`'s deferred state if a call since the last apply queued any.
    pub fn apply(&mut self, index: usize, world: &mut World) {
        let reg = &mut self.params[index];
        if std::mem::take(&mut reg.dirty) && reg.state.deferred() {
            reg.state.apply(world);
        }
    }

    /// Drops param `index`'s deferred state from a failed run by rebuilding its state, which
    /// also resets its `Local`s and query cache.
    pub fn discard(&mut self, index: usize, world: &mut World) {
        let reg = &mut self.params[index];
        if std::mem::take(&mut reg.dirty) && reg.state.deferred() {
            reg.state = (reg.spec.init)(world);
        }
    }

    /// Applies, or on `ok == false` drops, every called param's deferred state, in
    /// registration order (an entry call, which has no parameter order).
    pub fn settle_all(&mut self, world: &mut World, ok: bool) {
        for i in 0..self.params.len() {
            if ok { self.apply(i, world) } else { self.discard(i, world) }
        }
    }

    /// Calls host function `id` at the run's ticks. A panic in the method becomes an error
    /// naming it; a slice overlapping `regions` that the call could alias is refused, and so is a
    /// param the running system does not take (`allowed`, by index; `None` for an entry, which
    /// may call any).
    #[allow(clippy::too_many_arguments)]
    pub fn call(
        &mut self,
        world: UnsafeWorldCell<'_>,
        ticks: RunTicks,
        regions: &[Region],
        allowed: Option<&[usize]>,
        id: u32,
        args: &[u8],
        ret: &mut [u8],
        slices: &mut [&mut [u8]],
    ) -> Result<usize, String> {
        self.calls += 1;
        let &(p, m) = self.fns.get(id as usize).ok_or_else(|| format!("no host function {id}"))?;
        let reg = &mut self.params[p];
        let method = &reg.spec.methods[m];
        let (ts, call) = (reg.spec.ts, method.call);
        // A param instance kept from another system's run would skip this run's overlap check
        // and settle with the wrong system.
        if allowed.is_some_and(|a| !a.contains(&p)) {
            return Err(format!("{ts}.{}: this system does not take {ts}; take it as a parameter of the system that calls it", method.name));
        }
        if !regions.is_empty() {
            let specs = method.args.iter().filter_map(ArgSpec::slice);
            for (spec, bytes) in specs.zip(slices.iter()) {
                let (start, end) = (bytes.as_ptr() as usize, bytes.as_ptr() as usize + bytes.len());
                if start == end {
                    continue;
                }
                if let Some(r) = regions.iter().find(|r| start < r.end && r.start < end && (spec.mutable || reg.writes.binary_search(&r.id).is_ok())) {
                    let why = if spec.mutable { "it is a &mut slice" } else { "the param may write that column" };
                    return Err(format!("{ts}.{}: argument {} views a column or resource row of this run ({:?}), and {why}; copy it first", method.name, spec.name, r.id));
                }
            }
        }
        reg.dirty = true;
        let state = &mut reg.state;
        let mut reader = SlotReader::new(args);
        let mut writer = SlotWriter::new(ret);
        let r = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| call(state, world, ticks, &mut reader, &mut writer, slices)));
        let name = || reg.spec.methods[m].name.as_str();
        match r {
            Ok(Ok(())) => writer.finish().map_err(|e| format!("{ts}.{}: {e}", name())),
            Ok(Err(e)) => Err(format!("{ts}.{}: {e}", name())),
            Err(p) => {
                let msg = p.downcast_ref::<String>().cloned().or_else(|| p.downcast_ref::<&str>().map(|s| s.to_string())).unwrap_or_default();
                Err(format!("{ts}.{} panicked: {msg}", name()))
            }
        }
    }

    /// The JS glue for every param, by TS name: a function of the host object returning the
    /// param's class.
    pub fn glue(&self) -> BTreeMap<String, String> {
        self.params.iter().map(|r| (r.spec.ts.to_string(), glue(&r.spec, r.first))).collect()
    }

    /// TS declarations of every param, grouped by module: the records and enums the module's
    /// params use, once each, then the params.
    /// Each module's declarations of its params and the records they use, with the import of
    /// what they name from `bevy-ts`.
    pub fn declarations(&self) -> BTreeMap<&'static str, String> {
        let mut types: BTreeMap<&'static str, BTreeMap<&'static str, String>> = BTreeMap::new();
        let mut params: BTreeMap<&'static str, String> = BTreeMap::new();
        let mut entity: BTreeSet<&'static str> = BTreeSet::new();
        for r in self.params.iter().filter(|r| !r.spec.module_declares) {
            named_types(&r.spec, types.entry(r.spec.module).or_default());
            params.entry(r.spec.module).or_default().push_str(&declaration(&r.spec));
            let kinds = r.spec.methods.iter().flat_map(|m| m.args.iter().filter_map(|a| if let ArgSpec::Value(_, k) = a { Some(k) } else { None }).chain([&m.ret]));
            if kinds.into_iter().any(Kind::names_entity) {
                entity.insert(r.spec.module);
            }
        }
        params
            .into_iter()
            .map(|(m, p)| {
                let import = if entity.contains(m) { "import type { Entity } from \"bevy-ts\";\n" } else { "" };
                (m, format!("{import}{}{p}", types.remove(m).unwrap_or_default().into_values().collect::<String>()))
            })
            .collect()
    }
}

/// The engine-side end of the seam during a run: the run's world, held exclusively for the run,
/// its ticks, the columns it exposes to JS, and the params.
pub struct WorldHost<'a> {
    pub world: UnsafeWorldCell<'a>,
    pub ticks: RunTicks,
    pub regions: &'a [Region],
    /// The params the running system takes, by index; `None` for an entry call.
    pub allowed: Option<&'a [usize]>,
    pub params: &'a mut HostParams,
    /// Pool ids `reserve` handed JS during the call, in order, after the call's pool.
    pub reserved: Vec<u64>,
    /// `reserve` calls made during the call.
    pub reserves: u64,
}

/// The host while a generation's modules evaluate: no Rust params (no system runs), only the
/// bridge's JSON services, so a scene's first render can call `assets`.
pub struct LoadHost<'a> {
    pub world: &'a World,
}

impl crate::backend::HostCalls for LoadHost<'_> {
    fn call(&mut self, _id: u32, _args: &[u8], _ret: &mut [u8], _slices: &mut [&mut [u8]]) -> Result<usize, String> {
        Err("Rust params can only be called while a system runs".into())
    }

    #[cfg_attr(not(feature = "pbr"), allow(unused_variables))]
    fn call_json(&mut self, name: &str, args: &str) -> Result<String, String> {
        match name.split_once('.') {
            #[cfg(feature = "reflect")]
            Some(("assets", _)) => crate::reflect_asset::host_call(self.world, name, args),
            _ => Err(format!("no host service {name}")),
        }
    }
}

impl crate::backend::HostCalls for WorldHost<'_> {
    fn call(&mut self, id: u32, args: &[u8], ret: &mut [u8], slices: &mut [&mut [u8]]) -> Result<usize, String> {
        if id == crate::commands::RESERVE {
            // `reserve(n)`: n more entity ids, as f64 slots (the pool's representation).
            let n = args.get(..8).map_or(0.0, |b| f64::from_le_bytes(b.try_into().unwrap())) as usize;
            let n = n.min(ret.len() / 8);
            self.reserves += 1;
            let alloc = self.world.entity_allocator();
            for i in 0..n {
                let bits = alloc.alloc().to_bits();
                self.reserved.push(bits);
                ret[i * 8..i * 8 + 8].copy_from_slice(&(bits as f64).to_le_bytes());
            }
            return Ok(n);
        }
        #[cfg(feature = "reflect")]
        if id >= crate::reflect_codec::READ {
            return crate::reflect_codec::call(self.world, self.ticks.this_run, id, args, ret, slices);
        }
        self.params.call(self.world, self.ticks, self.regions, self.allowed, id, args, ret, slices)
    }

    #[cfg_attr(not(feature = "pbr"), allow(unused_variables))]
    fn call_json(&mut self, name: &str, args: &str) -> Result<String, String> {
        match name.split_once('.') {
            #[cfg(feature = "reflect")]
            // SAFETY: the asset calls read resources only (the registry, `ScriptAssets`, the asset
            // server, `Assets<T>`), never a table a running system's regions point into, and they
            // change nothing: what they make waits in `ScriptAssets` for the run's end.
            Some(("assets", _)) => crate::reflect_asset::host_call(unsafe { self.world.world() }, name, args),
            _ => Err(format!("no host service {name}")),
        }
    }
}

fn write_js(kind: &Kind, expr: &str, out: &mut String) {
    match kind {
        Kind::Num(_) => {
            let _ = write!(out, "a[i++] = +{expr};");
        }
        Kind::Bool => {
            let _ = write!(out, "a[i++] = {expr} ? 1 : 0;");
        }
        Kind::Entity => {
            let _ = write!(out, "a[i++] = Number({expr});");
        }
        Kind::Vec2 | Kind::Vec3 | Kind::Quat => {
            let n = kind.slots();
            let lanes = ["x", "y", "z", "w"];
            let _ = write!(out, "{{ const v = {expr}; if (Array.isArray(v)) {{ ");
            for i in 0..n {
                let _ = write!(out, "a[i++] = v[{i}];");
            }
            out.push_str(" } else { ");
            for l in &lanes[..n] {
                let _ = write!(out, "a[i++] = v.{l};");
            }
            out.push_str(" } }");
        }
        Kind::Record(_, fields) => {
            let _ = write!(out, "{{ const r = {expr} || {{}}; ");
            for (f, k) in fields {
                // A missing optional field is `None`, a missing enum its first variant, any other
                // missing field zero.
                let e = match k {
                    Kind::Option(_) => format!("r[{f:?}]"),
                    Kind::Enum(_, variants) => format!("(r[{f:?}] ?? {:?})", variants[0]),
                    _ => format!("(r[{f:?}] ?? 0)"),
                };
                write_js(k, &e, out);
            }
            out.push('}');
        }
        Kind::Enum(name, variants) => {
            let _ = write!(
                out,
                "{{ const k = {variants:?}.indexOf({expr}); if (k < 0) throw new TypeError(`${{{expr}}} is not a {name}: give one of {}`); a[i++] = k; }}",
                variants.join(", ")
            );
        }
        Kind::Option(k) => {
            // A `None`'s value slots are zeroed: a string's length slot must read as empty.
            let _ = write!(out, "{{ const o = {expr}; if (o === null || o === undefined) {{ a[i++] = 0; a.fill(0, i, i + {n}); i += {n}; }} else {{ a[i++] = 1; ", n = k.slots());
            write_js(k, "o", out);
            out.push_str(" } }");
        }
        Kind::Str => {
            let _ = write!(out, "i = H.str({expr}, i);");
        }
        Kind::UVec2 => write_js(&Kind::Vec2, expr, out),
        Kind::UVec3 => write_js(&Kind::Vec3, expr, out),
        Kind::Color => {
            let _ = write!(out, "i = H.color({expr}, i);");
        }
        Kind::Isometry2d => {
            let _ = write!(out, "i = H.iso2({expr}, i);");
        }
        Kind::Isometry3d => {
            let _ = write!(out, "i = H.iso3({expr}, i);");
        }
        Kind::TransformPoint => {
            let _ = write!(out, "i = H.transform({expr}, i);");
        }
        Kind::List(ks) => {
            let _ = write!(out, "{{ const l = {expr}; a[i++] = l.length; for (const e of l) {{ ");
            for (n, k) in ks.iter().enumerate() {
                write_js(k, &format!("e[{n}]"), out);
            }
            out.push_str(" } }");
        }
        Kind::Json(_) => {
            let _ = write!(out, "i = H.str(JSON.stringify({expr}), i);");
        }
        Kind::Unit => {}
    }
}

/// Whether a kind's slots vary with its value (strings, lists), so a call can run out of them.
fn varies(k: &Kind) -> bool {
    match k {
        Kind::Str | Kind::Color | Kind::List(_) | Kind::Json(_) => true,
        Kind::Option(k) => varies(k),
        Kind::Record(_, fields) => fields.iter().any(|(_, k)| varies(k)),
        _ => false,
    }
}

/// JS that reads a result from `a` at `i` (advancing it) into a fresh or reused value.
fn read_js(kind: &Kind) -> String {
    match kind {
        Kind::Num(_) => "a[i++]".into(),
        Kind::Bool => "a[i++] !== 0".into(),
        Kind::Entity => "a[i++]".into(),
        Kind::Vec2 => "({ x: a[i++], y: a[i++] })".into(),
        Kind::Vec3 => "({ x: a[i++], y: a[i++], z: a[i++] })".into(),
        Kind::Quat => "({ x: a[i++], y: a[i++], z: a[i++], w: a[i++] })".into(),
        Kind::Record(_, fields) => {
            let parts: Vec<String> = fields.iter().map(|(f, k)| format!("{f:?}: {}", read_js(k))).collect();
            format!("({{ {} }})", parts.join(", "))
        }
        Kind::Enum(_, variants) => format!("{variants:?}[a[i++]]"),
        Kind::Option(k) => format!("(a[i++] !== 0 ? {} : (i += {}, undefined))", read_js(k), k.slots()),
        Kind::Str => "((t) => ((i = t[1]), t[0]))(H.text(i))".into(),
        Kind::Json(_) => format!("JSON.parse({})", read_js(&Kind::Str)),
        Kind::UVec2 => read_js(&Kind::Vec2),
        Kind::UVec3 => read_js(&Kind::Vec3),
        Kind::Color | Kind::Isometry2d | Kind::Isometry3d | Kind::TransformPoint | Kind::List(_) => "undefined".into(),
        Kind::Unit => "undefined".into(),
    }
}

/// The JS class for one param: `H` is the host object (`call`, `error`, the slot scratch).
fn glue(spec: &ParamSpec, first: u32) -> String {
    let mut s = format!("const NAME = {:?};\nreturn class {} {{\n", spec.ts, spec.ts);
    let mut seen = std::collections::HashSet::new();
    for (m, method) in spec.methods.iter().enumerate() {
        // Settings of one name on several builders share the first one's JS method.
        if !seen.insert(method.name.as_str()) {
            continue;
        }
        let id = first as usize + m;
        // Arguments are `$`-prefixed in JS so none meets the glue's own `a`, `i`, `n` and `r`.
        let names: Vec<String> = method
            .args
            .iter()
            .map(|a| match a {
                ArgSpec::Value(n, _) => format!("${n}"),
                ArgSpec::Slice(sl) | ArgSpec::Points(sl, _) => format!("${}", sl.name),
            })
            .collect();
        let _ = writeln!(s, "  {}({}) {{", method.name, names.join(", "));
        // A setting on a builder a later builder call replaced throws (design 5); the buffer
        // check in Rust cannot tell two builders apart.
        if method.of.is_some() {
            let _ = writeln!(s, "    H.live(this, {:?});", method.name);
        }
        let slices = write_args(method, &mut s);
        let _ = writeln!(s, "\n    const n = H.call({id}, i, {});", slices.join(", "));
        let _ = writeln!(s, "    if (n < 0) H.fail(NAME, {:?});", method.name);
        if let Some(builder) = method.chain {
            let _ = writeln!(s, "    return H.open(this, {builder:?});\n  }}");
        } else if method.of.is_some() {
            s.push_str("    return this;\n  }\n");
        } else {
            let _ = writeln!(s, "    i = 0; const r = H.result;\n    {{ const a = r; return {}; }}\n  }}", read_js(&method.ret));
        }
    }
    s.push_str("};\n");
    s
}

/// JS that writes `method`'s arguments into `H.slots` from slot 0, leaving `i` past them; returns
/// the slice arguments' expressions, padded to [`MAX_SLICES`].
fn write_args(method: &MethodSpec, s: &mut String) -> Vec<String> {
    s.push_str("    const a = H.slots; let i = 0;\n    ");
    let mut slices = Vec::new();
    for arg in &method.args {
        match arg {
            ArgSpec::Value(n, k) => write_js(k, &format!("${n}"), s),
            ArgSpec::Slice(sl) => slices.push(format!("H.bytes(${}, {}, NAME, {:?}, {:?})", sl.name, sl.view, method.name, sl.name)),
            ArgSpec::Points(sl, lanes) => slices.push(format!("H.points(${}, {lanes}, NAME, {:?}, {:?})", sl.name, method.name, sl.name)),
        }
    }
    while slices.len() < MAX_SLICES {
        slices.push("H.EMPTY".into());
    }
    // A list or string past the slots was dropped by the typed array; refuse the call.
    if method.args.iter().any(|a| matches!(a, ArgSpec::Value(_, k) if varies(k))) {
        let _ = write!(s, "\n    if (i > H.slots.length) throw new RangeError(`${{NAME}}.{}: the arguments need ${{i}} slots, past the call's ${{H.slots.length}}; split the call`);", method.name);
    }
    slices
}

/// The methods a recorder of `spec` has: those that draw (return nothing or a builder) and the
/// builders' settings, each name once.
fn recorded(spec: &ParamSpec) -> impl Iterator<Item = &MethodSpec> {
    let mut seen = std::collections::HashSet::new();
    spec.methods.iter().filter(|m| m.ret == Kind::Unit && !m.args.iter().any(|a| matches!(a, ArgSpec::Slice(_)))).filter(move |m| seen.insert(m.name.as_str()))
}

/// A recorder of `spec`'s calls (a retained `GizmoAsset`): JS for `new Function("H", src)`, which
/// returns a class with the param's drawing methods and settings. Each call writes its arguments
/// as the param's glue does, then `H.record` keeps its name, its slots as `u32` words and its
/// point lists' floats as words, in `calls`; Rust replays them through the param's own entries.
/// Builder settings record as calls after the builder's; a setting on a builder another call
/// replaced throws, as on the param. `toJSON` gives `{ calls }`. Methods with a byte slice or a
/// result are left out.
pub fn recorder_glue(spec: &ParamSpec, name: &str) -> String {
    let mut s = format!("const NAME = {name:?};\nreturn class {name} {{\n  constructor() {{ this.calls = []; }}\n  toJSON() {{ return {{ calls: this.calls }}; }}\n");
    for method in recorded(spec) {
        let names: Vec<String> = method
            .args
            .iter()
            .map(|a| match a {
                ArgSpec::Value(n, _) => format!("${n}"),
                ArgSpec::Slice(sl) | ArgSpec::Points(sl, _) => format!("${}", sl.name),
            })
            .collect();
        let _ = writeln!(s, "  {}({}) {{", method.name, names.join(", "));
        if method.of.is_some() {
            let _ = writeln!(s, "    H.live(this, {:?});", method.name);
        }
        let mut slices = write_args(method, &mut s);
        slices.truncate(method.args.iter().filter(|a| a.slice().is_some()).count());
        let rest: String = slices.iter().map(|e| format!(", {e}")).collect();
        let _ = writeln!(s, "\n    H.record(this.calls, {:?}, i{rest});", method.name);
        match method.chain {
            Some(builder) => {
                let _ = writeln!(s, "    return H.open(this, {builder:?});\n  }}");
            }
            None if method.of.is_some() => s.push_str("    return this;\n  }\n"),
            None => s.push_str("  }\n"),
        }
    }
    s.push_str("};\n");
    s
}

/// The TS interface of [`recorder_glue`]'s class under `head`: its drawing methods, typed as the
/// param's; the builders are the param's own declarations.
pub fn recorder_declaration(spec: &ParamSpec, head: &str) -> String {
    let mut s = format!("export interface {head} {{\n");
    for m in recorded(spec).filter(|m| m.of.is_none()) {
        let ret = m.chain.unwrap_or("void");
        let _ = writeln!(s, "  {}({}): {ret};", m.name, ts_args(m));
    }
    s.push_str("}\n");
    s
}

/// The records, enums and builders a param's methods use, by name.
fn named_types(spec: &ParamSpec, out: &mut BTreeMap<&'static str, String>) {
    for m in &spec.methods {
        m.ret.declare(out);
        for a in &m.args {
            if let ArgSpec::Value(_, k) = a {
                k.declare(out);
            }
        }
    }
    for b in spec.methods.iter().filter_map(|m| m.chain) {
        let mut s = format!("/** Settings of the shape just drawn; call them right after the call that returned it. */\nexport interface {b} {{\n");
        for m in spec.methods.iter().filter(|m| m.of == Some(b)) {
            let _ = writeln!(s, "  {}({}): {b};", m.name, ts_args(m));
        }
        s.push_str("}\n");
        out.insert(b, s);
    }
}

fn ts_args(m: &MethodSpec) -> String {
    let args: Vec<String> = m
        .args
        .iter()
        .map(|a| match a {
            ArgSpec::Value(n, k) => format!("{n}: {}", k.ts_in()),
            ArgSpec::Slice(sl) => format!("{}: {}", sl.name, sl.view),
            ArgSpec::Points(sl, lanes) => {
                let point = match lanes {
                    1 => "number".to_string(),
                    2 => Kind::Vec2.ts_in(),
                    _ => Kind::Vec3.ts_in(),
                };
                format!("{}: readonly ({point})[] | Float32Array", sl.name)
            }
        })
        .collect();
    args.join(", ")
}

/// The TS declaration of a param.
fn declaration(spec: &ParamSpec) -> String {
    interface(spec, spec.ts)
}

/// The param's interface, its head written `head` (`Gizmos<G extends ...>`); builder settings are
/// on the builders.
fn interface(spec: &ParamSpec, head: &str) -> String {
    let mut s = String::new();
    let _ = writeln!(s, "/** `{}`, a system param exported from Rust; take it as a system parameter. */", spec.rust);
    let _ = writeln!(s, "export interface {head} {{");
    for m in spec.methods.iter().filter(|m| m.of.is_none()) {
        let ret = m.chain.map_or_else(|| m.ret.ts_out(), str::to_string);
        let _ = writeln!(s, "  {}({}): {ret};", m.name, ts_args(m));
    }
    s.push_str("}\n");
    s
}

/// A param's whole declaration under another head, for a module that declares it itself (a
/// generic `Gizmos<G>` over one export per group): its records, enums and builders, then the
/// interface.
pub fn declaration_as(spec: &ParamSpec, head: &str) -> String {
    let mut types = BTreeMap::new();
    named_types(spec, &mut types);
    types.into_values().collect::<String>() + &interface(spec, head)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slots_round_trip_with_options() {
        let mut buf = vec![0u8; 8 * 16];
        let mut w = SlotWriter::new(&mut buf);
        Some(Vec3::new(1.0, 2.0, 3.0)).write(&mut w);
        None::<Vec3>.write(&mut w);
        Entity::from_bits(42).write(&mut w);
        assert_eq!(w.finish().unwrap(), 4 + 4 + 1);
        let mut r = SlotReader::new(&buf);
        assert_eq!(Option::<Vec3>::read(&mut r).unwrap(), Some(Vec3::new(1.0, 2.0, 3.0)));
        assert_eq!(Option::<Vec3>::read(&mut r).unwrap(), None);
        assert_eq!(Entity::read(&mut r).unwrap(), Entity::from_bits(42));
    }

    #[test]
    fn strings_round_trip() {
        let long = "x".repeat(300);
        let cases = ["", "ascii", "naïve 日本 🦀", long.as_str()];
        let mut buf = vec![0u8; 8 * 128];
        let mut w = SlotWriter::new(&mut buf);
        for c in cases {
            c.to_string().write(&mut w);
        }
        Some("in".to_string()).write(&mut w);
        None::<String>.write(&mut w);
        7u32.write(&mut w);
        // Each string: a length slot, then its bytes rounded up to whole slots.
        let used: usize = cases.iter().map(|c| 1 + c.len().div_ceil(8)).sum::<usize>() + (1 + 1 + 1) + (1 + 1) + 1;
        assert_eq!(w.finish().unwrap(), used);
        let mut r = SlotReader::new(&buf);
        for c in cases {
            assert_eq!(String::read(&mut r).unwrap(), c);
        }
        assert_eq!(Option::<String>::read(&mut r).unwrap().as_deref(), Some("in"));
        assert_eq!(Option::<String>::read(&mut r).unwrap(), None);
        assert_eq!(u32::read(&mut r).unwrap(), 7, "what follows a string is read where it was written");
    }

    #[test]
    fn a_string_longer_than_the_slots_is_refused() {
        let mut buf = vec![0u8; 8 * 4];
        let mut w = SlotWriter::new(&mut buf);
        "a string of more than twenty-four bytes".to_string().write(&mut w);
        assert!(w.finish().is_err(), "a result that does not fit is an error");
        let mut args = vec![0u8; 8 * 2];
        args[..8].copy_from_slice(&100f64.to_le_bytes());
        assert!(String::read(&mut SlotReader::new(&args)).is_err(), "a length past the arguments is an error");
        let mut bad = vec![0u8; 8 * 2];
        bad[..8].copy_from_slice(&2f64.to_le_bytes());
        bad[8..10].copy_from_slice(&[0xff, 0xfe]);
        assert!(String::read(&mut SlotReader::new(&bad)).is_err(), "bytes that are not UTF-8 are an error");
    }

    #[test]
    fn a_full_writer_reports_instead_of_writing_past_the_end() {
        let mut buf = vec![0u8; 8];
        let mut w = SlotWriter::new(&mut buf);
        Vec3::ONE.write(&mut w);
        assert!(w.finish().is_err());
    }

    /// An argument may share a name with the glue's own locals (`a`, `i`, `n`, `r`): the JS
    /// argument is `$`-prefixed, so `at(i)` is not `let i` declared twice.
    #[test]
    fn glue_arguments_never_meet_its_locals() {
        let method = |name: &str, args| MethodSpec { name: name.into(), args, ret: Kind::Unit, call: |_, _, _, _, _, _| Ok(()), chain: None, of: None };
        let spec = ParamSpec {
            ts: "Probe",
            module: "bevy-ts/probe",
            rust: "Probe",
            stands_for: None,
            init: init::<()>,
            module_declares: false,
            methods: vec![method("at", vec![ArgSpec::Value("i", Kind::Num("u32")), ArgSpec::Value("a", Kind::Vec2)])],
        };
        let js = glue(&spec, 0);
        assert!(js.contains("  at($i, $a) {"), "{js}");
        assert!(js.contains("a[i++] = +$i;") && js.contains("const v = $a;"), "{js}");
    }

    /// Slots as the glue writes them, for the codecs below.
    fn slots(write: impl FnOnce(&mut SlotWriter)) -> Vec<u8> {
        let mut buf = vec![0u8; 8 * SLOTS];
        let mut w = SlotWriter::new(&mut buf);
        write(&mut w);
        w.finish().unwrap();
        buf
    }

    fn color_of(tag: f64, rest: impl FnOnce(&mut SlotWriter)) -> Result<Color, String> {
        let buf = slots(|w| {
            w.push(tag);
            rest(w);
        });
        Color::read(&mut SlotReader::new(&buf))
    }

    #[test]
    fn colors_cross_in_every_form() {
        // A hex string goes to Rust as text: bevy_ts::color parses it.
        let hex = color_of(COLOR_HEX, |w| "#ff0000".to_string().write(w)).unwrap();
        assert_eq!(hex, crate::color::parse(&serde_json::json!("#ff0000")).unwrap());
        // An array is Srgba, its alpha 1 when left out (the glue writes it).
        assert_eq!(color_of(0.0, |w| [1.0, 0.5, 0.0, 1.0].iter().for_each(|v| w.push(*v))).unwrap(), Color::srgba(1.0, 0.5, 0.0, 1.0));
        // Each variant by its tag, its channels in reflected field order: what bevy_ts::color
        // gives for the reflected shape.
        for (tag, (variant, channels)) in COLOR_VARIANTS.iter().enumerate() {
            let values = [0.25f32, 0.5, 0.75, 0.5];
            let json = serde_json::json!({ *variant: channels.iter().zip(values).map(|(c, v)| (c.to_string(), serde_json::json!(v))).collect::<serde_json::Map<_, _>>() });
            let want = crate::color::parse(&json).unwrap();
            let got = color_of(tag as f64, |w| values.iter().for_each(|v| w.push(*v as f64))).unwrap();
            assert_eq!(got, want, "{variant}");
            let mut back = vec![0u8; 8 * 5];
            let mut w = SlotWriter::new(&mut back);
            got.write(&mut w);
            assert_eq!(Color::read(&mut SlotReader::new(&back)).unwrap(), got, "{variant} round-trips");
        }
        // Any other form is JSON text, parsed in Rust; a bad one names the forms taken.
        let obj = color_of(COLOR_JSON, |w| r#"{"LinearRgba":{"red":1,"green":0,"blue":0}}"#.to_string().write(w)).unwrap();
        assert_eq!(obj, Color::linear_rgba(1.0, 0.0, 0.0, 1.0));
        let bad = color_of(COLOR_HEX, |w| "#ff000".to_string().write(w)).unwrap_err();
        assert!(bad.contains("#ff000") && bad.contains(crate::color::FORMS), "{bad}");
        let bad = color_of(COLOR_JSON, |w| "null".to_string().write(w)).unwrap_err();
        assert!(bad.contains(crate::color::FORMS), "{bad}");
        assert!(color_of(20.0, |w| (0..4).for_each(|_| w.push(0.0))).is_err());
    }

    #[test]
    fn isometries_take_bevys_into_forms() {
        let iso = Isometry3d::new(Vec3::new(1.0, 2.0, 3.0), Quat::from_rotation_y(0.5));
        let buf = slots(|w| iso.write(w));
        assert_eq!(Isometry3d::read(&mut SlotReader::new(&buf)).unwrap(), iso);
        // What the glue writes for `[1, 2, 3]`: Bevy's `From<Vec3>`.
        let buf = slots(|w| [1.0, 2.0, 3.0, 0.0, 0.0, 0.0, 1.0].iter().for_each(|v| w.push(*v)));
        assert_eq!(Isometry3d::read(&mut SlotReader::new(&buf)).unwrap(), Isometry3d::from_translation(Vec3::new(1.0, 2.0, 3.0)));
        let iso2 = Isometry2d::new(Vec2::new(1.0, 2.0), Rot2::radians(0.3));
        let buf = slots(|w| iso2.write(w));
        assert_eq!(Isometry2d::read(&mut SlotReader::new(&buf)).unwrap(), iso2);
        // A rotation that is not one is refused.
        let buf = slots(|w| [0.0, 0.0, 2.0, 0.0].iter().for_each(|v| w.push(*v)));
        assert!(Isometry2d::read(&mut SlotReader::new(&buf)).unwrap_err().contains("is not a rotation"));
    }

    #[test]
    fn transforms_take_transform_and_global_transform() {
        let t = Transform { translation: Vec3::new(1.0, 2.0, 3.0), rotation: Quat::from_rotation_x(0.4), scale: Vec3::splat(2.0) };
        let buf = slots(|w| HostTransform::Transform(t).write(w));
        assert_eq!(HostTransform::read(&mut SlotReader::new(&buf)).unwrap(), HostTransform::Transform(t));
        let g = GlobalTransform::from(t);
        let buf = slots(|w| HostTransform::Global(g).write(w));
        // A GlobalTransform is kept as given for Bevy's `impl TransformPoint`, and decomposed for a
        // `Transform` argument.
        assert_eq!(HostTransform::read(&mut SlotReader::new(&buf)).unwrap(), HostTransform::Global(g));
        let back = Transform::read(&mut SlotReader::new(&buf)).unwrap();
        assert!(back.translation.abs_diff_eq(t.translation, 1e-5) && back.rotation.abs_diff_eq(t.rotation, 1e-5) && back.scale.abs_diff_eq(t.scale, 1e-5), "{back:?}");
        assert_eq!(Kind::TransformPoint.slots(), 13);
    }

    #[test]
    fn uvecs_refuse_negative_lanes() {
        let read = |v: [f64; 2]| UVec2::read(&mut SlotReader::new(&slots(|w| v.iter().for_each(|x| w.push(*x)))));
        assert_eq!(read([3.0, 4.0]).unwrap(), UVec2::new(3, 4));
        assert!(read([-1.0, 4.0]).unwrap_err().contains("x -1 is not a whole number"));
        assert!(read([1.0, 0.5]).unwrap_err().contains("y 0.5"));
        assert!(read([1.0, f64::NAN]).is_err());
        let buf = slots(|w| UVec3::new(1, 2, 3).write(w));
        assert_eq!(UVec3::read(&mut SlotReader::new(&buf)).unwrap(), UVec3::new(1, 2, 3));
    }

    #[test]
    fn lists_cross_in_slots_and_refuse_past_the_room() {
        let list = HostList(vec![(Vec3::X, Color::srgb(1.0, 0.0, 0.0)), (Vec3::Y, Color::srgb(0.0, 1.0, 0.0))]);
        let buf = slots(|w| list.write(w));
        assert_eq!(HostList::<(Vec3, Color)>::read(&mut SlotReader::new(&buf)).unwrap(), list);
        assert_eq!(HostList::<(Vec3, Color)>::kind(), Kind::List(vec![Kind::Vec3, Kind::Color]));
        // A list past the slots is refused in JS, naming the method and the room.
        let spec = probe(vec![ArgSpec::Value("points", HostList::<(Vec3, Color)>::kind())], None, None);
        let js = glue(&spec, 0);
        assert!(js.contains("for (const e of l) {") && js.contains("i = H.color(e[1], i);"), "{js}");
        assert!(js.contains("if (i > H.slots.length) throw new RangeError(`${NAME}.m: the arguments need ${i} slots, past the call's ${H.slots.length}; split the call`);"), "{js}");
        // Fixed-size arguments need no such check.
        assert!(!glue(&probe(vec![ArgSpec::Value("v", Kind::Vec3)], None, None), 0).contains("RangeError"));
        // A result that does not fit is an error, as before.
        let mut small = vec![0u8; 8 * 3];
        let mut w = SlotWriter::new(&mut small);
        list.write(&mut w);
        assert!(w.finish().is_err());
    }

    #[test]
    fn json_round_trips() {
        let v = HostJson(serde_json::json!({ "line": { "width": 4, "style": { "Dashed": { "gap_scale": 2 } } }, "enabled": true }));
        let buf = slots(|w| v.write(w));
        assert_eq!(HostJson::read(&mut SlotReader::new(&buf)).unwrap(), v);
        let buf = slots(|w| "{ not json".to_string().write(w));
        assert!(HostJson::read(&mut SlotReader::new(&buf)).unwrap_err().starts_with("not JSON"));
        let spec = MethodSpec { ret: Kind::Json("GizmoConfig"), ..probe(vec![ArgSpec::Value("c", Kind::Json("Partial<GizmoConfig>"))], None, None).methods.remove(0) };
        let js = glue(&ParamSpec { methods: vec![spec.clone()], ..probe(vec![], None, None) }, 0);
        assert!(js.contains("i = H.str(JSON.stringify($c), i);") && js.contains("return JSON.parse("), "{js}");
        assert_eq!(ts_args(&spec), "c: Partial<GizmoConfig>");
    }

    #[test]
    fn points_from_arrays_and_float32arrays() {
        let v = [0.0f32, 0.0, 0.0, 1.0, 0.0, 0.0];
        let bytes = unsafe { std::slice::from_raw_parts(v.as_ptr() as *const u8, 24) };
        let p = HostPoints::<Vec3>::new(bytes, "positions").unwrap();
        assert_eq!(p.iter().collect::<Vec<_>>(), vec![Vec3::ZERO, Vec3::X]);
        assert_eq!(HostPoints::<Vec2>::new(bytes, "positions").unwrap().len(), 3);
        assert!(HostPoints::<Vec3>::new(&bytes[..16], "positions").unwrap_err().contains("positions: 4 floats is not a whole number of 3-float points"));
        // The glue passes a Float32Array as is and packs an array: both through `H.points`.
        let spec = probe(vec![<HostPoints<Vec3> as HostArg>::spec("positions"), ArgSpec::Value("color", Kind::Color)], None, None);
        let js = glue(&spec, 0);
        assert!(js.contains(r#"H.points($positions, 3, NAME, "m", "positions")"#), "{js}");
        assert!(ts_args(&spec.methods[0]).starts_with(&format!("positions: readonly ({})[] | Float32Array, color: ColorValue", Kind::Vec3.ts_in())));
    }

    fn probe(args: Vec<ArgSpec>, chain: Option<&'static str>, of: Option<&'static str>) -> ParamSpec {
        ParamSpec {
            ts: "Probe",
            module: "bevy-ts/probe",
            rust: "Probe",
            stands_for: None,
            init: init::<()>,
            module_declares: false,
            methods: vec![MethodSpec { name: "m".into(), args, ret: Kind::Unit, call: |_, _, _, _, _, _| Ok(()), chain, of }],
        }
    }

    #[test]
    fn builder_glue_returns_a_builder() {
        let mut spec = probe(vec![ArgSpec::Value("radius", Kind::Num("f32"))], Some("CircleBuilder"), None);
        let setting = |name: &str, args, of| MethodSpec { name: name.into(), args, ret: Kind::Unit, call: |_, _, _, _, _, _| Ok(()), chain: None, of: Some(of) };
        spec.methods.push(setting("resolution", vec![ArgSpec::Value("resolution", Kind::Num("u32"))], "CircleBuilder"));
        spec.methods.push(setting("skew", vec![ArgSpec::Value("skew", Kind::Vec3)], "CircleBuilder"));
        // A second builder's setting of the same name shares the first one's JS method.
        spec.methods.push(setting("skew", vec![ArgSpec::Value("skew", Kind::Vec2)], "OtherBuilder"));
        let js = glue(&spec, 10);
        // The entry returns a builder of its own; each setting checks it is the latest, then
        // returns it.
        assert!(js.contains("return H.open(this, \"CircleBuilder\");"), "{js}");
        assert_eq!(js.matches("return this;").count(), 2, "{js}");
        assert_eq!(js.matches("H.live(this, ").count(), 2, "{js}");
        assert_eq!(js.matches("  skew($skew) {").count(), 1, "{js}");
        assert!(js.contains("H.call(12, i,"), "the shared method calls the first one's id: {js}");
        assert!(!js.contains("H.call(13,"), "{js}");
        // TS: the method returns the builder; settings are on the builder, not the param.
        let d = declaration(&spec);
        assert!(d.contains("  m(radius: number): CircleBuilder;") && !d.contains("resolution"), "{d}");
        let mut types = BTreeMap::new();
        named_types(&spec, &mut types);
        assert!(types["CircleBuilder"].contains("  resolution(resolution: number): CircleBuilder;"), "{types:?}");
        assert!(types["CircleBuilder"].contains("  skew(skew: { readonly x: number; readonly y: number; readonly z: number }"), "{types:?}");
        // A module that declares the param itself takes the whole declaration under its own head.
        let all = declaration_as(&spec, "Probe<G>");
        assert!(all.contains("export interface CircleBuilder {") && all.contains("export interface Probe<G> {"), "{all}");
    }

    #[test]
    fn a_recorder_records_drawing_calls_and_settings() {
        let mut spec = probe(vec![ArgSpec::Value("radius", Kind::Num("f32")), <HostPoints<Vec3> as HostArg>::spec("positions")], Some("CircleBuilder"), None);
        let method = |name: &str, ret, of| MethodSpec { name: name.into(), args: vec![ArgSpec::Value("n", Kind::Num("u32"))], ret, call: |_, _, _, _, _, _| Ok(()), chain: None, of };
        spec.methods.push(method("resolution", Kind::Unit, Some("CircleBuilder")));
        spec.methods.push(method("config", Kind::Json("GizmoConfig"), None));
        let js = recorder_glue(&spec, "Rec");
        // Arguments are written as the param's glue writes them, then recorded with the one
        // point list, not the padding slices.
        assert!(js.contains("return class Rec {") && js.contains("toJSON() { return { calls: this.calls }; }"), "{js}");
        assert!(js.contains("H.record(this.calls, \"m\", i, H.points($positions, 3, NAME, \"m\", \"positions\"));"), "{js}");
        assert!(js.contains("return H.open(this, \"CircleBuilder\");"), "{js}");
        assert!(js.contains("H.live(this, \"resolution\");") && js.contains("H.record(this.calls, \"resolution\", i);"), "{js}");
        assert!(!js.contains("config(") && !js.contains("H.call("), "a reader is left out, and nothing calls the host: {js}");
        let d = recorder_declaration(&spec, "Rec");
        assert!(d.contains("  m(radius: number, positions: ") && d.contains("): CircleBuilder;") && !d.contains("resolution") && !d.contains("config"), "{d}");
    }

    #[test]
    fn spellings_normalise() {
        assert_eq!(normalise_spelling("ResMut< bevy::time::Time<bevy::time::Virtual> >"), "ResMut<Time<Virtual>>");
    }

    #[test]
    fn an_enum_crosses_as_its_variant_names() {
        let settings = Kind::Record("Settings", vec![("visibility".into(), Kind::Enum("Seen", vec!["InView", "Any"])), ("early".into(), Kind::Bool)]);
        let mut decls = BTreeMap::new();
        settings.declare(&mut decls);
        assert_eq!(decls["Seen"], "export type Seen = \"InView\" | \"Any\";\n");
        assert!(decls["Settings"].contains("readonly visibility: Seen;"), "{decls:?}");
        // A record that leaves the enum out gets its first variant; a name not in it throws.
        let mut js = String::new();
        write_js(&settings, "s", &mut js);
        assert!(js.contains(r#"["InView", "Any"].indexOf((r["visibility"] ?? "InView"))"#), "{js}");
        assert!(js.contains("is not a Seen: give one of InView, Any"), "{js}");
        assert_eq!(read_js(&Kind::Enum("Seen", vec!["InView", "Any"])), r#"["InView", "Any"][a[i++]]"#);
    }

    #[test]
    fn slices_check_length_and_alignment() {
        let v = [1.0f32, 2.0, 3.0];
        let bytes = unsafe { std::slice::from_raw_parts(v.as_ptr() as *const u8, 12) };
        assert_eq!(slice_ref::<f32>(bytes, "xs").unwrap(), &[1.0, 2.0, 3.0]);
        assert!(slice_ref::<f32>(&bytes[1..], "xs").is_err());
        // An empty array, at whatever address, is an empty slice.
        assert_eq!(slice_ref::<f64>(&bytes[1..1], "none").unwrap(), &[] as &[f64]);
        assert_eq!(slice_mut::<f64>(&mut [], "none").unwrap(), &mut [] as &mut [f64]);
    }
}
