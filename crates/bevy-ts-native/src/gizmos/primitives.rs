//! Primitives and curves as reflected values (design 7, 8): TS names the Bevy type by a one-key
//! object, `{ Cuboid: { half_size: [1, 1, 1] } }`, and the value is written over the type's
//! default by `bevy_ts::reflect_value`, then drawn by Bevy's `primitive_3d` or `curve_3d`.

use super::draw::{Settings, Settle};
use bevy::curve::{ConstantCurve, EasingCurve, SampleAutoCurve, UnevenSampleAutoCurve};
use bevy::gizmos::config::GizmoConfigGroup;
use bevy::gizmos::gizmos::GizmoBuffer;
use bevy::gizmos::primitives::dim2::GizmoPrimitive2d;
use bevy::gizmos::primitives::dim3::GizmoPrimitive3d;
use bevy::math::{Dir2, Dir3};
use bevy::prelude::*;
use bevy::reflect::GetTypeRegistration;
use bevy::shape::{
    Annulus, Arc2d, Capsule2d, Capsule3d, Circle, CircularSector, CircularSegment, Cone, ConicalFrustum, Cuboid, Cylinder, Ellipse, Line2d, Line3d, Plane2d,
    Plane3d, Polygon, Polyline2d, Polyline3d, Rectangle, RegularPolygon, Rhombus, Segment2d, Segment3d, Sphere, Tetrahedron, Torus, Triangle2d, Triangle3d,
};
use bevy_ts::host::{HostJson, HostValue, Kind, SlotReader, SlotWriter};
use serde_json::Value;
use std::any::TypeId;

macro_rules! json_args {
    ($($name:ident: $doc:literal),*) => {
        $(#[doc = $doc]
        pub struct $name(pub Value);

        impl HostValue for $name {
            fn kind() -> Kind {
                Kind::Json(stringify!($name))
            }
            fn read(r: &mut SlotReader) -> Result<Self, String> {
                HostJson::read(r).map(|j| $name(j.0))
            }
            fn write(&self, w: &mut SlotWriter) {
                HostJson(self.0.clone()).write(w);
            }
        })*
    };
}

json_args! {
    Primitive2d: "A 2D primitive as TS gives it, `{ Circle: { radius: 1 } }`; TS types it from the table below.",
    Primitive3d: "A 3D primitive as TS gives it, `{ Cuboid: { half_size: [1, 1, 1] } }`.",
    Curve2d: "A curve of `Vec2` as TS gives it, `{ EasingCurve: { start, end, ease_fn } }`.",
    Curve3d: "A curve of `Vec3` as TS gives it."
}

macro_rules! primitives_2d {
    ($cb:ident) => {
        $cb!(Dir2, Arc2d, Circle, CircularSector, CircularSegment, Ellipse, Annulus, Rhombus, Capsule2d, Line2d, Plane2d, Segment2d, Polyline2d, Triangle2d, Rectangle, Polygon, RegularPolygon)
    };
}

macro_rules! primitives_3d {
    ($cb:ident) => {
        $cb!(Dir3, Sphere, Plane3d, Line3d, Segment3d, Polyline3d, Triangle3d, Cuboid, Cylinder, Capsule3d, Cone, ConicalFrustum, Torus, Tetrahedron)
    };
}

macro_rules! names {
    ($($t:ident),*) => { &[$(stringify!($t)),*] };
}

/// Every type with a `GizmoPrimitive2d` impl in Bevy 0.20, by the key TS names it with.
pub const PRIMITIVES_2D: &[&str] = primitives_2d!(names);
/// Every type with a `GizmoPrimitive3d` impl in Bevy 0.20.
pub const PRIMITIVES_3D: &[&str] = primitives_3d!(names);
/// The curves TS may draw, each of `Vec2` (`curve2d`) or `Vec3` (`curve3d`).
pub const CURVES: &[&str] = &["EasingCurve", "SampleAutoCurve", "UnevenSampleAutoCurve", "ConstantCurve"];

