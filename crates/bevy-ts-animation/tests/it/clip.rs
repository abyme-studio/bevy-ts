//! Script-built clips (change `animation-control`, stage 2): a curve's JSON, as a TS
//! `AnimatableCurve` holds it, built into Bevy's curves on any reflected component field, played by
//! `PlayAnimation` on an entity with no scene. Each sample is checked against Bevy's own curve at
//! the player's seek time.

use bevy::animation::graph::{AnimationGraph, AnimationGraphHandle};
use bevy::animation::{AnimationClip, AnimationPlayer, AnimationTargetId};
use bevy::curve::easing::{EaseFunction, EasingCurve, JumpAt};
use bevy::curve::{Curve, Interval};
use bevy::prelude::*;
use bevy::world_serialization::WorldAssetRoot;
use bevy_ts_animation::curve::{add_curve, build, eases, target};
use bevy_ts_animation::{AnimationTargets, PlayAnimation};
use bevy_ts_material::{FadeMaterial, MaterialAlpha};
use serde_json::{Value, json};
use crate::common::*;

const TRANSFORM: &str = "bevy_transform::components::transform::Transform";

fn field(component: &str, field: &str) -> Value {
    json!({ "component": component, "field": field })
}

/// `ease` from `start` to `end` over `secs`, as TS writes `new EasingCurve(...).reparametrizeLinear(interval(0, secs))`.
fn tween(start: Value, end: Value, ease: Value, secs: f32) -> Value {
    json!({ "ReparametrizeLinear": [{ "EasingCurve": { "start": start, "end": end, "ease": ease } }, [0.0, secs]] })
}

/// A clip of `curves`, `[target, { property, curve }]` each, as the clip hand-map builds it.
fn clip(app: &mut App, curves: &[(Value, Value)]) -> Handle<AnimationClip> {
    let mut c = AnimationClip::default();
    {
        let registry = app.world().resource::<AppTypeRegistry>().read();
        for (t, curve) in curves {
            add_curve(&registry, &mut c, target(t).unwrap(), curve).unwrap();
        }
    }
    app.world_mut().resource_mut::<Assets<AnimationClip>>().add(c)
}

fn root() -> Value {
    json!({ "names": [] })
}

fn tweened(app: &mut App, clip: Handle<AnimationClip>, extra: impl Bundle) -> Entity {
    app.world_mut().spawn((Transform::default(), PlayAnimation { clip, repeat: bevy::animation::RepeatAnimation::Never, ..default() }, extra)).id()
}

fn seek(app: &App, root: Entity) -> f32 {
    let p = app.world().get::<AnimationPlayer>(root).expect("the root is its own player");
    p.playing_animations().next().map_or(0.0, |(_, a)| a.seek_time())
}

fn transform(app: &App, e: Entity) -> Transform {
    *app.world().get::<Transform>(e).unwrap()
}

/// Steps until the root's clip has played `secs`, checking `check` each frame.
fn play(app: &mut App, root: Entity, secs: f32, mut check: impl FnMut(&App, f32)) {
    for _ in 0..((secs / DT).ceil() as usize + 2) {
        app.update();
        let t = seek(app, root);
        check(app, t);
    }
}

fn the_app() -> App {
    bare_app_with(|app| {
        app.add_plugins(bevy_ts_material::ScriptMaterialRenderPlugin);
    })
}

#[test]
fn a_tween_translates_with_an_ease() {
    let mut app = the_app();
    let c = clip(&mut app, &[(root(), json!({ "property": field(TRANSFORM, "translation"), "curve": tween(json!([0, 0, 0]), json!([0, 2, 0]), json!("CubicOut"), 0.5) }))]);
    let e = tweened(&mut app, c, ());
    let bevy = EasingCurve::new(Vec3::ZERO, Vec3::new(0.0, 2.0, 0.0), EaseFunction::CubicOut).reparametrize_linear(Interval::new(0.0, 0.5).unwrap()).unwrap();
    let mut mid = false;
    play(&mut app, e, 0.7, |app, t| {
        let y = transform(app, e).translation.y;
        assert!((y - bevy.sample_clamped(t).y).abs() < 1e-4, "at {t} s: {y} against Bevy's {}", bevy.sample_clamped(t).y);
        mid |= t > 0.2 && t < 0.3;
    });
    assert!(mid, "a sample near 0.25 s");
    assert_eq!(transform(&app, e).translation.y, 2.0, "it ends at 2 and stays");
    assert!(app.world().get::<AnimationTargets>(e).is_some_and(|t| t.players == vec![e]), "the entity is its own player");
}

