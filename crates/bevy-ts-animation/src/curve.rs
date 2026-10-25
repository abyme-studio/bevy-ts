//! Script-built curves (change `animation-control`, design 17 and 18): the JSON a TS
//! `AnimatableCurve` holds, turned into Bevy's own curves and a property any reflected component
//! field can be.
//!
//! A curve is `{ Constructor: args }` or `{ Adaptor: args }`, Bevy's names:
//!
//! - `{ "EasingCurve": { "start", "end", "ease" } }`, `ease` an `EaseFunction` as Bevy names its
//!   variants (`"CubicOut"`, `{ "Steps": [4, "End"] }`, `{ "Elastic": 2 }`);
//! - `{ "ConstantCurve": { "domain", "value" } }`, `{ "SampleAutoCurve": { "domain", "samples" } }`;
//! - `{ "UnevenSampleAutoCurve" | "AnimatableKeyframeCurve" | "SteppedKeyframeCurve": [[t, v], ...] }`;
//! - `{ "CubicKeyframeCurve" | "CubicRotationCurve": { "times", "values" } }`;
//! - `{ "ToCurve" | "ToCurveCyclic": { "CubicBezier" | "CubicHermite" | ...: args } }`, a spline;
//! - `{ "ReparametrizeLinear": [curve, domain] }`, `{ "Reverse" | "Forever" | "PingPong": curve }`,
//!   `{ "Repeat": [curve, n] }`, `{ "Chain" | "ChainContinue": [a, b] }`.
//!
//! A domain is `[start, end]`, `null` for an unbounded end. Values are the field's type in the
//! reflected JSON (`[x, y, z]` for a `Vec3`). [`ScriptCurve`] holds the built Bevy curve and the
//! JSON; Bevy samples, this only dispatches.

use bevy::animation::animatable::Animatable;
use bevy::animation::animation_curves::{AnimatableCurve, AnimatableKeyframeCurve, AnimatableProperty, EvaluatorId};
use bevy::animation::gltf_curves::{CubicKeyframeCurve, CubicRotationCurve, SteppedKeyframeCurve};
use bevy::animation::{AnimationClip, AnimationEntityMut, AnimationEvaluationError, AnimationTargetId};
use bevy::color::{Laba, LinearRgba, Oklaba, Srgba, Xyza};
use bevy::curve::cubic_splines::{CubicBSpline, CubicBezier, CubicCardinalSpline, CubicGenerator, CubicHermite, CubicNurbs, CyclicCubicGenerator, LinearSpline, RationalGenerator};
use bevy::curve::easing::{EaseFunction, EasingCurve, JumpAt};
use bevy::curve::{ConstantCurve, Curve, Interval, SampleAutoCurve, UnevenSampleAutoCurve};
use bevy::ecs::component::ComponentId;
use bevy::math::{DVec2, DVec3, DVec4, Quat, Rot2, Vec2, Vec3, Vec3A, Vec4};
use bevy::platform::hash::Hashed;
use bevy::prelude::*;
use bevy::reflect::{FromReflect, ReflectFromPtr, ReflectMut, TypePath, TypeRegistry, Typed};
use serde_json::Value;
use std::any::TypeId;
use std::fmt;
use std::sync::Arc;

pub use crate::hand::target;

type Dyn<A> = Arc<dyn Curve<A> + Send + Sync>;

/// A curve a script built: Bevy's own curve, behind an `Arc` so it clones, with the JSON it was
/// built from for `Debug`. Opaque to reflection, as Bevy's boxed clip curves are.
#[derive(Reflect)]
#[reflect(opaque, Clone, Debug)]
pub struct ScriptCurve<A: TypePath> {
    curve: Dyn<A>,
    json: Arc<Value>,
}

impl<A: TypePath> Clone for ScriptCurve<A> {
    fn clone(&self) -> Self {
        Self { curve: self.curve.clone(), json: self.json.clone() }
    }
}

impl<A: TypePath> fmt::Debug for ScriptCurve<A> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "ScriptCurve<{}>({})", A::short_type_path(), self.json)
    }
}

impl<A: TypePath> Curve<A> for ScriptCurve<A> {
    fn domain(&self) -> Interval {
        self.curve.domain()
    }