/// The key and value of a one-key object naming one of `keys`.
fn one_key<'a>(v: &'a Value, what: &str, keys: &[&str]) -> Result<(&'a str, &'a Value), String> {
    match v {
        Value::Object(o) if o.len() == 1 => {
            let (k, inner) = o.iter().next().unwrap();
            if keys.contains(&k.as_str()) {
                Ok((k.as_str(), inner))
            } else {
                Err(format!("{what}: {k} is not one of {}", keys.join(", ")))
            }
        }
        _ => Err(format!("{what} takes a one-key object naming one of {}, as {{ {}: {{ ... }} }}; not {v}", keys.join(", "), keys[0])),
    }
}

/// `json` written over `T`'s default (or given whole, for a type with none) by reflection.
fn decode<T: FromReflect + TypePath>(registry: &AppTypeRegistry, json: &Value) -> Result<T, String> {
    let r = registry.read();
    let value = bevy_ts::reflect_value::value_of(&r, TypeId::of::<T>(), json).map_err(|e| format!("{}: {e}", T::short_type_path()))?;
    T::from_reflect(value.as_ref()).ok_or_else(|| format!("{} is not a {}", json, T::short_type_path()))
}

pub fn draw_2d<C: GizmoConfigGroup>(g: &mut GizmoBuffer<C, ()>, registry: &AppTypeRegistry, shape: &Value, isometry: Isometry2d, color: Color, settings: &Settings) -> Result<(), String> {
    let (key, json) = one_key(shape, "primitive2d", PRIMITIVES_2D)?;
    macro_rules! each {
        ($($t:ident),*) => {
            match key {
                $(stringify!($t) => g.primitive_2d(&decode::<$t>(registry, json)?, isometry, color).settle_primitive(key, settings),)*
                _ => unreachable!("one_key checked {key}"),
            }
        };
    }
    primitives_2d!(each)
}

pub fn draw_3d<C: GizmoConfigGroup>(g: &mut GizmoBuffer<C, ()>, registry: &AppTypeRegistry, shape: &Value, isometry: Isometry3d, color: Color, settings: &Settings) -> Result<(), String> {
    let (key, json) = one_key(shape, "primitive3d", PRIMITIVES_3D)?;
    macro_rules! each {
        ($($t:ident),*) => {
            match key {
                $(stringify!($t) => g.primitive_3d(&decode::<$t>(registry, json)?, isometry, color).settle_primitive(key, settings),)*
                _ => unreachable!("one_key checked {key}"),
            }
        };
    }
    primitives_3d!(each)
}

macro_rules! curves {
    ($name:ident, $gradient:ident, $what:literal, $grad_what:literal, $v:ty, $call:ident, $grad_call:ident) => {
        pub fn $name<C: GizmoConfigGroup>(g: &mut GizmoBuffer<C, ()>, registry: &AppTypeRegistry, curve: &Value, times: impl Iterator<Item = f32>, color: Color) -> Result<(), String> {
            match one_key(curve, $what, CURVES)? {
                ("EasingCurve", j) => g.$call(decode::<EasingCurve<$v>>(registry, j)?, times, color),
                ("SampleAutoCurve", j) => g.$call(decode::<SampleAutoCurve<$v>>(registry, j)?, times, color),
                ("UnevenSampleAutoCurve", j) => g.$call(decode::<UnevenSampleAutoCurve<$v>>(registry, j)?, times, color),
                (_, j) => g.$call(decode::<ConstantCurve<$v>>(registry, j)?, times, color),
            }
            Ok(())
        }

        pub fn $gradient<C: GizmoConfigGroup>(g: &mut GizmoBuffer<C, ()>, registry: &AppTypeRegistry, curve: &Value, times: Vec<(f32, Color)>) -> Result<(), String> {
            match one_key(curve, $grad_what, CURVES)? {
                ("EasingCurve", j) => g.$grad_call(decode::<EasingCurve<$v>>(registry, j)?, times),
                ("SampleAutoCurve", j) => g.$grad_call(decode::<SampleAutoCurve<$v>>(registry, j)?, times),
                ("UnevenSampleAutoCurve", j) => g.$grad_call(decode::<UnevenSampleAutoCurve<$v>>(registry, j)?, times),
                (_, j) => g.$grad_call(decode::<ConstantCurve<$v>>(registry, j)?, times),
            }
            Ok(())
        }
    };
}