#[test]
fn a_tween_rotates() {
    let mut app = the_app();
    // A quarter turn: a half turn's end is as near one way round as the other, and slerp may take either.
    let end = Quat::from_rotation_y(std::f32::consts::FRAC_PI_2);
    let c = clip(&mut app, &[(root(), json!({ "property": field(TRANSFORM, "rotation"), "curve": tween(json!([0, 0, 0, 1]), json!([end.x, end.y, end.z, end.w]), json!("Linear"), 1.0) }))]);
    let e = tweened(&mut app, c, ());
    let bevy = EasingCurve::new(Quat::IDENTITY, end, EaseFunction::Linear).reparametrize_linear(Interval::new(0.0, 1.0).unwrap()).unwrap();
    let mut eighth = false;
    play(&mut app, e, 1.0, |app, t| {
        let r = transform(app, e).rotation;
        assert!(r.abs_diff_eq(bevy.sample_clamped(t), 1e-4), "at {t} s: {r} against Bevy's {}", bevy.sample_clamped(t));
        if (t - 0.5).abs() < DT / 2.0 {
            eighth = r.angle_between(Quat::from_rotation_y(std::f32::consts::FRAC_PI_4)) < 1e-3;
        }
    });
    assert!(eighth, "an eighth of a turn at 0.5 s");
}

#[test]
fn scale_ping_pongs() {
    let mut app = the_app();
    let curve = json!({ "PingPong": tween(json!([1, 1, 1]), json!([2, 2, 2]), json!("SineInOut"), 0.4) });
    let c = clip(&mut app, &[(root(), json!({ "property": field(TRANSFORM, "scale"), "curve": curve }))]);
    let e = tweened(&mut app, c, ());
    let bevy = EasingCurve::new(Vec3::ONE, Vec3::splat(2.0), EaseFunction::SineInOut).reparametrize_linear(Interval::new(0.0, 0.4).unwrap()).unwrap().ping_pong().unwrap();
    let mut peak = 0.0f32;
    play(&mut app, e, 0.8, |app, t| {
        let s = transform(app, e).scale;
        assert!(s.abs_diff_eq(bevy.sample_clamped(t), 1e-4), "at {t} s: {s}");
        peak = peak.max(s.x);
    });
    assert!(peak > 1.99, "it reaches 2: {peak}");
    assert!((transform(&app, e).scale.x - 1.0).abs() < 1e-4, "and is back at 1 at 0.8 s");
}

#[test]
fn a_fade_draws_the_frame_it_samples() {
    let mut app = the_app();
    let c = clip(&mut app, &[(root(), json!({ "property": field("bevy_ts_material::alpha::MaterialAlpha", "alpha"), "curve": tween(json!(1.0), json!(0.0), json!("QuadraticIn"), 1.0) }))]);
    let mesh = app.world_mut().resource_mut::<Assets<Mesh>>().add(Cuboid::default());
    let red = app.world_mut().resource_mut::<Assets<StandardMaterial>>().add(StandardMaterial::from(Color::srgb(1.0, 0.0, 0.0)));
    let e = tweened(&mut app, c, (Mesh3d(mesh), MeshMaterial3d(red), MaterialAlpha { alpha: 1.0 }));
    let bevy = EasingCurve::new(1.0f32, 0.0, EaseFunction::QuadraticIn);
    play(&mut app, e, 1.0, |app, t| {
        let alpha = app.world().get::<MaterialAlpha>(e).unwrap().alpha;
        assert!((alpha - bevy.sample_clamped(t)).abs() < 1e-4, "at {t} s: alpha {alpha}");
        if let Some(copy) = app.world().get::<MeshMaterial3d<FadeMaterial>>(e) {
            let drawn = app.world().resource::<Assets<FadeMaterial>>().get(&copy.0).unwrap().extension.alpha;
            assert_eq!(drawn, alpha.clamp(0.0, 1.0), "at {t} s the copy draws the alpha sampled this frame");
        }
    });
    assert_eq!(app.world().get::<MaterialAlpha>(e).unwrap().alpha, 0.0, "0 at 1 s");
}