    fn sample_unchecked(&self, t: f32) -> A {
        self.curve.sample_unchecked(t)
    }
}

impl<A: TypePath> ScriptCurve<A> {
    fn new(curve: Dyn<A>, json: &Value) -> Self {
        Self { curve, json: Arc::new(json.clone()) }
    }
}

/// A spline's points, read as the field's type: Bevy's generators, by name.
pub enum Spline<A> {
    Bezier(Vec<[A; 4]>),
    Hermite(Vec<A>, Vec<A>),
    Cardinal(f32, Vec<A>),
    BSpline(Vec<A>),
    Nurbs(Vec<A>, Option<Vec<f32>>, Option<Vec<f32>>),
    Linear(Vec<A>),
}

/// An `Animatable` value a script curve can hold. Each constructor Bevy bounds more tightly
/// (`EasingCurve` needs `Ease`, the splines a vector space over `f32`) is a method here that
/// refuses, naming the type, unless the type meets the bound.
pub trait ScriptValue: Animatable + Clone + fmt::Debug + Reflect + TypePath + Typed + FromReflect + Send + Sync + 'static {
    fn easing(_: Self, _: Self, _: EaseFunction) -> Result<Dyn<Self>, String> {
        Err(refused::<Self>("EasingCurve", "Bevy cannot ease it"))
    }
    fn sample_auto(_: Interval, _: Vec<Self>) -> Result<Dyn<Self>, String> {
        Err(refused::<Self>("SampleAutoCurve", "it has no stable interpolation"))
    }
    fn uneven_sample_auto(_: Vec<(f32, Self)>) -> Result<Dyn<Self>, String> {
        Err(refused::<Self>("UnevenSampleAutoCurve", "it has no stable interpolation"))
    }
    fn cubic_keyframe(_: Vec<f32>, _: Vec<Self>) -> Result<Dyn<Self>, String> {
        Err(refused::<Self>("CubicKeyframeCurve", "it is not a vector space over f32"))
    }
    fn cubic_rotation(_: Vec<f32>, _: Vec<Self>) -> Result<Dyn<Self>, String> {
        Err(refused::<Self>("CubicRotationCurve", "it is not a Quat"))
    }
    fn spline(_: Spline<Self>, _: bool) -> Result<Dyn<Self>, String> {
        Err(refused::<Self>("a spline", "it is not a vector space over f32"))
    }
    fn chain_continue(_: ScriptCurve<Self>, _: ScriptCurve<Self>) -> Result<Dyn<Self>, String> {
        Err(refused::<Self>("chainContinue", "it is not a vector space"))
    }
}

fn refused<A: TypePath>(what: &str, why: &str) -> String {
    format!("{what} cannot hold a {}: {why}", A::short_type_path())
}

fn err(e: impl fmt::Display) -> String {
    e.to_string()
}

macro_rules! ease {
    () => {
        fn easing(start: Self, end: Self, f: EaseFunction) -> Result<Dyn<Self>, String> {
            Ok(Arc::new(EasingCurve::new(start, end, f)))
        }
    };
}

macro_rules! stable {
    () => {
        fn sample_auto(domain: Interval, samples: Vec<Self>) -> Result<Dyn<Self>, String> {
            Ok(Arc::new(SampleAutoCurve::new(domain, samples).map_err(err)?))
        }
        fn uneven_sample_auto(samples: Vec<(f32, Self)>) -> Result<Dyn<Self>, String> {
            Ok(Arc::new(UnevenSampleAutoCurve::new(samples).map_err(err)?))
        }
    };
}

macro_rules! vector {
    () => {
        fn chain_continue(a: ScriptCurve<Self>, b: ScriptCurve<Self>) -> Result<Dyn<Self>, String> {
            Ok(Arc::new(a.chain_continue(b).map_err(err)?))
        }
    };
}