curves!(curve_2d, curve_gradient_2d, "curve2d", "curveGradient2d", Vec2, curve_2d, curve_gradient_2d);
curves!(curve_3d, curve_gradient_3d, "curve3d", "curveGradient3d", Vec3, curve_3d, curve_gradient_3d);

/// The type paths of each primitive (`2` or `3`) or curve (`Vec2`/`Vec3`) key, for the TS unions.
pub fn type_paths() -> [Vec<(&'static str, &'static str)>; 4] {
    macro_rules! paths {
        ($($t:ident),*) => { vec![$((stringify!($t), <$t as TypePath>::type_path())),*] };
    }
    let curves = |v2: bool| {
        if v2 {
            vec![
                ("EasingCurve", EasingCurve::<Vec2>::type_path()),
                ("SampleAutoCurve", SampleAutoCurve::<Vec2>::type_path()),
                ("UnevenSampleAutoCurve", UnevenSampleAutoCurve::<Vec2>::type_path()),
                ("ConstantCurve", ConstantCurve::<Vec2>::type_path()),
            ]
        } else {
            vec![
                ("EasingCurve", EasingCurve::<Vec3>::type_path()),
                ("SampleAutoCurve", SampleAutoCurve::<Vec3>::type_path()),
                ("UnevenSampleAutoCurve", UnevenSampleAutoCurve::<Vec3>::type_path()),
                ("ConstantCurve", ConstantCurve::<Vec3>::type_path()),
            ]
        }
    };
    [primitives_2d!(paths), primitives_3d!(paths), curves(true), curves(false)]
}

/// Registers every primitive and curve type TS may name, so reflection can write them.
pub fn register_types(app: &mut App) {
    fn reg<T: GetTypeRegistration>(app: &mut App) {
        app.register_type::<T>();
    }
    macro_rules! each {
        ($($t:ident),*) => { $(reg::<$t>(app);)* };
    }
    primitives_2d!(each);
    primitives_3d!(each);
    // Bevy reflects some primitives without their `Default` (`Segment2d`), so a value TS leaves a
    // field out of would not start from it. Directions, lines and `Polygon` have none.
    fn with_default<T: GetTypeRegistration + Default + Reflect + TypePath>(app: &mut App) {
        app.register_type_data::<T, ReflectDefault>();
    }
    macro_rules! defaults {
        ($($t:ident),*) => { $(with_default::<$t>(app);)* };
    }
    defaults!(Arc2d, Circle, CircularSector, CircularSegment, Ellipse, Annulus, Rhombus, Capsule2d, Plane2d, Segment2d, Polyline2d, Triangle2d, Rectangle, RegularPolygon);
    defaults!(Sphere, Plane3d, Segment3d, Polyline3d, Triangle3d, Cuboid, Cylinder, Capsule3d, Cone, ConicalFrustum, Torus, Tetrahedron);
    reg::<EasingCurve<Vec2>>(app);
    reg::<EasingCurve<Vec3>>(app);
    reg::<SampleAutoCurve<Vec2>>(app);
    reg::<SampleAutoCurve<Vec3>>(app);
    reg::<UnevenSampleAutoCurve<Vec2>>(app);
    reg::<UnevenSampleAutoCurve<Vec3>>(app);
    reg::<ConstantCurve<Vec2>>(app);
    reg::<ConstantCurve<Vec3>>(app);
}