#[test]
fn a_named_child_moves_alone() {
    let mut app = the_app();
    let c = clip(&mut app, &[(json!({ "names": ["lid"] }), json!({ "property": field(TRANSFORM, "translation"), "curve": tween(json!([0, 0, 0]), json!([0, 1, 0]), json!("Linear"), 0.5) }))]);
    let e = tweened(&mut app, c, ());
    let lid = app.world_mut().spawn((Name::new("lid"), Transform::default(), ChildOf(e))).id();
    play(&mut app, e, 0.6, |_, _| {});
    assert_eq!(transform(&app, lid).translation.y, 1.0, "the lid moves");
    assert_eq!(transform(&app, e).translation, Vec3::ZERO, "its parent does not");
    assert_eq!(app.world().get::<AnimationTargetId>(lid), Some(&AnimationTargetId::from_names([Name::new("lid")].iter())));
}

#[test]
fn a_wrong_field_fails() {
    let app = the_app();
    let registry = app.world().resource::<AppTypeRegistry>().read();
    let mut c = AnimationClip::default();
    let curve = json!({ "property": field(TRANSFORM, "translation"), "curve": tween(json!(0), json!(1), json!("Linear"), 1.0) });
    let e = add_curve(&registry, &mut c, AnimationTargetId::from_names(std::iter::empty()), &curve).unwrap_err();
    assert!(e.contains("Transform::translation") && e.contains("Vec3") && e.contains("f32"), "{e}");
    let nope = json!({ "property": field(TRANSFORM, "nope"), "curve": tween(json!(0), json!(1), json!("Linear"), 1.0) });
    let e = add_curve(&registry, &mut c, AnimationTargetId::from_names(std::iter::empty()), &nope).unwrap_err();
    assert!(e.contains("Transform") && e.contains("nope"), "{e}");
    let name = json!({ "property": field("bevy_ecs::name::Name", "name"), "curve": tween(json!(0), json!(1), json!("Linear"), 1.0) });
    assert!(add_curve(&registry, &mut c, AnimationTargetId::from_names(std::iter::empty()), &name).is_err(), "a field Bevy cannot animate");
}

#[test]
fn a_paused_tween_parks() {
    let mut app = the_app();
    let curve = json!({ "Forever": tween(json!([0, 0, 0]), json!([0, 2, 0]), json!("Linear"), 1.0) });
    let c = clip(&mut app, &[(root(), json!({ "property": field(TRANSFORM, "translation"), "curve": curve }))]);
    let e = tweened(&mut app, c, ());
    play(&mut app, e, 0.2, |_, _| {});
    app.world_mut().get_mut::<PlayAnimation>(e).unwrap().paused = true;
    app.update();
    app.update();
    assert!(app.world().get::<AnimationGraphHandle>(e).is_none(), "parked");
    let held = transform(&app, e);
    play(&mut app, e, 0.2, |_, _| {});
    assert_eq!(transform(&app, e), held, "the transform stops changing");
}

#[test]
fn a_script_rotation_blends_with_a_gltf_clip() {
    let mut app = app();
    let leg = AnimationTargetId::from_names("root/_rootJoint/b_Root_00/b_Hip_01/b_LeftLeg01_015".split('/').map(Name::new).collect::<Vec<_>>().iter());
    let still = clip(&mut app, &[(json!(leg.0.to_string()), json!({ "property": field(TRANSFORM, "rotation"), "curve": { "ConstantCurve": { "domain": [0, 10], "value": [0, 0, 0, 1] } } }))]);
    let run: Handle<AnimationClip> = load(&app, RUN);
    let scene = load(&app, SCENE);
    let blended = {
        let mut g = AnimationGraph::new();
        let b = g.add_blend(1.0, g.root);
        g.add_clip(run.clone(), 0.5, b);
        g.add_clip(still, 0.5, b);
        app.world_mut().resource_mut::<Assets<AnimationGraph>>().add(g)
    };
    let alone = {
        let (g, _) = AnimationGraph::from_clip(run);
        app.world_mut().resource_mut::<Assets<AnimationGraph>>().add(g)
    };
    let a = app.world_mut().spawn((WorldAssetRoot(scene.clone()), PlayAnimation { graph: alone, ..default() })).id();
    let b = app.world_mut().spawn((WorldAssetRoot(scene), PlayAnimation { graph: blended, ..default() })).id();
    ready(&mut app, a);
    ready(&mut app, b);
    for _ in 0..20 {
        app.update();
    }
    let bone = |root: Entity| {
        let w = app.world();
        let mut stack = vec![root];
        while let Some(e) = stack.pop() {
            if w.get::<AnimationTargetId>(e) == Some(&leg) {
                return w.get::<Transform>(e).unwrap().rotation;
            }
            stack.extend(w.get::<Children>(e).into_iter().flat_map(|c| c.iter()));
        }
        panic!("no leg bone");
    };
    let (ran, mixed) = (bone(a), bone(b));
    let want = Quat::IDENTITY.slerp(ran, 0.5);
    assert!(!ran.abs_diff_eq(Quat::IDENTITY, 1e-3), "the run moves the leg");
    assert!(mixed.abs_diff_eq(want, 1e-2) || mixed.abs_diff_eq(-want, 1e-2), "the script rotation blends half and half with the run's: {mixed} against {want}");
}