macro_rules! cubic {
    () => {
        fn cubic_keyframe(times: Vec<f32>, values: Vec<Self>) -> Result<Dyn<Self>, String> {
            Ok(Arc::new(CubicKeyframeCurve::new(times, values).map_err(err)?))
        }
        fn spline(s: Spline<Self>, cyclic: bool) -> Result<Dyn<Self>, String> {
            Ok(match (s, cyclic) {
                (Spline::Bezier(p), false) => Arc::new(CubicBezier::new(p).to_curve().map_err(err)?),
                (Spline::Bezier(_), true) => return Err("CubicBezier has no toCurveCyclic".into()),
                (Spline::Hermite(p, t), false) => Arc::new(CubicHermite::new(p, t).to_curve().map_err(err)?),
                (Spline::Hermite(p, t), true) => Arc::new(CubicHermite::new(p, t).to_curve_cyclic().map_err(err)?),
                (Spline::Cardinal(k, p), false) => Arc::new(CubicCardinalSpline::new(k, p).to_curve().map_err(err)?),
                (Spline::Cardinal(k, p), true) => Arc::new(CubicCardinalSpline::new(k, p).to_curve_cyclic().map_err(err)?),
                (Spline::BSpline(p), false) => Arc::new(CubicBSpline::new(p).to_curve().map_err(err)?),
                (Spline::BSpline(p), true) => Arc::new(CubicBSpline::new(p).to_curve_cyclic().map_err(err)?),
                (Spline::Nurbs(p, w, k), false) => Arc::new(CubicNurbs::new(p, w, k).map_err(err)?.to_curve().map_err(err)?),
                (Spline::Nurbs(..), true) => return Err("CubicNurbs has no toCurveCyclic".into()),
                (Spline::Linear(p), false) => Arc::new(LinearSpline::new(p).to_curve().map_err(err)?),
                (Spline::Linear(p), true) => Arc::new(LinearSpline::new(p).to_curve_cyclic().map_err(err)?),
            })
        }
    };
}

macro_rules! rotation {
    () => {
        fn cubic_rotation(times: Vec<f32>, values: Vec<Self>) -> Result<Dyn<Self>, String> {
            Ok(Arc::new(CubicRotationCurve::new(times, values.into_iter().map(Vec4::from)).map_err(err)?))
        }
    };
}

macro_rules! values {
    ($($t:ty => [$($m:ident),*]),* $(,)?) => {
        $(impl ScriptValue for $t { $($m!();)* })*

        /// The curve JSON as a curve on the field type `ty`, which must be one of the
        /// `Animatable` types, put on `target` in `clip`.
        fn add_typed(registry: &TypeRegistry, clip: &mut AnimationClip, target: AnimationTargetId, field: &Field, json: &Value) -> Result<Interval, String> {
            $(if field.ty == TypeId::of::<$t>() {
                let curve = build::<$t>(registry, json)?;
                let domain = curve.domain();
                clip.add_curve_to_target(target, AnimatableCurve::new(ReflectedField::<$t>::new(field.clone()), curve));
                return Ok(domain);
            })*
            Err(format!("{}: its type is not one Bevy can animate", field.label))
        }

        /// Whether a field of type `ty` can be animated.
        pub fn animatable(ty: TypeId) -> bool {
            $(ty == TypeId::of::<$t>() ||)* false
        }
    };
}

values! {
    f32 => [ease, stable, vector, cubic],
    Vec2 => [ease, stable, vector, cubic],
    Vec3 => [ease, stable, vector, cubic],
    Vec3A => [ease, stable, vector, cubic],
    Vec4 => [ease, stable, vector, cubic],
    f64 => [vector],
    DVec2 => [vector],
    DVec3 => [vector],
    DVec4 => [vector],
    Quat => [ease, stable, rotation],
    Rot2 => [ease, stable],
    bool => [],
    Transform => [],
    LinearRgba => [stable, vector, cubic],
    Laba => [stable, vector, cubic],
    Oklaba => [stable, vector, cubic],
    Srgba => [stable, vector, cubic],
    Xyza => [stable, vector, cubic],
}