/// Samples `json` built as a curve of `f32` against `bevy` at times across its domain.
fn same_f32(json: Value, bevy: &dyn Curve<f32>) {
    let mut registry = bevy::reflect::TypeRegistry::default();
    registry.register::<f32>();
    let ours = build::<f32>(&registry, &json).unwrap_or_else(|e| panic!("{json}: {e}"));
    assert_eq!(ours.domain(), bevy.domain(), "{json}");
    let d = bevy.domain();
    let s = if d.has_finite_start() { d.start() } else { -5.0 };
    let e = if d.has_finite_end() { d.end() } else { s + 5.0 };
    for i in 0..=40 {
        let t = s + (e - s) * i as f32 / 40.0;
        let (a, b) = (ours.sample_clamped(t), bevy.sample_clamped(t));
        assert!((a - b).abs() < 1e-5, "{json} at {t}: {a} against Bevy's {b}");
    }
}

#[test]
fn every_ease_samples_as_bevy() {
    let mut all: Vec<(Value, EaseFunction)> = eases().map(|(n, f)| (json!(n), f)).collect();
    all.push((json!({ "Steps": [4, "End"] }), EaseFunction::Steps(4, JumpAt::End)));
    all.push((json!({ "Steps": [3, "Both"] }), EaseFunction::Steps(3, JumpAt::Both)));
    all.push((json!({ "Elastic": 2.0 }), EaseFunction::Elastic(2.0)));
    assert_eq!(all.len(), 40, "every EaseFunction variant");
    for (name, f) in all {
        same_f32(json!({ "EasingCurve": { "start": 0.0, "end": 1.0, "ease": name } }), &EasingCurve::new(0.0f32, 1.0, f));
    }
}