/// The curve `json` on values of type `A`.
pub fn build<A: ScriptValue>(registry: &TypeRegistry, json: &Value) -> Result<ScriptCurve<A>, String> {
    let (name, args) = one_key(json, "a curve")?;
    let value = |v: &Value| value::<A>(registry, v);
    let values = |v: &Value| list(v, "values")?.iter().map(value).collect::<Result<Vec<A>, String>>();
    let timed = |v: &Value| {
        list(v, "keyframes")?
            .iter()
            .map(|p| match p.as_array().map(Vec::as_slice) {
                Some([t, v]) => Ok((number(t, "a keyframe time")?, value(v)?)),
                _ => Err(format!("a keyframe is [time, value], not {p}")),
            })
            .collect::<Result<Vec<(f32, A)>, String>>()
    };
    let field = |name: &str| args.get(name).ok_or_else(|| format!("{name} is missing"));
    let curve: Dyn<A> = match name {
        "EasingCurve" => A::easing(value(field("start")?)?, value(field("end")?)?, ease(field("ease")?)?)?,
        "ConstantCurve" => Arc::new(ConstantCurve::new(interval(field("domain")?)?, value(field("value")?)?)),
        "SampleAutoCurve" => A::sample_auto(interval(field("domain")?)?, values(field("samples")?)?)?,
        "UnevenSampleAutoCurve" => A::uneven_sample_auto(timed(args)?)?,
        "AnimatableKeyframeCurve" => Arc::new(AnimatableKeyframeCurve::new(timed(args)?).map_err(err)?),
        "SteppedKeyframeCurve" => Arc::new(SteppedKeyframeCurve::new(timed(args)?).map_err(err)?),
        "CubicKeyframeCurve" => A::cubic_keyframe(numbers(field("times")?)?, values(field("values")?)?)?,
        "CubicRotationCurve" => A::cubic_rotation(numbers(field("times")?)?, values(field("values")?)?)?,
        "ToCurve" | "ToCurveCyclic" => A::spline(spline(registry, args)?, name == "ToCurveCyclic")?,
        "ReparametrizeLinear" => {
            let [c, d] = pair(args)?;
            Arc::new(build::<A>(registry, c)?.reparametrize_linear(interval(d)?).map_err(err)?)
        }
        "Reverse" => Arc::new(build::<A>(registry, args)?.reverse().map_err(err)?),
        "Repeat" => {
            let [c, n] = pair(args)?;
            let n = n.as_u64().ok_or_else(|| format!("repeat takes a whole count, not {n}"))? as usize;
            Arc::new(build::<A>(registry, c)?.repeat(n).map_err(err)?)
        }
        "Forever" => Arc::new(build::<A>(registry, args)?.forever().map_err(err)?),
        "PingPong" => Arc::new(build::<A>(registry, args)?.ping_pong().map_err(err)?),
        "Chain" => {
            let [a, b] = pair(args)?;
            Arc::new(build::<A>(registry, a)?.chain(build::<A>(registry, b)?).map_err(err)?)
        }
        "ChainContinue" => {
            let [a, b] = pair(args)?;
            A::chain_continue(build::<A>(registry, a)?, build::<A>(registry, b)?)?
        }
        _ => return Err(format!("{name} is not a curve bevy-ts builds")),
    };
    Ok(ScriptCurve::new(curve, json))
}

fn spline<A: ScriptValue>(registry: &TypeRegistry, json: &Value) -> Result<Spline<A>, String> {
    let (name, args) = one_key(json, "a spline")?;
    let points = |key: &str| -> Result<Vec<A>, String> {
        list(args.get(key).ok_or_else(|| format!("{name} {key} is missing"))?, key)?.iter().map(|v| value::<A>(registry, v)).collect()
    };
    let floats = |key: &str| args.get(key).filter(|v| !v.is_null()).map(numbers).transpose();
    Ok(match name {
        "CubicBezier" => Spline::Bezier(
            list(args.get("control_points").ok_or("CubicBezier control_points is missing")?, "control_points")?
                .iter()
                .map(|seg| {
                    let pts = list(seg, "a Bezier segment")?.iter().map(|v| value::<A>(registry, v)).collect::<Result<Vec<A>, String>>()?;
                    <[A; 4]>::try_from(pts).map_err(|p| format!("a Bezier segment has 4 points, not {}", p.len()))
                })
                .collect::<Result<_, _>>()?,
        ),
        "CubicHermite" => Spline::Hermite(points("control_points")?, points("tangents")?),
        "CubicCardinalSpline" => Spline::Cardinal(number(args.get("tension").ok_or("CubicCardinalSpline tension is missing")?, "tension")?, points("control_points")?),
        "CubicBSpline" => Spline::BSpline(points("control_points")?),
        "CubicNurbs" => Spline::Nurbs(points("control_points")?, floats("weights")?, floats("knots")?),
        "LinearSpline" => Spline::Linear(points("points")?),
        _ => return Err(format!("{name} is not a spline bevy-ts builds")),
    })
}

/// A value of type `A` from its reflected JSON, refused naming both types.
fn value<A: ScriptValue>(registry: &TypeRegistry, json: &Value) -> Result<A, String> {
    let v = bevy_ts::reflect_value::value_of(registry, TypeId::of::<A>(), json)
        .map_err(|e| format!("a {} value, not {} ({json}): {e}", A::short_type_path(), json_kind(json)))?;
    A::from_reflect(v.as_ref()).ok_or_else(|| format!("a {} value, not {}", A::short_type_path(), json_kind(json)))
}

/// What a JSON value looks like, in the type names TS writes it for.
pub fn json_kind(json: &Value) -> &'static str {
    match json {
        Value::Number(_) => "f32",
        Value::Bool(_) => "bool",
        Value::Array(a) => match a.len() {
            2 => "Vec2",
            3 => "Vec3",
            4 => "Vec4 or Quat",
            _ => "a list",
        },
        Value::String(_) => "a string",
        Value::Object(_) => "an object",
        Value::Null => "null",
    }
}

fn one_key<'a>(json: &'a Value, what: &str) -> Result<(&'a str, &'a Value), String> {
    match json.as_object() {
        Some(o) if o.len() == 1 => {
            let (k, v) = o.iter().next().expect("one key");
            Ok((k.as_str(), v))
        }
        _ => Err(format!("{what} is {{ Name: args }}, not {json}")),
    }
}

fn list<'a>(v: &'a Value, what: &str) -> Result<&'a Vec<Value>, String> {
    v.as_array().ok_or_else(|| format!("{what} is a list, not {v}"))
}

fn pair(v: &Value) -> Result<[&Value; 2], String> {
    match v.as_array().map(Vec::as_slice) {
        Some([a, b]) => Ok([a, b]),
        _ => Err(format!("expected [a, b], not {v}")),
    }
}

fn number(v: &Value, what: &str) -> Result<f32, String> {
    v.as_f64().map(|n| n as f32).ok_or_else(|| format!("{what} is a number, not {v}"))
}

fn numbers(v: &Value) -> Result<Vec<f32>, String> {
    list(v, "times")?.iter().map(|n| number(n, "a time")).collect()
}

/// `[start, end]`, `null` for an unbounded end.
fn interval(v: &Value) -> Result<Interval, String> {
    let [s, e] = pair(v)?;
    let end = |v: &Value, inf: f32| if v.is_null() { Ok(inf) } else { number(v, "an interval end") };
    Interval::new(end(s, f32::NEG_INFINITY)?, end(e, f32::INFINITY)?).map_err(err)
}