#[test]
fn every_curve_samples_as_bevy() {
    use bevy::animation::animation_curves::AnimatableKeyframeCurve;
    use bevy::animation::gltf_curves::{CubicKeyframeCurve, SteppedKeyframeCurve};
    use bevy::curve::cubic_splines::{CubicBSpline, CubicBezier, CubicCardinalSpline, CubicGenerator, CubicHermite, CubicNurbs, CyclicCubicGenerator, LinearSpline, RationalGenerator};
    use bevy::curve::{ConstantCurve, SampleAutoCurve, UnevenSampleAutoCurve};
    let unit = Interval::new(0.0, 1.0).unwrap();
    let keys = json!([[0.0, 1.0], [0.5, 3.0], [2.0, -1.0]]);
    let pairs = [(0.0, 1.0f32), (0.5, 3.0), (2.0, -1.0)];
    let ease = || EasingCurve::new(0.0f32, 1.0, EaseFunction::SineInOut);
    let ease_json = json!({ "EasingCurve": { "start": 0.0, "end": 1.0, "ease": "SineInOut" } });
    same_f32(json!({ "ConstantCurve": { "domain": [0.0, 2.0], "value": 4.0 } }), &ConstantCurve::new(Interval::new(0.0, 2.0).unwrap(), 4.0f32));
    same_f32(json!({ "SampleAutoCurve": { "domain": [0.0, 1.0], "samples": [0.0, 2.0, 1.0] } }), &SampleAutoCurve::new(unit, [0.0f32, 2.0, 1.0]).unwrap());
    same_f32(json!({ "UnevenSampleAutoCurve": keys }), &UnevenSampleAutoCurve::new(pairs).unwrap());
    same_f32(json!({ "AnimatableKeyframeCurve": keys }), &AnimatableKeyframeCurve::new(pairs).unwrap());
    same_f32(json!({ "SteppedKeyframeCurve": keys }), &SteppedKeyframeCurve::new(pairs).unwrap());
    same_f32(
        json!({ "CubicKeyframeCurve": { "times": [0.0, 1.0], "values": [0.0, 0.0, 1.0, -1.0, 2.0, 0.0] } }),
        &CubicKeyframeCurve::new([0.0, 1.0], [0.0f32, 0.0, 1.0, -1.0, 2.0, 0.0]).unwrap(),
    );
    let pts = [0.0f32, 1.0, 3.0, 2.0, 5.0];
    let pts_json = json!(pts);
    same_f32(json!({ "ToCurve": { "CubicBezier": { "control_points": [[0.0, 1.0, 3.0, 2.0]] } } }), &CubicBezier::new([[0.0f32, 1.0, 3.0, 2.0]]).to_curve().unwrap());
    same_f32(json!({ "ToCurve": { "CubicHermite": { "control_points": [0.0, 2.0], "tangents": [1.0, -1.0] } } }), &CubicHermite::new([0.0f32, 2.0], [1.0, -1.0]).to_curve().unwrap());
    same_f32(json!({ "ToCurveCyclic": { "CubicHermite": { "control_points": [0.0, 2.0], "tangents": [1.0, -1.0] } } }), &CubicHermite::new([0.0f32, 2.0], [1.0, -1.0]).to_curve_cyclic().unwrap());
    same_f32(json!({ "ToCurve": { "CubicCardinalSpline": { "tension": 0.5, "control_points": pts_json } } }), &CubicCardinalSpline::new(0.5, pts).to_curve().unwrap());
    same_f32(json!({ "ToCurveCyclic": { "CubicCardinalSpline": { "tension": 0.5, "control_points": pts_json } } }), &CubicCardinalSpline::new(0.5, pts).to_curve_cyclic().unwrap());
    same_f32(json!({ "ToCurve": { "CubicBSpline": { "control_points": pts_json } } }), &CubicBSpline::new(pts).to_curve().unwrap());
    same_f32(json!({ "ToCurveCyclic": { "CubicBSpline": { "control_points": pts_json } } }), &CubicBSpline::new(pts).to_curve_cyclic().unwrap());
    same_f32(json!({ "ToCurve": { "CubicNurbs": { "control_points": pts_json, "weights": null, "knots": null } } }), &CubicNurbs::new(pts, None::<Vec<f32>>, None::<Vec<f32>>).unwrap().to_curve().unwrap());
    same_f32(json!({ "ToCurve": { "LinearSpline": { "points": pts_json } } }), &LinearSpline::new(pts).to_curve().unwrap());
    same_f32(json!({ "ToCurveCyclic": { "LinearSpline": { "points": pts_json } } }), &LinearSpline::new(pts).to_curve_cyclic().unwrap());

    let two = Interval::new(0.0, 2.0).unwrap();
    same_f32(json!({ "ReparametrizeLinear": [ease_json, [0.0, 2.0]] }), &ease().reparametrize_linear(two).unwrap());
    same_f32(json!({ "Reverse": ease_json }), &ease().reverse().unwrap());
    same_f32(json!({ "Repeat": [ease_json, 2] }), &ease().repeat(2).unwrap());
    same_f32(json!({ "Forever": ease_json }), &ease().forever().unwrap());
    same_f32(json!({ "PingPong": ease_json }), &ease().ping_pong().unwrap());
    let later = json!({ "ReparametrizeLinear": [ease_json, [1.0, 2.0]] });
    let later_bevy = || ease().reparametrize_linear(Interval::new(1.0, 2.0).unwrap()).unwrap();
    same_f32(json!({ "Chain": [ease_json, later] }), &ease().chain(later_bevy()).unwrap());
    same_f32(json!({ "ChainContinue": [ease_json, later] }), &ease().chain_continue(later_bevy()).unwrap());
}