const EASES: &[(&str, EaseFunction)] = &[
    ("Linear", EaseFunction::Linear),
    ("QuadraticIn", EaseFunction::QuadraticIn),
    ("QuadraticOut", EaseFunction::QuadraticOut),
    ("QuadraticInOut", EaseFunction::QuadraticInOut),
    ("CubicIn", EaseFunction::CubicIn),
    ("CubicOut", EaseFunction::CubicOut),
    ("CubicInOut", EaseFunction::CubicInOut),
    ("QuarticIn", EaseFunction::QuarticIn),
    ("QuarticOut", EaseFunction::QuarticOut),
    ("QuarticInOut", EaseFunction::QuarticInOut),
    ("QuinticIn", EaseFunction::QuinticIn),
    ("QuinticOut", EaseFunction::QuinticOut),
    ("QuinticInOut", EaseFunction::QuinticInOut),
    ("SmoothStepIn", EaseFunction::SmoothStepIn),
    ("SmoothStepOut", EaseFunction::SmoothStepOut),
    ("SmoothStep", EaseFunction::SmoothStep),
    ("SmootherStepIn", EaseFunction::SmootherStepIn),
    ("SmootherStepOut", EaseFunction::SmootherStepOut),
    ("SmootherStep", EaseFunction::SmootherStep),
    ("SineIn", EaseFunction::SineIn),
    ("SineOut", EaseFunction::SineOut),
    ("SineInOut", EaseFunction::SineInOut),
    ("CircularIn", EaseFunction::CircularIn),
    ("CircularOut", EaseFunction::CircularOut),
    ("CircularInOut", EaseFunction::CircularInOut),
    ("ExponentialIn", EaseFunction::ExponentialIn),
    ("ExponentialOut", EaseFunction::ExponentialOut),
    ("ExponentialInOut", EaseFunction::ExponentialInOut),
    ("ElasticIn", EaseFunction::ElasticIn),
    ("ElasticOut", EaseFunction::ElasticOut),
    ("ElasticInOut", EaseFunction::ElasticInOut),
    ("BackIn", EaseFunction::BackIn),
    ("BackOut", EaseFunction::BackOut),
    ("BackInOut", EaseFunction::BackInOut),
    ("BounceIn", EaseFunction::BounceIn),
    ("BounceOut", EaseFunction::BounceOut),
    ("BounceInOut", EaseFunction::BounceInOut),
];

/// Every `EaseFunction` with no data, by Bevy's name.
pub fn eases() -> impl Iterator<Item = (&'static str, EaseFunction)> {
    EASES.iter().copied()
}

/// An `EaseFunction` as Bevy names its variants: `"CubicOut"`, `{ "Steps": [4, "End"] }`,
/// `{ "Elastic": 2 }`.
pub fn ease(v: &Value) -> Result<EaseFunction, String> {
    if let Some(name) = v.as_str() {
        return EASES.iter().find(|(n, _)| *n == name).map(|(_, f)| *f).ok_or_else(|| format!("{name} is not an EaseFunction"));
    }
    let (name, args) = one_key(v, "an ease")?;
    match name {
        "Steps" => {
            let [n, at] = pair(args)?;
            let n = n.as_u64().ok_or_else(|| format!("Steps takes a whole count, not {n}"))? as usize;
            let at = match at.as_str() {
                Some("Start") => JumpAt::Start,
                Some("End") => JumpAt::End,
                Some("None") => JumpAt::None,
                Some("Both") => JumpAt::Both,
                _ => return Err(format!("a JumpAt is \"Start\", \"End\", \"None\" or \"Both\", not {at}")),
            };
            Ok(EaseFunction::Steps(n, at))
        }
        "Elastic" => Ok(EaseFunction::Elastic(number(args, "Elastic's omega")?)),
        _ => Err(format!("{name} is not an EaseFunction")),
    }
}

/// A component field a curve animates, resolved from its type path and name.
#[derive(Clone)]
pub struct Field {
    pub component: TypeId,
    pub index: usize,
    /// The field's type.
    pub ty: TypeId,
    /// `Transform::translation`, for messages.
    pub label: Arc<str>,
    from_ptr: ReflectFromPtr,
}

/// `{ component, field }` (TS's `animatedField(C, field)`): a reflected, mutable component and
/// one of its top-level fields, with the field's type.
pub fn field(registry: &TypeRegistry, json: &Value) -> Result<Field, String> {
    let path = json.get("component").and_then(Value::as_str).ok_or_else(|| format!("an animated field is {{ component, field }}, not {json}"))?;
    let name = json.get("field").and_then(Value::as_str).ok_or_else(|| format!("an animated field is {{ component, field }}, not {json}"))?;
    let reg = registry
        .get_with_type_path(path)
        .or_else(|| registry.get_with_short_type_path(path))
        .ok_or_else(|| format!("{path} is not a registered type"))?;
    if reg.data::<ReflectComponent>().is_none() {
        return Err(format!("{path} is not a reflected component"));
    }
    let short = reg.type_info().type_path_table().short_path();
    let label = format!("{short}::{name}");
    let (index, ty, ty_path) = match reg.type_info() {
        bevy::reflect::TypeInfo::Struct(s) => {
            let i = s.index_of(name).ok_or_else(|| format!("{short} has no field {name}"))?;
            let f = s.field_at(i).expect("an index of the struct");
            (i, f.type_id(), f.type_path())
        }
        bevy::reflect::TypeInfo::TupleStruct(s) => {
            // TS names a newtype's one field `value`.
            let i: usize = if name == "value" { 0 } else { name.parse().map_err(|_| format!("{short} is a tuple struct: its fields are 0, 1, ..., not {name}"))? };
            let f = s.field_at(i).ok_or_else(|| format!("{short} has no field {name}"))?;
            (i, f.type_id(), f.type_path())
        }
        _ => return Err(format!("{short} is not a struct: a curve animates a struct's field")),
    };
    if !animatable(ty) {
        return Err(format!("{label} is a {ty_path}, which Bevy cannot animate"));
    }
    let from_ptr = reg.data::<ReflectFromPtr>().cloned().ok_or_else(|| format!("{path} has no ReflectFromPtr"))?;
    Ok(Field { component: reg.type_id(), index, ty, label: label.into(), from_ptr })
}

/// `{ property: { component, field }, curve }` on `target`, added to `clip` with Bevy's
/// `add_curve_to_target`; a value of the wrong type fails naming the field and both types. Gives
/// the curve's domain: an unbounded one adds nothing to the clip's duration.
pub fn add_curve(registry: &TypeRegistry, clip: &mut AnimationClip, target: AnimationTargetId, json: &Value) -> Result<Interval, String> {
    let property = json.get("property").ok_or_else(|| format!("an AnimatableCurve is {{ property, curve }}, not {json}"))?;
    let curve = json.get("curve").ok_or_else(|| format!("an AnimatableCurve is {{ property, curve }}, not {json}"))?;
    let field = field(registry, property)?;
    add_typed(registry, clip, target, &field, curve).map_err(|e| {
        let ty = registry.get(field.ty).map_or("?", |r| r.type_info().type_path_table().short_path());
        format!("{} is a {ty}: {e}", field.label)
    })
}

/// Bevy's `AnimatableProperty` for any reflected component field of an animatable type (design
/// 18): the component by id, turned into `&mut dyn Reflect` with `ReflectFromPtr`, its field
/// downcast. Its evaluator id is the one `animated_field!` hashes, so it blends with a glTF clip
/// on the same field.
pub struct ReflectedField<A> {
    field: Field,
    id: Hashed<(TypeId, usize)>,
    marker: std::marker::PhantomData<fn() -> A>,
}

impl<A> Clone for ReflectedField<A> {
    fn clone(&self) -> Self {
        Self { field: self.field.clone(), id: self.id.clone(), marker: std::marker::PhantomData }
    }
}

impl<A> ReflectedField<A> {
    fn new(field: Field) -> Self {
        let id = Hashed::new((field.component, field.index));
        Self { field, id, marker: std::marker::PhantomData }
    }
}

impl<A: ScriptValue> AnimatableProperty for ReflectedField<A> {
    type Property = A;

    fn get_mut<'a>(&self, entity: &'a mut AnimationEntityMut) -> Result<&'a mut A, AnimationEvaluationError> {
        let missing = AnimationEvaluationError::ComponentNotPresent(self.field.component);
        let id: ComponentId = {
            let cell = entity.as_unsafe_entity_cell();
            cell.world().components().get_id(self.field.component).ok_or(missing.clone())?
        };
        let ptr = entity.get_mut_by_id::<ComponentId>(id).ok_or(missing)?.into_inner();
        // SAFETY: `id` is the component of type `field.component`, whose `ReflectFromPtr` this is.
        let value = unsafe { self.field.from_ptr.ptr_as_reflect_mut(ptr) };
        let field = match value.reflect_mut() {
            ReflectMut::Struct(s) => s.field_at_mut(self.field.index),
            ReflectMut::TupleStruct(s) => s.field_mut(self.field.index),
            _ => None,
        };
        field.and_then(|f| f.try_downcast_mut::<A>()).ok_or(AnimationEvaluationError::PropertyNotPresent(TypeId::of::<A>()))
    }

    fn evaluator_id(&self) -> EvaluatorId<'_> {
        EvaluatorId::ComponentField(&self.id)
    }
}
