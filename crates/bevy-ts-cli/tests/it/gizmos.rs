//! Bevy's `Gizmos` from TS (openspec script-gizmos): the param per group, builders, config,
//! and one same-buffer row per drawing method (`ts_draws_what_rust_draws`).

use crate::common;

use bevy::gizmos::GizmoAsset;
use bevy::gizmos::GizmoHandles;
use bevy::gizmos::GizmoPlugin;
use bevy::gizmos::config::{DefaultGizmoConfigGroup, GizmoConfigGroup};
use bevy::light::gizmos::{LightGizmoConfigGroup, LightGizmoPlugin};
use bevy::prelude::*;
use common::*;
use std::any::TypeId;

const HEAD: &str = r##"import { app, PostUpdate, Update } from "bevy-ts";
import type { Local } from "bevy-ts";
import type { Gizmos, LightGizmoConfigGroup } from "bevy-ts/gizmos";
"##;

/// A headless app with Bevy's gizmos, the light group and the engine's `GizmosPlugin`.
fn gizmo_app(name: &str, body: &str, setup: impl FnOnce(&mut App)) -> App {
    let dir = project(name, &[("game.ts", &format!("{HEAD}{body}"))]);
    let mut app = app(&dir.join("game.ts"));
    app.add_plugins(AssetPlugin::default()).init_asset::<Mesh>().init_asset::<bevy::mesh::skinning::SkinnedMeshInverseBindposes>();
    app.add_plugins((GizmoPlugin, LightGizmoPlugin, bevy_ts_cli::gizmos::GizmosPlugin));
    setup(&mut app);
    let r = reload(&mut app, false);
    assert!(r.errors.is_empty(), "{:?}", r.errors);
    app
}

/// What a frame drew into group `C`: the four vectors Bevy moved into the group's asset.
#[derive(Debug, Default, PartialEq)]
struct Drawn {
    lines: Vec<Vec3>,
    line_colors: Vec<LinearRgba>,
    strips: Vec<Vec3>,
    strip_colors: Vec<LinearRgba>,
}

fn drawn<C: GizmoConfigGroup>(app: &App) -> Drawn {
    let Some(Some(handle)) = app.world().resource::<GizmoHandles>().handles().get(&TypeId::of::<C>()) else {
        return Drawn::default();
    };
    let b = app.world().resource::<Assets<GizmoAsset>>().get(handle).unwrap().buffer();
    Drawn { lines: b.list_positions.clone(), line_colors: b.list_colors.clone(), strips: b.strip_positions.clone(), strip_colors: b.strip_colors.clone() }
}

/// Log lines starting with `prefix`, the prefix cut.
/// What the systems logged with `prefix`. Console output reaches the log when the runtime is
/// pumped, so pump it.
fn logged(app: &mut App, prefix: &str) -> Vec<String> {
    bevy_ts::runtime::pump(app.world_mut());
    lines(app, prefix)
}

fn lines(app: &App, prefix: &str) -> Vec<String> {
    messages(app).into_iter().filter_map(|m| m.strip_prefix(prefix).map(str::to_string)).collect()
}

#[test]
fn a_system_draws_lines() {
    let body = r##"
export function draw(gizmos: Gizmos) { gizmos.line([0, 0, 0], [1, 0, 0], "#f00"); }
app.addSystems(Update, draw);
"##;
    let mut app = gizmo_app("gizmos-lines", body, |_| {});
    app.update();
    let d = drawn::<DefaultGizmoConfigGroup>(&app);
    assert_eq!(d.lines, vec![Vec3::ZERO, Vec3::X]);
    assert_eq!(d.line_colors, vec![LinearRgba::RED; 2]);
}

#[test]
fn a_throw_drops_the_drawing() {
    let body = r##"
export function draw(gizmos: Gizmos) { gizmos.line([0, 0, 0], [1, 0, 0], "#f00"); throw new Error("no"); }
app.addSystems(Update, draw);
"##;
    let mut app = gizmo_app("gizmos-throw", body, |_| {});
    app.update();
    assert_eq!(drawn::<DefaultGizmoConfigGroup>(&app), Drawn::default());
}

#[test]
fn each_group_draws_into_its_own_storage() {
    let body = r##"
export function draw(light: Gizmos<LightGizmoConfigGroup>) { light.line([0, 0, 0], [0, 1, 0], "#0f0"); }
app.addSystems(Update, draw);
"##;
    let mut app = gizmo_app("gizmos-groups", body, |_| {});
    app.update();
    assert_eq!(drawn::<LightGizmoConfigGroup>(&app).lines, vec![Vec3::ZERO, Vec3::Y]);
    assert_eq!(drawn::<DefaultGizmoConfigGroup>(&app), Drawn::default());
}

#[test]
fn a_gizmos_kept_from_another_system_throws() {
    let body = r##"
let kept: Gizmos | undefined;
export function a(gizmos: Gizmos) { kept = gizmos; }
export function b(l: Local<{ n: u32 }>) {
  try { kept!.line([0, 0, 0], [1, 0, 0], "#f00"); } catch (e) { console.log(`threw ${(e as Error).message}`); }
}
app.addSystems(Update, a).addSystems(PostUpdate, b);
"##;
    let mut app = gizmo_app("gizmos-kept", body, |_| {});
    app.update();
    let threw = logged(&mut app, "threw ");
    assert!(threw.first().is_some_and(|m| m.starts_with("Gizmos.line: this system does not take Gizmos")), "{threw:?}");
    assert_eq!(drawn::<DefaultGizmoConfigGroup>(&app), Drawn::default());
}

#[test]
fn a_builder_setting_redraws_the_shape() {
    let body = r##"
export function draw(gizmos: Gizmos) { gizmos.circle([0, 0, 0], 1, "#fff").resolution(64); }
app.addSystems(Update, draw);
"##;
    let mut app = gizmo_app("gizmos-builder", body, |_| {});
    app.update();
    // One 64-segment circle (65 strip points and a separator), no 32-segment one.
    let mut want = bevy::gizmos::gizmos::GizmoBuffer::<DefaultGizmoConfigGroup, ()>::default();
    want.circle(Vec3::ZERO, 1.0, Color::WHITE).resolution(64);
    let got = drawn::<DefaultGizmoConfigGroup>(&app).strips;
    assert_close(&got, &want.strip_positions);
}

#[test]
fn a_late_setting_throws() {
    let body = r##"
export function draw(gizmos: Gizmos) {
  const c = gizmos.circle([0, 0, 0], 1, "#fff");
  gizmos.line([0, 0, 0], [1, 0, 0], "#f00");
  try { c.resolution(64); } catch (e) { console.log(`threw ${(e as Error).message}`); }
}
app.addSystems(Update, draw);
"##;
    let mut app = gizmo_app("gizmos-late", body, |_| {});
    app.update();
    let threw = logged(&mut app, "threw ");
    assert!(threw.first().is_some_and(|m| m.contains("EllipseBuilder.resolution: call it right after the call that returned the builder")), "{threw:?}");
    let mut want = bevy::gizmos::gizmos::GizmoBuffer::<DefaultGizmoConfigGroup, ()>::default();
    want.circle(Vec3::ZERO, 1.0, Color::WHITE);
    want.line(Vec3::ZERO, Vec3::X, Color::srgb(1.0, 0.0, 0.0));
    let d = drawn::<DefaultGizmoConfigGroup>(&app);
    assert_eq!(d.lines, want.list_positions, "the line");
    assert_close(&d.strips, &want.strip_positions);
}

#[test]
fn a_builder_kept_past_another_builder_throws() {
    let body = r##"
export function draw(gizmos: Gizmos) {
  const c = gizmos.circle([0, 0, 0], 1, "#fff");
  gizmos.sphere([0, 0, 0], 1, "#f00");
  try { c.resolution(64); } catch (e) { console.log(`threw ${(e as Error).message}`); }
  const a = gizmos.arrow([0, 0, 0], [1, 0, 0], "#0f0");
  gizmos.circle([0, 0, 0], 2, "#fff");
  try { a.withTipLength(0.5); } catch (e) { console.log(`threw ${(e as Error).message}`); }
}
app.addSystems(Update, draw);
"##;
    let mut app = gizmo_app("gizmos-kept-builder", body, |_| {});
    app.update();
    let threw = logged(&mut app, "threw ");
    assert_eq!(
        threw,
        [
            "EllipseBuilder.resolution: call it right after the call that returned the builder",
            "ArrowBuilder.withTipLength: call it right after the call that returned the builder"
        ],
        "{:?}",
        messages(&app)
    );
    // Every shape as drawn, the sphere and the second circle at their defaults.
    let mut want = bevy::gizmos::gizmos::GizmoBuffer::<DefaultGizmoConfigGroup, ()>::default();
    want.circle(Vec3::ZERO, 1.0, Color::WHITE);
    want.sphere(Vec3::ZERO, 1.0, Color::srgb(1.0, 0.0, 0.0));
    want.arrow(Vec3::ZERO, Vec3::X, Color::srgb(0.0, 1.0, 0.0));
    want.circle(Vec3::ZERO, 2.0, Color::WHITE);
    let d = drawn::<DefaultGizmoConfigGroup>(&app);
    assert_eq!(d.lines, want.list_positions);
    assert_close(&d.strips, &want.strip_positions);
}

#[test]
fn a_long_strip_crosses_as_a_slice() {
    let body = r##"
const points = new Float32Array(3000);
for (let k = 0; k < 1000; k++) points[k * 3] = k;
export function draw(gizmos: Gizmos, l: Local<{ calls: u32 }>) {
  gizmos.linestrip(points, "#0f0");
  try { gizmos.linestrip(new Float32Array(4), "#0f0"); } catch (e) { console.log(`threw ${(e as Error).message}`); }
}
app.addSystems(Update, draw);
"##;
    let mut app = gizmo_app("gizmos-strip", body, |_| {});
    let before = calls(&app);
    app.update();
    // One host call for the strip; the bad one throws in JS, before any call.
    assert_eq!(calls(&app) - before, 1);
    let d = drawn::<DefaultGizmoConfigGroup>(&app);
    assert_eq!(d.strips.iter().filter(|p| p.is_finite()).count(), 1000);
    assert_eq!(d.strips[999], Vec3::new(999.0, 0.0, 0.0));
    let threw = logged(&mut app, "threw ");
    assert!(threw.first().is_some_and(|m| m == "Gizmos.linestrip: positions holds 4 floats, not a whole number of 3-float points"), "{threw:?}");
}

#[test]
fn isometry_takes_bevys_into_forms() {
    let body = r##"
const q = { x: 0, y: Math.sin(0.25), z: 0, w: Math.cos(0.25) };
export function draw(gizmos: Gizmos) {
  gizmos.rect([1, 2, 3], [1, 1], "#fff");
  gizmos.rect(q, [1, 1], "#fff");
  gizmos.rect([q.x, q.y, q.z, q.w], [1, 1], "#fff");
  gizmos.rect({ translation: { x: 1, y: 2, z: 3 }, rotation: q }, [1, 1], "#fff");
  gizmos.rect2d([1, 2], [1, 1], "#fff");
  gizmos.rect2d({ cos: Math.cos(0.5), sin: Math.sin(0.5) }, [1, 1], "#fff");
  gizmos.rect2d({ translation: [1, 2], rotation: { cos: Math.cos(0.5), sin: Math.sin(0.5) } }, [1, 1], "#fff");
  try { gizmos.rect2d({ rotation: 0.5 } as any, [1, 1], "#fff"); } catch (e) { console.log(`threw ${(e as Error).message}`); }
}
app.addSystems(Update, draw);
"##;
    let mut app = gizmo_app("gizmos-iso", body, |_| {});
    app.update();
    let q = Quat::from_rotation_y(0.5);
    let r = Rot2::radians(0.5);
    let mut want = bevy::gizmos::gizmos::GizmoBuffer::<DefaultGizmoConfigGroup, ()>::default();
    let size = Vec2::ONE;
    want.rect(Vec3::new(1.0, 2.0, 3.0), size, Color::WHITE);
    want.rect(q, size, Color::WHITE);
    want.rect(q, size, Color::WHITE);
    want.rect(Isometry3d::new(Vec3::new(1.0, 2.0, 3.0), q), size, Color::WHITE);
    want.rect_2d(Vec2::new(1.0, 2.0), size, Color::WHITE);
    want.rect_2d(r, size, Color::WHITE);
    want.rect_2d(Isometry2d::new(Vec2::new(1.0, 2.0), r), size, Color::WHITE);
    let d = drawn::<DefaultGizmoConfigGroup>(&app);
    assert_close(&d.strips, &want.strip_positions);
    let threw = logged(&mut app, "threw ");
    assert!(threw.first().is_some_and(|m| m.contains("a Rot2 is { cos, sin }, not a number")), "{threw:?}");
}

#[test]
fn config_reads_through_reflection() {
    let body = r##"
export function read(gizmos: Gizmos, light: Gizmos<LightGizmoConfigGroup>, l: Local<{ n: u32 }>) {
  if (l.n++ > 0) return;
  console.log(`config ${JSON.stringify(gizmos.config())}`);
  console.log(`ext ${JSON.stringify(light.configExt())}`);
}
app.addSystems(Update, read);
"##;
    let mut app = gizmo_app("gizmos-config", body, |_| {});
    app.update();
    let config: serde_json::Value = serde_json::from_str(&logged(&mut app, "config ")[0]).unwrap();
    assert_eq!(config["enabled"], true);
    assert_eq!(config["line"]["width"], 2.0);
    let ext: serde_json::Value = serde_json::from_str(&logged(&mut app, "ext ")[0]).unwrap();
    assert_eq!(ext["draw_all"], false);
}

fn assert_close(got: &[Vec3], want: &[Vec3]) {
    assert_eq!(got.len(), want.len(), "{got:?}\n{want:?}");
    for (k, (g, w)) in got.iter().zip(want).enumerate() {
        // NaN separates strips on both sides.
        assert!((g.is_nan() && w.is_nan()) || g.abs_diff_eq(*w, 1e-5), "vertex {k}: {g} != {w}");
    }
}

type Buf = bevy::gizmos::gizmos::GizmoBuffer<DefaultGizmoConfigGroup, ()>;

/// One same-buffer row: the entries and `Builder.setting`s it covers, the TS call, the Rust call.
struct Row {
    covers: &'static [&'static str],
    ts: &'static str,
    rust: fn(&mut Buf),
}

const RED: Color = Color::srgb(1.0, 0.0, 0.0);
const GREEN: Color = Color::srgb(0.0, 1.0, 0.0);
const BLUE: Color = Color::srgb(0.0, 0.0, 1.0);

fn rows() -> Vec<Row> {
    use bevy::shape::Aabb3d;
    vec![
        Row { covers: &["line"], ts: r##"g.line([0, 0, 0], [1, 2, 3], "#f00")"##, rust: |g| g.line(Vec3::ZERO, Vec3::new(1.0, 2.0, 3.0), RED) },
        Row { covers: &["lineGradient"], ts: r##"g.lineGradient([0, 0, 0], [1, 0, 0], "#f00", srgb(0, 0, 1))"##, rust: |g| g.line_gradient(Vec3::ZERO, Vec3::X, RED, BLUE) },
        Row { covers: &["ray"], ts: r##"g.ray([1, 0, 0], { x: 0, y: 2, z: 0 }, "#0f0")"##, rust: |g| g.ray(Vec3::X, Vec3::new(0.0, 2.0, 0.0), GREEN) },
        Row { covers: &["rayGradient"], ts: r##"g.rayGradient([1, 0, 0], [0, 2, 0], "#0f0", [0, 0, 1])"##, rust: |g| g.ray_gradient(Vec3::X, Vec3::new(0.0, 2.0, 0.0), GREEN, BLUE) },
        Row {
            covers: &["linestrip"],
            ts: r##"g.linestrip([[0, 0, 0], [1, 0, 0], { x: 1, y: 1, z: 0 }], "#f00")"##,
            rust: |g| g.linestrip([Vec3::ZERO, Vec3::X, Vec3::new(1.0, 1.0, 0.0)], RED),
        },
        Row { covers: &["lineloop"], ts: r##"g.lineloop(new Float32Array([0, 0, 0, 1, 0, 0, 1, 1, 0]), "#f00")"##, rust: |g| g.lineloop([Vec3::ZERO, Vec3::X, Vec3::new(1.0, 1.0, 0.0)], RED) },
        Row {
            covers: &["linestripGradient"],
            ts: r##"g.linestripGradient([[[0, 0, 0], "#f00"], [[1, 0, 0], "#0f0"], [[1, 1, 0], srgb(0, 0, 1)]])"##,
            rust: |g| g.linestrip_gradient([(Vec3::ZERO, RED), (Vec3::X, GREEN), (Vec3::new(1.0, 1.0, 0.0), BLUE)]),
        },
        Row { covers: &["line2d"], ts: r##"g.line2d([0, 0], [1, 2], "#f00")"##, rust: |g| g.line_2d(Vec2::ZERO, Vec2::new(1.0, 2.0), RED) },
        Row { covers: &["lineGradient2d"], ts: r##"g.lineGradient2d([0, 0], [1, 2], "#f00", "#00f")"##, rust: |g| g.line_gradient_2d(Vec2::ZERO, Vec2::new(1.0, 2.0), RED, BLUE) },
        Row { covers: &["linestrip2d"], ts: r##"g.linestrip2d([[0, 0], { x: 1, y: 0 }, [1, 1]], "#f00")"##, rust: |g| g.linestrip_2d([Vec2::ZERO, Vec2::X, Vec2::ONE], RED) },
        Row { covers: &["lineloop2d"], ts: r##"g.lineloop2d(new Float32Array([0, 0, 1, 0, 1, 1]), "#f00")"##, rust: |g| g.lineloop_2d([Vec2::ZERO, Vec2::X, Vec2::ONE], RED) },
        Row {
            covers: &["linestripGradient2d"],
            ts: r##"g.linestripGradient2d([[[0, 0], "#f00"], [[1, 0], "#0f0"]])"##,
            rust: |g| g.linestrip_gradient_2d([(Vec2::ZERO, RED), (Vec2::X, GREEN)]),
        },
        Row { covers: &["ray2d"], ts: r##"g.ray2d([1, 0], [0, 2], "#0f0")"##, rust: |g| g.ray_2d(Vec2::X, Vec2::new(0.0, 2.0), GREEN) },
        Row { covers: &["rayGradient2d"], ts: r##"g.rayGradient2d([1, 0], [0, 2], "#0f0", "#00f")"##, rust: |g| g.ray_gradient_2d(Vec2::X, Vec2::new(0.0, 2.0), GREEN, BLUE) },
        Row { covers: &["rect"], ts: r##"g.rect([1, 2, 3], [2, 1], "#f00")"##, rust: |g| g.rect(Vec3::new(1.0, 2.0, 3.0), Vec2::new(2.0, 1.0), RED) },
        Row { covers: &["rect2d"], ts: r##"g.rect2d([1, 2], [2, 1], "#f00")"##, rust: |g| g.rect_2d(Vec2::new(1.0, 2.0), Vec2::new(2.0, 1.0), RED) },
        Row {
            covers: &["cube"],
            ts: r##"g.cube({ translation: [1, 0, 0], scale: [2, 2, 2] }, "#f00")"##,
            rust: |g| g.cube(Transform::from_xyz(1.0, 0.0, 0.0).with_scale(Vec3::splat(2.0)), RED),
        },
        Row {
            covers: &["cube"],
            ts: r##"g.cube({ value: [2, 0, 0, 0, 2, 0, 0, 0, 2, 1, 0, 0] }, "#f00")"##,
            rust: |g| g.cube(GlobalTransform::from(Transform::from_xyz(1.0, 0.0, 0.0).with_scale(Vec3::splat(2.0))), RED),
        },
        Row {
            covers: &["aabb3d"],
            ts: r##"g.aabb3d({ min: [-1, -1, -1], max: [1, 2, 1] }, { translation: [0, 1, 0] }, "#ff0")"##,
            rust: |g| g.aabb_3d(Aabb3d { min: Vec3A::splat(-1.0), max: Vec3A::new(1.0, 2.0, 1.0) }, Transform::from_xyz(0.0, 1.0, 0.0), Color::srgb(1.0, 1.0, 0.0)),
        },
        Row { covers: &["cross"], ts: r##"g.cross([0, 1, 0], 0.5, "#f00")"##, rust: |g| g.cross(Vec3::Y, 0.5, RED) },
        Row { covers: &["cross2d"], ts: r##"g.cross2d([0, 1], 0.5, "#f00")"##, rust: |g| g.cross_2d(Vec2::Y, 0.5, RED) },
        Row { covers: &["circle", "EllipseBuilder.resolution"], ts: r##"g.circle([0, 0, 0], 1, "#fff").resolution(64)"##, rust: |g| {
                g.circle(Vec3::ZERO, 1.0, Color::WHITE).resolution(64);
            } },
        Row { covers: &["circle2d", "Ellipse2dBuilder.resolution"], ts: r##"g.circle2d([0, 0], 1, "#fff").resolution(7)"##, rust: |g| {
                g.circle_2d(Vec2::ZERO, 1.0, Color::WHITE).resolution(7);
            } },
        Row { covers: &["ellipse"], ts: r##"g.ellipse([0, 0, 0], [2, 1], "#fff")"##, rust: |g| {
                g.ellipse(Vec3::ZERO, Vec2::new(2.0, 1.0), Color::WHITE);
            } },
        Row { covers: &["ellipse2d"], ts: r##"g.ellipse2d([0, 0], [2, 1], "#fff")"##, rust: |g| {
                g.ellipse_2d(Vec2::ZERO, Vec2::new(2.0, 1.0), Color::WHITE);
            } },
        Row { covers: &["sphere", "SphereBuilder.resolution"], ts: r##"g.sphere([0, 0, 0], 1, "#fff").resolution(8)"##, rust: |g| {
                g.sphere(Vec3::ZERO, 1.0, Color::WHITE).resolution(8);
            } },
        Row { covers: &["arc2d", "Arc2dBuilder.resolution"], ts: r##"g.arc2d([0, 0], 1.5, 2, "#fff").resolution(9)"##, rust: |g| {
                g.arc_2d(Vec2::ZERO, 1.5, 2.0, Color::WHITE).resolution(9);
            } },
        Row { covers: &["arc3d", "Arc3dBuilder.resolution"], ts: r##"g.arc3d(1.5, 2, [0, 0, 0], "#fff").resolution(9)"##, rust: |g| {
                g.arc_3d(1.5, 2.0, Vec3::ZERO, Color::WHITE).resolution(9);
            } },
        Row { covers: &["shortArc3dBetween"], ts: r##"g.shortArc3dBetween([0, 0, 0], [1, 0, 0], [0, 1, 0], "#fff")"##, rust: |g| {
                g.short_arc_3d_between(Vec3::ZERO, Vec3::X, Vec3::Y, Color::WHITE);
            } },
        Row { covers: &["longArc3dBetween"], ts: r##"g.longArc3dBetween([0, 0, 0], [1, 0, 0], [0, 1, 0], "#fff")"##, rust: |g| {
                g.long_arc_3d_between(Vec3::ZERO, Vec3::X, Vec3::Y, Color::WHITE);
            } },
        Row { covers: &["shortArc2dBetween"], ts: r##"g.shortArc2dBetween([0, 0], [1, 0], [0, 1], "#fff")"##, rust: |g| {
                g.short_arc_2d_between(Vec2::ZERO, Vec2::X, Vec2::Y, Color::WHITE);
            } },
        Row { covers: &["longArc2dBetween"], ts: r##"g.longArc2dBetween([0, 0], [1, 0], [0, 1], "#fff")"##, rust: |g| {
                g.long_arc_2d_between(Vec2::ZERO, Vec2::X, Vec2::Y, Color::WHITE);
            } },
        Row {
            covers: &["arrow", "ArrowBuilder.withTipLength", "ArrowBuilder.withDoubleEnd"],
            ts: r##"g.arrow([0, 0, 0], [0, 2, 0], "#f00").withTipLength(0.5).withDoubleEnd()"##,
            rust: |g| {
                g.arrow(Vec3::ZERO, Vec3::new(0.0, 2.0, 0.0), RED).with_tip_length(0.5).with_double_end();
            },
        },
        Row { covers: &["arrow2d"], ts: r##"g.arrow2d([0, 0], [0, 2], "#f00")"##, rust: |g| {
                g.arrow_2d(Vec2::ZERO, Vec2::new(0.0, 2.0), RED);
            } },
        Row { covers: &["axes"], ts: r##"g.axes({ translation: [1, 0, 0] }, 2)"##, rust: |g| g.axes(Transform::from_xyz(1.0, 0.0, 0.0), 2.0) },
        Row { covers: &["axes2d"], ts: r##"g.axes2d({ translation: [1, 0, 0] }, 2)"##, rust: |g| g.axes_2d(Transform::from_xyz(1.0, 0.0, 0.0), 2.0) },
        Row {
            covers: &["grid", "GridBuilder2d.skewX", "GridBuilder2d.skewY", "GridBuilder2d.outerEdgesX", "GridBuilder2d.outerEdgesY"],
            ts: r##"g.grid([0, 0, 0], [3, 2], [1, 1], "#888").skewX(0.1).skewY(0.2).outerEdgesX().outerEdgesY()"##,
            rust: |g| {
                g.grid(Vec3::ZERO, UVec2::new(3, 2), Vec2::ONE, Color::srgb_u8(0x88, 0x88, 0x88)).skew_x(0.1).skew_y(0.2).outer_edges_x().outer_edges_y();
            },
        },
        Row {
            covers: &["grid3d", "GridBuilder3d.skewX", "GridBuilder3d.skewY", "GridBuilder3d.skewZ", "GridBuilder3d.outerEdgesX", "GridBuilder3d.outerEdgesY", "GridBuilder3d.outerEdgesZ"],
            ts: r##"g.grid3d([0, 0, 0], [2, 2, 2], [1, 1, 1], "#888").skewX(0.1).skewY(0.2).skewZ(0.3).outerEdgesX().outerEdgesY().outerEdgesZ()"##,
            rust: |g| {
                g.grid_3d(Vec3::ZERO, UVec3::splat(2), Vec3::ONE, Color::srgb_u8(0x88, 0x88, 0x88)).skew_x(0.1).skew_y(0.2).skew_z(0.3).outer_edges_x().outer_edges_y().outer_edges_z();
            },
        },
        Row {
            covers: &["grid3d", "GridBuilder3d.skew", "GridBuilder3d.outerEdges"],
            ts: r##"g.grid3d([0, 0, 0], [2, 1, 2], [1, 1, 1], "#888").skew([0.1, 0.2, 0.3]).outerEdges()"##,
            rust: |g| {
                g.grid_3d(Vec3::ZERO, UVec3::new(2, 1, 2), Vec3::ONE, Color::srgb_u8(0x88, 0x88, 0x88)).skew(Vec3::new(0.1, 0.2, 0.3)).outer_edges();
            },
        },
        Row {
            covers: &["grid2d", "GridBuilder2d.skew", "GridBuilder2d.outerEdges"],
            ts: r##"g.grid2d([0, 0], [3, 2], [1, 1], "#888").skew([0.1, 0.2]).outerEdges()"##,
            rust: |g| {
                g.grid_2d(Vec2::ZERO, UVec2::new(3, 2), Vec2::ONE, Color::srgb_u8(0x88, 0x88, 0x88)).skew(Vec2::new(0.1, 0.2)).outer_edges();
            },
        },
        Row {
            covers: &["roundedRect", "RoundedRectBuilder.cornerRadius", "RoundedRectBuilder.arcResolution"],
            ts: r##"g.roundedRect([0, 0, 0], [2, 1], "#fff").cornerRadius(0.25).arcResolution(4)"##,
            rust: |g| {
                g.rounded_rect(Vec3::ZERO, Vec2::new(2.0, 1.0), Color::WHITE).corner_radius(0.25).arc_resolution(4);
            },
        },
        Row { covers: &["roundedRect2d"], ts: r##"g.roundedRect2d([0, 0], [2, 1], "#fff")"##, rust: |g| {
                g.rounded_rect_2d(Vec2::ZERO, Vec2::new(2.0, 1.0), Color::WHITE);
            } },
        Row {
            covers: &["roundedCuboid", "RoundedCuboidBuilder.edgeRadius", "RoundedCuboidBuilder.arcResolution"],
            ts: r##"g.roundedCuboid([0, 0, 0], [1, 2, 1], "#fff").edgeRadius(0.2).arcResolution(3)"##,
            rust: |g| {
                g.rounded_cuboid(Vec3::ZERO, Vec3::new(1.0, 2.0, 1.0), Color::WHITE).edge_radius(0.2).arc_resolution(3);
            },
        },
        Row { covers: &["text"], ts: r##"g.text([0, 0, 0], "hp 10", 0.5, [0, 0], "#fff")"##, rust: |g| g.text(Vec3::ZERO, "hp 10", 0.5, Vec2::ZERO, Color::WHITE) },
        Row {
            covers: &["textSections"],
            ts: r##"g.textSections([0, 0, 0], [["hp ", "#fff"], ["10", "#f00"]], 0.5, [0.5, 0])"##,
            rust: |g| g.text_sections(Vec3::ZERO, &[("hp ", Color::WHITE), ("10", RED)], 0.5, Vec2::new(0.5, 0.0)),
        },
        Row { covers: &["text2d"], ts: r##"g.text2d([0, 0], "hp 10", 12, [0, 0], "#fff")"##, rust: |g| g.text_2d(Vec2::ZERO, "hp 10", 12.0, Vec2::ZERO, Color::WHITE) },
        Row {
            covers: &["textSections2d"],
            ts: r##"g.textSections2d([0, 0], [["hp ", "#fff"], ["10", "#f00"]], 12, [0, 0])"##,
            rust: |g| g.text_sections_2d(Vec2::ZERO, &[("hp ", Color::WHITE), ("10", RED)], 12.0, Vec2::ZERO),
        },
    ]
}

#[test]
fn ts_draws_what_rust_draws() {
    use bevy_ts_native::gizmos::draw::{Entry, builders};
    use bevy_ts_native::gizmos::primitives::{CURVES, PRIMITIVES_2D, PRIMITIVES_3D};
    let rows = all_rows();
    // Every entry, builder setting, primitive and curve type has a row.
    let covered: std::collections::BTreeSet<&str> = rows.iter().flat_map(|r| r.covers.iter().copied()).collect();
    let mut want: Vec<String> = Entry::ALL.iter().map(|e| e.ts().to_string()).collect();
    for (b, _, settings) in builders() {
        want.extend(settings.iter().map(|(s, _)| format!("{b}.{}", s.ts())));
    }
    want.extend(PRIMITIVES_2D.iter().map(|p| format!("2d:{p}")));
    want.extend(PRIMITIVES_3D.iter().map(|p| format!("3d:{p}")));
    want.extend(CURVES.iter().map(|c| format!("curve:{c}")));
    let missing: Vec<&String> = want.iter().filter(|w| !covered.contains(w.as_str())).collect();
    assert!(missing.is_empty(), "no same-buffer row for {missing:?}");

    let calls: String = rows.iter().map(|r| format!("  {};\n", r.ts)).collect();
    let body = format!(
        "import {{ srgb }} from \"bevy-ts\";\nexport function draw(g: Gizmos, l: Local<{{ n: u32 }}>) {{\n  if (l.n++ > 0) return;\n{calls}}}\napp.addSystems(Update, draw);\n"
    );
    let mut app = gizmo_app("gizmos-same-buffer", &body, |_| {});
    app.update();
    assert_same_as_rows(&drawn::<DefaultGizmoConfigGroup>(&app), &rows);
}

/// Every row's TS calls (`rows`, `primitive_rows`), each checked for a row first.
fn all_rows() -> Vec<Row> {
    rows().into_iter().chain(primitive_rows()).collect()
}

/// `got` holds what the rows' Rust calls draw, vertex for vertex; a difference names its row.
fn assert_same_as_rows(got: &Drawn, rows: &[Row]) {
    let mut buf = Buf::default();
    let mut ends = Vec::new();
    for r in rows {
        (r.rust)(&mut buf);
        ends.push((buf.list_positions.len(), buf.strip_positions.len()));
    }
    // The first row whose vertices differ, by where each row's vertices end.
    let row_of = |k: usize, strip: bool| rows[ends.iter().position(|e| k < if strip { e.1 } else { e.0 }).unwrap_or(rows.len() - 1)].ts;
    let same = |a: Vec3, b: Vec3| (a.is_nan() && b.is_nan()) || a.abs_diff_eq(b, 1e-4);
    let same_c = |a: LinearRgba, b: LinearRgba| a.to_vec4().abs_diff_eq(b.to_vec4(), 1e-4);
    for (k, (g, w)) in got.lines.iter().zip(&buf.list_positions).enumerate() {
        assert!(same(*g, *w), "{}: line vertex {k}: {g} != {w}", row_of(k, false));
    }
    for (k, (g, w)) in got.line_colors.iter().zip(&buf.list_colors).enumerate() {
        assert!(same_c(*g, *w), "{}: line colour {k}: {g:?} != {w:?}", row_of(k, false));
    }
    for (k, (g, w)) in got.strips.iter().zip(&buf.strip_positions).enumerate() {
        assert!(same(*g, *w), "{}: strip vertex {k}: {g} != {w}", row_of(k, true));
    }
    for (k, (g, w)) in got.strip_colors.iter().zip(&buf.strip_colors).enumerate() {
        assert!(same_c(*g, *w) || (g.red.is_nan() && w.red.is_nan()), "{}: strip colour {k}: {g:?} != {w:?}", row_of(k, true));
    }
    assert_eq!((got.lines.len(), got.strips.len()), (buf.list_positions.len(), buf.strip_positions.len()), "vertex counts");
}

/// The `GizmoAsset` a script's ref `n` names, as drawn.
fn asset_drawn(app: &App, n: u64) -> Drawn {
    let handle = app.world().resource::<bevy_ts::reflect_asset::ScriptAssets>().handle(n).unwrap();
    let b = app.world().resource::<Assets<GizmoAsset>>().get(handle.id().typed::<GizmoAsset>()).expect("in Assets<GizmoAsset>").buffer();
    Drawn { lines: b.list_positions.clone(), line_colors: b.list_colors.clone(), strips: b.strip_positions.clone(), strip_colors: b.strip_colors.clone() }
}

const RETAINED: &str = "import { assets, srgb } from \"bevy-ts\";\nimport { GizmoAsset } from \"bevy-ts/gizmos\";\n";

/// Every same-buffer row recorded on a `GizmoAsset` and added: the asset holds what `Gizmos`
/// draws, so the replay covers every entry, setting, primitive and curve.
#[test]
fn a_gizmo_asset_holds_what_gizmos_would_draw() {
    let rows = all_rows();
    let calls: String = rows.iter().map(|r| format!("  {};\n", r.ts)).collect();
    let body = format!(
        "{RETAINED}export function draw(l: Local<{{ n: u32 }}>) {{\n  if (l.n++ > 0) return;\n  const g = new GizmoAsset();\n{calls}  const r = assets.add(GizmoAsset, g);\n  console.log(`ref ${{r.$asset}} ${{g.calls.length}}`);\n}}\napp.addSystems(Update, draw);\n"
    );
    let mut app = gizmo_app("gizmos-asset", &body, |_| {});
    app.update();
    let got = logged(&mut app, "ref ");
    let (n, calls) = got.first().and_then(|l| l.split_once(' ')).unwrap_or_else(|| panic!("{:?}", messages(&app)));
    assert!(calls.parse::<usize>().unwrap() >= rows.len(), "every row recorded: {calls}");
    assert_same_as_rows(&asset_drawn(&app, n.parse().unwrap()), &rows);
    assert_eq!(drawn::<DefaultGizmoConfigGroup>(&app), Drawn::default(), "nothing drawn immediately");
}

/// A builder kept on a `GizmoAsset` past any other drawing call throws at its setting, naming it,
/// as on `Gizmos`; the asset holds the shapes as drawn.
#[test]
fn a_gizmo_asset_builder_kept_past_a_call_throws() {
    let body = format!(
        "{RETAINED}export function draw(l: Local<{{ n: u32 }}>) {{\n  if (l.n++ > 0) return;\n  const g = new GizmoAsset();\n  const c = g.circle([0, 0, 0], 1, \"#fff\");\n  g.line([0, 0, 0], [1, 0, 0], \"#f00\");\n  try {{ c.resolution(64); }} catch (e) {{ console.log(`threw ${{(e as Error).message}}`); }}\n  const a = g.arrow([0, 0, 0], [1, 0, 0], \"#0f0\");\n  g.sphere([0, 0, 0], 1, \"#00f\");\n  try {{ a.withTipLength(0.5); }} catch (e) {{ console.log(`threw ${{(e as Error).message}}`); }}\n  const r = assets.add(GizmoAsset, g);\n  console.log(`ref ${{r.$asset}} ${{g.calls.length}}`);\n}}\napp.addSystems(Update, draw);\n"
    );
    let mut app = gizmo_app("gizmos-asset-kept", &body, |_| {});
    app.update();
    let threw = logged(&mut app, "threw ");
    assert_eq!(
        threw,
        [
            "EllipseBuilder.resolution: call it right after the call that returned the builder",
            "ArrowBuilder.withTipLength: call it right after the call that returned the builder"
        ],
        "{:?}",
        messages(&app)
    );
    let got = logged(&mut app, "ref ");
    let (n, calls) = got.first().and_then(|l| l.split_once(' ')).unwrap_or_else(|| panic!("{:?}", messages(&app)));
    assert_eq!(calls, "4", "the refused settings recorded nothing");
    let mut want = bevy::gizmos::gizmos::GizmoBuffer::<DefaultGizmoConfigGroup, ()>::default();
    want.circle(Vec3::ZERO, 1.0, Color::WHITE);
    want.line(Vec3::ZERO, Vec3::X, RED);
    want.arrow(Vec3::ZERO, Vec3::X, Color::srgb(0.0, 1.0, 0.0));
    want.sphere(Vec3::ZERO, 1.0, Color::srgb(0.0, 0.0, 1.0));
    let d = asset_drawn(&app, n.parse().unwrap());
    assert_eq!(d.lines, want.list_positions, "the lines");
    assert_close(&d.strips, &want.strip_positions);
}

/// `assets.set(ref, g2)` replaces the asset's calls whole; `assets.get` reads the record back.
#[test]
fn set_replaces_a_gizmo_assets_calls() {
    let body = format!(
        "{RETAINED}import type {{ AssetRef }} from \"bevy-ts\";\nimport type {{ GizmoAssetValue }} from \"bevy-ts/gizmos\";\nlet r: AssetRef<GizmoAssetValue> | undefined;\nexport function draw(l: Local<{{ n: u32 }}>) {{\n  const g = new GizmoAsset();\n  if (l.n === 0) {{ g.line([0, 0, 0], [1, 0, 0], \"#f00\"); g.circle([0, 0, 0], 1, \"#f00\").resolution(4); r = assets.add(GizmoAsset, g); console.log(`ref ${{r.$asset}}`); }}\n  if (l.n === 1) {{ g.line([0, 0, 0], [0, 2, 0], \"#00f\"); assets.set(r!, g); console.log(`calls ${{JSON.stringify(assets.get(r!)!.calls.map((c) => (c as unknown[])[0]))}}`); }}\n  l.n++;\n}}\napp.addSystems(Update, draw);\n"
    );
    let mut app = gizmo_app("gizmos-asset-set", &body, |_| {});
    app.update();
    let n: u64 = logged(&mut app, "ref ").first().unwrap_or_else(|| panic!("{:?}", messages(&app))).parse().unwrap();
    let first = Row { covers: &[], ts: "the first record", rust: |g| {
        g.line(Vec3::ZERO, Vec3::X, RED);
        g.circle(Vec3::ZERO, 1.0, RED).resolution(4);
    } };
    assert_same_as_rows(&asset_drawn(&app, n), &[first]);
    app.update();
    assert_eq!(logged(&mut app, "calls "), vec![r#"["line"]"#.to_string()], "{:?}", messages(&app));
    let second = asset_drawn(&app, n);
    assert_eq!(second.lines, vec![Vec3::ZERO, Vec3::new(0.0, 2.0, 0.0)]);
    assert_eq!(second.line_colors, vec![LinearRgba::BLUE; 2]);
    assert!(second.strips.is_empty(), "the circle is gone: {:?}", second.strips);
}

#[test]
fn fixed_update_gizmos_last_until_the_next_fixed_step() {
    use bevy::time::TimeUpdateStrategy;
    use std::time::Duration;
    // A fixed step every 1 s, frames 0.4 s apart: a line drawn in the first fixed step stays
    // drawn every frame until the next step, which draws nothing, as Bevy's fixed context does.
    let body = r##"
import { FixedUpdate } from "bevy-ts";
export function fixed(gizmos: Gizmos, l: Local<{ n: u32 }>) {
  console.log(`fixed ${l.n}`);
  if (l.n++ === 0) gizmos.line([0, 0, 0], [1, 0, 0], "#f00");
}
app.addSystems(FixedUpdate, fixed);
"##;
    let mut app = gizmo_app("gizmos-fixed", body, |app| {
        app.insert_resource(Time::<Fixed>::from_seconds(1.0));
        app.insert_resource(TimeUpdateStrategy::ManualDuration(Duration::from_millis(400)));
    });
    // Each frame's (fixed steps so far, lines drawn); a frame's console output is collected at
    // the next frame's pump, so its step count is read one frame later.
    let mut logged = Vec::new();
    let mut drew = Vec::new();
    for _ in 0..13 {
        app.update();
        logged.push(lines(&app, "fixed ").len());
        drew.push(drawn::<DefaultGizmoConfigGroup>(&app).lines.len());
    }
    let seen: Vec<(usize, usize)> = drew.iter().enumerate().take(12).map(|(k, d)| (logged[k + 1], *d)).collect();
    // Frames after the first step and before the second hold the line; from the second, none.
    let first = seen.iter().position(|s| s.0 == 1).expect("a first fixed step");
    let second = seen.iter().position(|s| s.0 == 2).expect("a second fixed step");
    assert!(second > first + 1, "a frame between the steps: {seen:?}");
    assert!(seen[first..second].iter().all(|s| s.1 == 2), "{seen:?}");
    assert_eq!(seen[second].1, 0, "{seen:?}");
}

#[derive(Default, Reflect)]
pub struct PathGizmos;

impl GizmoConfigGroup for PathGizmos {}

#[test]
fn a_project_group_reaches_ts() {
    use bevy::gizmos::AppGizmoBuilder;
    use bevy_ts_native::BevyTsNativeExt;
    let body = r##"
export function draw(path: Gizmos<PathGizmos>) { path.line([0, 0, 0], [0, 0, 1], "#00f"); }
app.addSystems(Update, draw);
"##;
    let mut app = gizmo_app("gizmos-project-group", body, |app| {
        app.init_gizmo_group::<PathGizmos>().ts_gizmo_group::<PathGizmos>();
    });
    app.update();
    assert_eq!(drawn::<PathGizmos>(&app).lines, vec![Vec3::ZERO, Vec3::Z]);
    assert_eq!(drawn::<DefaultGizmoConfigGroup>(&app), Drawn::default());
    // `bevy-ts types` declares the group and its value.
    let d = bevy_ts_cli::types::emit_modules(app.world());
    assert!(d.contains("export type PathGizmos = \"PathGizmos\";") && d.contains("PathGizmos: "), "{d}");
}

#[derive(Default, Reflect)]
pub struct NeverInit;

impl GizmoConfigGroup for NeverInit {}

#[test]
fn an_unregistered_group_fails_the_app() {
    use bevy_ts_native::BevyTsNativeExt;
    let mut app = App::new();
    app.add_plugins((MinimalPlugins, AssetPlugin::default(), GizmoPlugin));
    app.ts_gizmo_group::<NeverInit>();
    let e = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| app.finish())).unwrap_err();
    let msg = e.downcast_ref::<String>().cloned().unwrap_or_default();
    assert!(msg.contains("ts_gizmo_group exposes it::gizmos::NeverInit to TS, but Bevy has no such gizmo group"), "{msg}");
}

#[test]
fn the_editor_groups_are_not_exposed() {
    let mut app = gizmo_app("gizmos-editor", "export function none() {}\n", |_| {});
    let groups: Vec<&str> = app.world().resource::<bevy_ts_native::gizmos::TsGizmoGroups>().0.iter().map(|g| g.short).collect();
    assert_eq!(groups, ["DefaultGizmoConfigGroup", "AabbGizmoConfigGroup", "FrustumGizmoConfigGroup", "SkinnedMeshBoundsGizmoConfigGroup", "LightGizmoConfigGroup"]);
    // A TS system naming one fails the load, naming it and the claimed spellings.
    let dir = project("gizmos-editor-load", &[("game.ts", &format!("{HEAD}export function edit(g: Gizmos<EditorGizmos>) {{}}\napp.addSystems(Update, edit);\n"))]);
    app.insert_resource(bevy_ts::generation::DevSource(std::sync::Arc::new(bevy_ts_cli::compile::Compiler::new(dir.join("game.ts"), true).unwrap())));
    let r = reload(&mut app, false);
    let errors = format!("{:?}", r.errors);
    assert!(errors.contains("Gizmos<EditorGizmos> is not a gizmo group this engine exports; claimed Gizmos spellings:"), "{errors}");
}

/// Headless apps with Bevy's whole default set and a camera drawing to an offscreen image: the
/// markers' gizmos need computed bounds and frusta, and the render test needs pixels.
#[cfg(target_os = "macos")]
mod rendered {
    use super::*;
    use bevy::camera::RenderTarget;
    use bevy::render::render_resource::TextureFormat;
    use bevy::render::view::screenshot::{Screenshot, ScreenshotCaptured};
    use bevy::window::ExitCondition;
    use bevy::winit::WinitPlugin;
    use bevy_ts::generation::{DevSource, ReloadStatus};
    use bevy_ts_cli::compile::Compiler;
    use bevy_ts_v8::V8Options;
    use std::sync::{Arc, Mutex};

    pub const W: u32 = 160;
    pub const H: u32 = 120;

    #[derive(Resource, Clone)]
    struct Target(Handle<Image>);

    pub fn render_app(name: &str, src: &str) -> App {
        let dir = project(name, &[("game.tsx", src)]);
        let mut app = App::new();
        app.add_plugins(
            DefaultPlugins
                .build()
                .disable::<WinitPlugin>()
                .set(WindowPlugin { primary_window: None, exit_condition: ExitCondition::DontExit, ..default() }),
        )
        .add_plugins(bevy_ts::BevyTsPlugin::new(bevy_ts_v8::factory(V8Options::default())).dev(true))
        .add_plugins(bevy_ts_tsx::TsxPlugin)
        .add_plugins(bevy_ts_cli::gizmos::GizmosPlugin)
        .insert_resource(ClearColor(Color::BLACK))
        .insert_resource(DevSource(Arc::new(Compiler::new(dir.join("game.tsx"), true).unwrap())));
        while app.plugins_state() == bevy::app::PluginsState::Adding {
            bevy::tasks::tick_global_task_pools_on_main_thread();
        }
        app.finish();
        app.cleanup();
        let mut image = Image::new_target_texture(W, H, TextureFormat::Rgba8UnormSrgb, None);
        image.asset_usage = bevy::asset::RenderAssetUsages::default();
        let target = app.world_mut().resource_mut::<Assets<Image>>().add(image);
        app.insert_resource(Target(target));
        app.add_observer(|add: On<Add<Camera>>, target: Res<Target>, mut commands: Commands| {
            commands.entity(add.entity).insert(RenderTarget::Image(target.0.clone().into()));
        });
        let r = reload(&mut app, false);
        assert_eq!(r.status, ReloadStatus::Applied, "{:?}", r.errors);
        app
    }

    /// Draws until `done` accepts a picture; panics after many frames.
    pub fn picture(app: &mut App, done: impl Fn(&Image) -> bool) -> Image {
        let target = app.world().resource::<Target>().0.clone();
        let shot: Arc<Mutex<Option<Image>>> = Arc::default();
        for frame in 0..900 {
            app.update();
            if frame % 10 == 5 {
                let s = shot.clone();
                app.world_mut().spawn(Screenshot::image(target.clone())).observe(move |e: On<ScreenshotCaptured>| {
                    *s.lock().unwrap() = Some(e.image.clone());
                });
            }
            if let Some(img) = shot.lock().unwrap().take()
                && done(&img)
            {
                return img;
            }
        }
        panic!("the picture never settled");
    }

    pub fn px(img: &Image, x: u32, y: u32) -> [u8; 3] {
        let i = ((y * W + x) * 4) as usize;
        let d = img.data.as_ref().unwrap();
        [d[i], d[i + 1], d[i + 2]]
    }

    /// Frames until group `C` has drawn something, then what it drew.
    pub fn until_drawn<C: GizmoConfigGroup>(app: &mut App) -> Drawn {
        for _ in 0..300 {
            app.update();
            let d = drawn::<C>(app);
            if d != Drawn::default() {
                return d;
            }
        }
        panic!("{} drew nothing", C::short_type_path());
    }
}

#[cfg(target_os = "macos")]
const CAMERA_2D: &str = r##"import { app, Update } from "bevy-ts";
import type { Gizmos } from "bevy-ts/gizmos";
import { Camera2d, Tonemapping } from "bevy-ts/bevy";
"##;

#[cfg(target_os = "macos")]
#[test]
fn gizmos_render_headless() {
    use rendered::*;
    let src = format!(
        "{CAMERA_2D}export function draw(g: Gizmos) {{ g.line2d([-100, 0], [100, 0], \"#f00\"); }}\napp.addSystems(Update, draw);\napp.scene(\"s\", () => <entity name=\"cam\"><Camera2d /><Tonemapping value=\"None\" /></entity>);\n"
    );
    let mut app = render_app("gizmos-render", &src);
    let red = |p: [u8; 3]| p[0] > 128 && p[1] < 64 && p[2] < 64;
    let img = picture(&mut app, |i| red(px(i, W / 2, H / 2)));
    for x in [10, W / 4, W / 2, 3 * W / 4, W - 10] {
        assert!(red(px(&img, x, H / 2)) || red(px(&img, x, H / 2 - 1)), "red on the line at x {x}: {:?}", px(&img, x, H / 2));
        assert_eq!(px(&img, x, H / 2 + 6), [0, 0, 0], "nothing a few pixels off at x {x}");
    }
}

/// A width and style written through `ResMut<GizmoConfigStore>` reach the pixels: a 10 px dashed line
/// is about 10 px thick where drawn and has gaps along it; the default line is 2 px and solid.
#[cfg(target_os = "macos")]
#[test]
fn line_width_and_style_render() {
    use rendered::*;
    let src = format!(
        "{CAMERA_2D}import type {{ ResMut }} from \"bevy-ts\";\nimport {{ DefaultGizmoConfigGroup, GizmoConfigStore }} from \"bevy-ts/gizmos\";\nexport function style(store: ResMut<GizmoConfigStore>) {{ store.insert(DefaultGizmoConfigGroup, {{ line: {{ width: 10, style: {{ Dashed: {{ gap_scale: 2, line_scale: 2 }} }} }} }}, {{}}); }}\nexport function draw(g: Gizmos) {{ g.line2d([-100, 0], [100, 0], \"#f00\"); }}\napp.addSystems(Update, style).addSystems(Update, draw);\napp.scene(\"s\", () => <entity name=\"cam\"><Camera2d /><Tonemapping value=\"None\" /></entity>);\n"
    );
    let mut app = render_app("gizmos-line-style", &src);
    let red = |p: [u8; 3]| p[0] > 128 && p[1] < 64 && p[2] < 64;
    let thickness = |img: &Image, x: u32| (0..H).filter(|&y| red(px(img, x, y))).count();
    let img = picture(&mut app, |i| (W / 4..3 * W / 4).any(|x| thickness(i, x) >= 8));
    let thick = (W / 4..3 * W / 4).map(|x| thickness(&img, x)).max().unwrap();
    assert!((8..=14).contains(&thick), "a 10 px line, {thick} px thick");
    let gaps = (W / 4..3 * W / 4).filter(|&x| thickness(&img, x) == 0).count();
    assert!(gaps > 10, "a dashed line has gaps along it: {gaps} empty columns");
}

/// A `GizmoAsset` added at load and shown by `<Gizmo handle={ref} line_config={{ width: 4 }} />`
/// reaches the pixels: a red line about 4 px thick, drawn with no `Gizmos` call.
#[cfg(target_os = "macos")]
#[test]
fn a_retained_gizmo_renders() {
    use rendered::*;
    let src = format!(
        "{CAMERA_2D}import {{ assets }} from \"bevy-ts\";\nimport {{ Gizmo }} from \"bevy-ts/bevy\";\nimport {{ GizmoAsset }} from \"bevy-ts/gizmos\";\nconst g = new GizmoAsset();\ng.line2d([-100, 0], [100, 0], \"#f00\");\nconst line = assets.add(GizmoAsset, g);\napp.scene(\"s\", () => <>\n  <entity name=\"cam\"><Camera2d /><Tonemapping value=\"None\" /></entity>\n  <entity name=\"line\"><Gizmo handle={{line}} line_config={{{{ width: 4 }}}} /></entity>\n</>);\n"
    );
    let mut app = render_app("gizmos-retained-render", &src);
    let red = |p: [u8; 3]| p[0] > 128 && p[1] < 64 && p[2] < 64;
    let thickness = |img: &Image, x: u32| (0..H).filter(|&y| red(px(img, x, y))).count();
    let img = picture(&mut app, |i| red(px(i, W / 2, H / 2)) || red(px(i, W / 2, H / 2 - 1)));
    for x in [10, W / 4, W / 2, 3 * W / 4, W - 10] {
        assert!((3..=6).contains(&thickness(&img, x)), "a 4 px line at x {x}: {} px", thickness(&img, x));
    }
    assert_eq!(px(&img, W / 2, H / 2 + 10), [0, 0, 0], "nothing well off the line");
}

#[cfg(target_os = "macos")]
const SCENE_HEAD: &str = r##"import { app } from "bevy-ts";
import { Camera3d, PointLight, ShowAabbGizmo, ShowFrustumGizmo, ShowLightGizmo, Transform } from "bevy-ts/bevy";
"##;

#[cfg(target_os = "macos")]
#[test]
fn show_aabb_gizmo_draws_its_box() {
    let src = format!(
        "{SCENE_HEAD}app.scene(\"s\", () => <><entity name=\"cam\"><Camera3d /><Transform translation={{[0, 0, 6]}} /></entity><entity name=\"box\"><Transform translation={{[0, 0, 0]}} /><mesh shape=\"cuboid\" size={{[2, 2, 2]}} /><ShowAabbGizmo color=\"#f00\" /></entity></>);\n"
    );
    let mut app = rendered::render_app("gizmos-show-aabb", &src);
    // A box is 12 edges, 24 line points.
    let d = rendered::until_drawn::<bevy::gizmos::aabb::AabbGizmoConfigGroup>(&mut app);
    assert!(d.lines.len() + d.strips.len() >= 16, "{d:?}");
}

#[cfg(target_os = "macos")]
#[test]
fn show_frustum_gizmo_draws() {
    let src = format!("{SCENE_HEAD}app.scene(\"s\", () => <entity name=\"cam\"><Camera3d /><ShowFrustumGizmo /></entity>);\n");
    let mut app = rendered::render_app("gizmos-show-frustum", &src);
    let d = rendered::until_drawn::<bevy::gizmos::frustum::FrustumGizmoConfigGroup>(&mut app);
    assert!(!d.lines.is_empty() || !d.strips.is_empty());
}

#[cfg(target_os = "macos")]
#[test]
fn show_light_gizmo_draws() {
    let src = format!("{SCENE_HEAD}app.scene(\"s\", () => <entity name=\"lamp\"><PointLight /><Transform translation={{[0, 2, 0]}} /><ShowLightGizmo /></entity>);\n");
    let mut app = rendered::render_app("gizmos-show-light", &src);
    let d = rendered::until_drawn::<LightGizmoConfigGroup>(&mut app);
    assert!(!d.lines.is_empty() || !d.strips.is_empty());
}

#[cfg(target_os = "macos")]
#[test]
fn draw_all_draws_every_aabb() {
    use bevy::gizmos::aabb::AabbGizmoConfigGroup;
    use bevy::gizmos::config::GizmoConfigStore;
    // Bevy draws the box of a visible entity only, so the scene has a camera.
    let boxes = "<entity name=\"cam\"><Camera3d /><Transform translation={[0, 0, 6]} /></entity><entity name=\"a\"><Transform translation={[-2, 0, 0]} /><mesh shape=\"cuboid\" size={[1, 1, 1]} /></entity><entity name=\"b\"><Transform translation={[2, 0, 0]} /><mesh shape=\"cuboid\" size={[1, 1, 1]} /></entity>";
    let src = format!("{SCENE_HEAD}app.scene(\"s\", () => <>{boxes}</>);\n");
    let mut app = rendered::render_app("gizmos-draw-all", &src);
    app.world_mut().resource_mut::<GizmoConfigStore>().config_mut::<AabbGizmoConfigGroup>().1.draw_all = true;
    // Two boxes, no marker on either: both drawn.
    let d = rendered::until_drawn::<AabbGizmoConfigGroup>(&mut app);
    let one = {
        let mut b = bevy::gizmos::gizmos::GizmoBuffer::<AabbGizmoConfigGroup, ()>::default();
        b.cube(Transform::IDENTITY, Color::WHITE);
        b.list_positions.len() + b.strip_positions.len()
    };
    assert_eq!(d.lines.len() + d.strips.len(), 2 * one, "{d:?}");
}

/// Rows for every primitive (each its default unless a field matters), each primitive builder
/// setting and each curve type.
fn primitive_rows() -> Vec<Row> {
    use bevy::curve::{ConstantCurve, EaseFunction, EasingCurve, Interval, SampleAutoCurve, UnevenSampleAutoCurve};
    use bevy::math::{Dir2, Dir3};
    use bevy::shape::*;
    const W: Color = Color::WHITE;
    vec![
        Row { covers: &["primitive2d", "2d:Dir2"], ts: r##"g.primitive2d({ Dir2: [0, 1] }, [0, 0], "#fff")"##, rust: |g| g.primitive_2d(&Dir2::Y, Vec2::ZERO, W) },
        Row { covers: &["2d:Arc2d"], ts: r##"g.primitive2d({ Arc2d: { radius: 2, half_angle: 0.5 } }, [0, 0], "#fff")"##, rust: |g| g.primitive_2d(&Arc2d::new(2.0, 0.5), Vec2::ZERO, W) },
        Row { covers: &["2d:Circle"], ts: r##"g.primitive2d({ Circle: { radius: 2 } }, [0, 0], "#fff")"##, rust: |g| { g.primitive_2d(&Circle::new(2.0), Vec2::ZERO, W); } },
        Row { covers: &["2d:CircularSector"], ts: r##"g.primitive2d({ CircularSector: {} }, [0, 0], "#fff")"##, rust: |g| g.primitive_2d(&CircularSector::default(), Vec2::ZERO, W) },
        Row { covers: &["2d:CircularSegment"], ts: r##"g.primitive2d({ CircularSegment: {} }, [0, 0], "#fff")"##, rust: |g| g.primitive_2d(&CircularSegment::default(), Vec2::ZERO, W) },
        Row { covers: &["2d:Ellipse"], ts: r##"g.primitive2d({ Ellipse: { half_size: [2, 1] } }, [0, 0], "#fff")"##, rust: |g| { g.primitive_2d(&Ellipse::new(2.0, 1.0), Vec2::ZERO, W); } },
        Row {
            covers: &["2d:Annulus", "Annulus2dBuilder.resolution", "Annulus2dBuilder.outerResolution", "Annulus2dBuilder.innerResolution"],
            ts: r##"g.primitive2d({ Annulus: {} }, [0, 0], "#fff").resolution(12).outerResolution(10).innerResolution(6)"##,
            rust: |g| {
                g.primitive_2d(&Annulus::default(), Vec2::ZERO, W).resolution(12).outer_resolution(10).inner_resolution(6);
            },
        },
        Row { covers: &["2d:Rhombus"], ts: r##"g.primitive2d({ Rhombus: {} }, [0, 0], "#fff")"##, rust: |g| g.primitive_2d(&Rhombus::default(), Vec2::ZERO, W) },
        Row { covers: &["2d:Capsule2d"], ts: r##"g.primitive2d({ Capsule2d: {} }, [0, 0], "#fff")"##, rust: |g| g.primitive_2d(&Capsule2d::default(), Vec2::ZERO, W) },
        Row {
            covers: &["2d:Line2d", "Line2dBuilder.drawArrow"],
            ts: r##"g.primitive2d({ Line2d: { direction: [0, 1] } }, [0, 0], "#fff").drawArrow(true)"##,
            rust: |g| {
                g.primitive_2d(&Line2d { direction: Dir2::Y }, Vec2::ZERO, W).draw_arrow(true);
            },
        },
        Row { covers: &["2d:Plane2d"], ts: r##"g.primitive2d({ Plane2d: {} }, [0, 0], "#fff")"##, rust: |g| g.primitive_2d(&Plane2d::default(), Vec2::ZERO, W) },
        Row {
            covers: &["2d:Segment2d", "Segment2dBuilder.drawArrow"],
            ts: r##"g.primitive2d({ Segment2d: {} }, [0, 0], "#fff").drawArrow(true)"##,
            rust: |g| {
                g.primitive_2d(&Segment2d::default(), Vec2::ZERO, W).draw_arrow(true);
            },
        },
        Row {
            covers: &["2d:Polyline2d"],
            ts: r##"g.primitive2d({ Polyline2d: { vertices: [[0, 0], [1, 0], [1, 1]] } }, [0, 0], "#fff")"##,
            rust: |g| g.primitive_2d(&Polyline2d::new([Vec2::ZERO, Vec2::X, Vec2::ONE]), Vec2::ZERO, W),
        },
        Row { covers: &["2d:Triangle2d"], ts: r##"g.primitive2d({ Triangle2d: {} }, [0, 0], "#fff")"##, rust: |g| g.primitive_2d(&Triangle2d::default(), Vec2::ZERO, W) },
        Row { covers: &["2d:Rectangle"], ts: r##"g.primitive2d({ Rectangle: { half_size: [2, 1] } }, [1, 0], "#fff")"##, rust: |g| g.primitive_2d(&Rectangle::new(4.0, 2.0), Vec2::X, W) },
        Row {
            covers: &["2d:Polygon"],
            ts: r##"g.primitive2d({ Polygon: { vertices: [[0, 0], [1, 0], [0, 1]] } }, [0, 0], "#fff")"##,
            rust: |g| g.primitive_2d(&Polygon::new([Vec2::ZERO, Vec2::X, Vec2::Y]), Vec2::ZERO, W),
        },
        Row { covers: &["2d:RegularPolygon"], ts: r##"g.primitive2d({ RegularPolygon: { sides: 5 } }, [0, 0], "#fff")"##, rust: |g| g.primitive_2d(&RegularPolygon::new(0.5, 5), Vec2::ZERO, W) },
        Row { covers: &["primitive3d", "3d:Dir3"], ts: r##"g.primitive3d({ Dir3: [0, 1, 0] }, [0, 0, 0], "#fff")"##, rust: |g| g.primitive_3d(&Dir3::Y, Vec3::ZERO, W) },
        Row { covers: &["3d:Sphere"], ts: r##"g.primitive3d({ Sphere: { radius: 2 } }, [0, 0, 0], "#fff").resolution(10)"##, rust: |g| { g.primitive_3d(&Sphere::new(2.0), Vec3::ZERO, W).resolution(10); } },
        Row {
            covers: &["3d:Plane3d", "Plane3dBuilder.cellCount", "Plane3dBuilder.spacing"],
            ts: r##"g.primitive3d({ Plane3d: {} }, [0, 0, 0], "#fff").cellCount([3, 2]).spacing([0.5, 0.5])"##,
            rust: |g| {
                g.primitive_3d(&Plane3d::default(), Vec3::ZERO, W).cell_count(UVec2::new(3, 2)).spacing(Vec2::splat(0.5));
            },
        },
        Row { covers: &["3d:Line3d"], ts: r##"g.primitive3d({ Line3d: { direction: [0, 1, 0] } }, [0, 0, 0], "#fff")"##, rust: |g| g.primitive_3d(&Line3d { direction: Dir3::Y }, Vec3::ZERO, W) },
        Row { covers: &["3d:Segment3d"], ts: r##"g.primitive3d({ Segment3d: {} }, [0, 0, 0], "#fff")"##, rust: |g| g.primitive_3d(&Segment3d::default(), Vec3::ZERO, W) },
        Row {
            covers: &["3d:Polyline3d"],
            ts: r##"g.primitive3d({ Polyline3d: { vertices: [[0, 0, 0], [1, 0, 0], [1, 1, 0]] } }, [0, 0, 0], "#fff")"##,
            rust: |g| g.primitive_3d(&Polyline3d::new([Vec3::ZERO, Vec3::X, Vec3::new(1.0, 1.0, 0.0)]), Vec3::ZERO, W),
        },
        Row { covers: &["3d:Triangle3d"], ts: r##"g.primitive3d({ Triangle3d: {} }, [0, 0, 0], "#fff")"##, rust: |g| g.primitive_3d(&Triangle3d::default(), Vec3::ZERO, W) },
        Row { covers: &["3d:Cuboid"], ts: r##"g.primitive3d({ Cuboid: { half_size: [1, 1, 1] } }, [0, 1, 0], "#ff0")"##, rust: |g| g.primitive_3d(&Cuboid::new(2.0, 2.0, 2.0), Vec3::Y, Color::srgb(1.0, 1.0, 0.0)) },
        Row {
            covers: &["3d:Cylinder", "Cylinder3dBuilder.resolution"],
            ts: r##"g.primitive3d({ Cylinder: {} }, [0, 0, 0], "#fff").resolution(9)"##,
            rust: |g| {
                g.primitive_3d(&Cylinder::default(), Vec3::ZERO, W).resolution(9);
            },
        },
        Row {
            covers: &["3d:Capsule3d", "Capsule3dBuilder.resolution"],
            ts: r##"g.primitive3d({ Capsule3d: {} }, [0, 0, 0], "#fff").resolution(9)"##,
            rust: |g| {
                g.primitive_3d(&Capsule3d::default(), Vec3::ZERO, W).resolution(9);
            },
        },
        Row {
            covers: &["3d:Cone", "Cone3dBuilder.resolution", "Cone3dBuilder.baseResolution", "Cone3dBuilder.heightResolution"],
            ts: r##"g.primitive3d({ Cone: {} }, [0, 0, 0], "#fff").resolution(9).baseResolution(7).heightResolution(5)"##,
            rust: |g| {
                g.primitive_3d(&Cone::default(), Vec3::ZERO, W).resolution(9).base_resolution(7).height_resolution(5);
            },
        },
        Row {
            covers: &["3d:ConicalFrustum", "ConicalFrustum3dBuilder.resolution"],
            ts: r##"g.primitive3d({ ConicalFrustum: {} }, [0, 0, 0], "#fff").resolution(9)"##,
            rust: |g| {
                g.primitive_3d(&ConicalFrustum::default(), Vec3::ZERO, W).resolution(9);
            },
        },
        Row {
            covers: &["3d:Torus", "Torus3dBuilder.minorResolution", "Torus3dBuilder.majorResolution"],
            ts: r##"g.primitive3d({ Torus: {} }, [0, 0, 0], "#fff").minorResolution(6).majorResolution(10)"##,
            rust: |g| {
                g.primitive_3d(&Torus::default(), Vec3::ZERO, W).minor_resolution(6).major_resolution(10);
            },
        },
        Row { covers: &["3d:Tetrahedron"], ts: r##"g.primitive3d({ Tetrahedron: {} }, [0, 0, 0], "#fff")"##, rust: |g| g.primitive_3d(&Tetrahedron::default(), Vec3::ZERO, W) },
        Row {
            covers: &["curve3d", "curve:EasingCurve"],
            ts: r##"g.curve3d({ EasingCurve: { start: [0, 0, 0], end: [1, 1, 0], ease_fn: "CubicInOut" } }, Array.from({ length: 20 }, (_, i) => i / 19), "#fff")"##,
            rust: |g| g.curve_3d(EasingCurve::new(Vec3::ZERO, Vec3::new(1.0, 1.0, 0.0), EaseFunction::CubicInOut), (0..20).map(|i| i as f32 / 19.0), W),
        },
        Row {
            covers: &["curve2d", "curve:SampleAutoCurve"],
            ts: r##"g.curve2d({ SampleAutoCurve: { core: { domain: { start: 0, end: 1 }, samples: [[0, 0], [1, 0], [1, 1]] } } }, new Float32Array([0, 0.25, 0.5, 0.75, 1]), "#fff")"##,
            rust: |g| g.curve_2d(SampleAutoCurve::new(Interval::UNIT, [Vec2::ZERO, Vec2::X, Vec2::ONE]).unwrap(), [0.0, 0.25, 0.5, 0.75, 1.0], W),
        },
        Row {
            covers: &["curveGradient3d", "curve:UnevenSampleAutoCurve"],
            ts: r##"g.curveGradient3d({ UnevenSampleAutoCurve: { core: { times: [0, 0.5, 1], samples: [[0, 0, 0], [1, 0, 0], [1, 1, 0]] } } }, [[0, "#f00"], [0.5, "#0f0"], [1, "#00f"]])"##,
            rust: |g| {
                let c = UnevenSampleAutoCurve::new([(0.0, Vec3::ZERO), (0.5, Vec3::X), (1.0, Vec3::new(1.0, 1.0, 0.0))]).unwrap();
                g.curve_gradient_3d(c, [(0.0, RED), (0.5, GREEN), (1.0, BLUE)]);
            },
        },
        Row {
            covers: &["curveGradient2d", "curve:ConstantCurve"],
            ts: r##"g.curveGradient2d({ ConstantCurve: { domain: { start: 0, end: 1 }, value: [1, 2] } }, [[0, "#f00"], [1, "#00f"]])"##,
            rust: |g| g.curve_gradient_2d(ConstantCurve::new(Interval::UNIT, Vec2::new(1.0, 2.0)), [(0.0, RED), (1.0, BLUE)]),
        },
    ]
}

#[test]
fn an_unknown_primitive_throws_naming_the_set() {
    let body = r##"
export function draw(g: Gizmos, l: Local<{ n: u32 }>) {
  if (l.n++ > 0) return;
  try { g.primitive3d({ Blob: {} } as any, [0, 0, 0], "#fff"); } catch (e) { console.log(`threw ${(e as Error).message}`); }
  try { g.primitive3d({ Cuboid: {} }, [0, 0, 0], "#fff").resolution(3); } catch (e) { console.log(`threw ${(e as Error).message}`); }
}
app.addSystems(Update, draw);
"##;
    let mut app = gizmo_app("gizmos-unknown-primitive", body, |_| {});
    app.update();
    let drew = drawn::<DefaultGizmoConfigGroup>(&app).lines;
    let threw = logged(&mut app, "threw ");
    assert!(threw[0].contains("primitive3d: Blob is not one of Dir3, Sphere, Plane3d"), "{threw:?}");
    assert!(threw[1].contains("Cuboid has no setting resolution"), "{threw:?}");
    // The cuboid stays drawn as it was.
    let mut want = Buf::default();
    want.primitive_3d(&bevy::shape::Cuboid::default(), Vec3::ZERO, Color::WHITE);
    assert_eq!(drew, want.list_positions);
}

#[test]
fn a_curve_outside_the_set_throws_naming_it() {
    let body = r##"
export function draw(g: Gizmos, l: Local<{ n: u32 }>) {
  if (l.n++ > 0) return;
  try { g.curve3d({ FunctionCurve: {} } as any, [0, 1], "#fff"); } catch (e) { console.log(`threw ${(e as Error).message}`); }
}
app.addSystems(Update, draw);
"##;
    let mut app = gizmo_app("gizmos-unknown-curve", body, |_| {});
    app.update();
    let threw = logged(&mut app, "threw ");
    assert!(threw[0].contains("curve3d: FunctionCurve is not one of EasingCurve, SampleAutoCurve, UnevenSampleAutoCurve, ConstantCurve"), "{threw:?}");
}

const STORE_HEAD: &str = "import type { Res, ResMut } from \"bevy-ts\";\nimport { AabbGizmoConfigGroup, DefaultGizmoConfigGroup, FrustumGizmoConfigGroup, GizmoConfigStore, LightGizmoConfigGroup, SkinnedMeshBoundsGizmoConfigGroup } from \"bevy-ts/gizmos\";\n";

#[test]
fn config_round_trips_through_the_store() {
    use bevy::camera::visibility::RenderLayers;
    use bevy::gizmos::aabb::AabbGizmoConfigGroup;
    use bevy::gizmos::config::{GizmoConfigStore, GizmoLineJoint, GizmoLineStyle};
    use bevy::gizmos::frustum::FrustumGizmoConfigGroup;
    use bevy::gizmos::skinned_mesh_bounds::SkinnedMeshBoundsGizmoConfigGroup;
    use bevy::light::gizmos::LightGizmoColor;
    let body = format!(
        "{STORE_HEAD}{}",
        r##"
export function write(store: ResMut<GizmoConfigStore>, l: Local<{ n: u32 }>) {
  if (l.n++ > 0) return;
  // Every GizmoConfig field, written to the default group.
  store.insert(DefaultGizmoConfigGroup, { enabled: true, line: { width: 4, perspective: true, style: { Dashed: { gap_scale: 2, line_scale: 1 } }, joints: { Round: 4 }, animation_offset: 0.5 }, depth_bias: -0.5 }, {});
  // render_layers as reflection read it from the light group, written to the default one.
  const [light] = store.config(LightGizmoConfigGroup);
  store.insert(DefaultGizmoConfigGroup, { render_layers: light.render_layers }, {});
  // Each group's own fields.
  store.insert(AabbGizmoConfigGroup, {}, { draw_all: true, default_color: "#f00" });
  store.insert(FrustumGizmoConfigGroup, {}, { draw_all: true, default_color: null });
  store.insert(SkinnedMeshBoundsGizmoConfigGroup, {}, { draw_all: true, default_color: "#0f0" });
  store.insert(LightGizmoConfigGroup, { line: { style: "Dotted", joints: "Bevel" } }, { draw_all: true, color: { Manual: "#00f" }, point_light_color: "#f00", spot_light_color: "#0f0", directional_light_color: "#00f", rect_light_color: "#fff" });
  const [config, ext] = store.config(DefaultGizmoConfigGroup);
  console.log(`read ${JSON.stringify(config.line)} ${JSON.stringify(ext)} ${JSON.stringify(store.groups())}`);
}
app.addSystems(Update, write);
"##
    );
    let mut app = gizmo_app("gizmos-store", &body, |app| {
        app.world_mut().resource_mut::<GizmoConfigStore>().config_mut::<LightGizmoConfigGroup>().0.render_layers = RenderLayers::from_layers(&[1, 3]);
    });
    app.update();
    let store = app.world().resource::<GizmoConfigStore>();
    let (c, _) = store.config::<DefaultGizmoConfigGroup>();
    assert!(c.enabled && c.line.width == 4.0 && c.line.perspective && c.line.animation_offset == 0.5 && c.depth_bias == -0.5, "{c:?}");
    assert_eq!(c.line.style, GizmoLineStyle::Dashed { gap_scale: 2.0, line_scale: 1.0 });
    assert_eq!(c.line.joints, GizmoLineJoint::Round(4));
    assert_eq!(c.render_layers, RenderLayers::from_layers(&[1, 3]));
    let (_, aabb) = store.config::<AabbGizmoConfigGroup>();
    assert!(aabb.draw_all && aabb.default_color == Some(Color::srgb(1.0, 0.0, 0.0)));
    let (_, frustum) = store.config::<FrustumGizmoConfigGroup>();
    assert!(frustum.draw_all && frustum.default_color.is_none());
    let (_, skinned) = store.config::<SkinnedMeshBoundsGizmoConfigGroup>();
    assert!(skinned.draw_all && skinned.default_color == Color::srgb(0.0, 1.0, 0.0));
    let (lc, light) = store.config::<LightGizmoConfigGroup>();
    assert_eq!((lc.line.style, lc.line.joints), (GizmoLineStyle::Dotted, GizmoLineJoint::Bevel));
    assert!(light.draw_all && matches!(light.color, LightGizmoColor::Manual(c) if c == Color::srgb(0.0, 0.0, 1.0)));
    assert_eq!((light.point_light_color, light.rect_light_color), (Color::srgb(1.0, 0.0, 0.0), Color::srgb(1.0, 1.0, 1.0)));
    let read = &logged(&mut app, "read ")[0];
    assert!(read.contains("\"width\":4") && read.contains("\"DefaultGizmoConfigGroup\"") && read.contains("\"LightGizmoConfigGroup\""), "{read}");
}

#[test]
fn insert_merges_over_the_current_config() {
    use bevy::gizmos::config::GizmoConfigStore;
    let body = format!(
        "{STORE_HEAD}{}",
        r##"
export function write(store: ResMut<GizmoConfigStore>, l: Local<{ n: u32 }>) {
  if (l.n++ > 0) return;
  store.insert(DefaultGizmoConfigGroup, { line: { width: 4, style: { Dashed: { gap_scale: 2, line_scale: 1 } } } }, {});
  store.insert(DefaultGizmoConfigGroup, { depth_bias: -1 }, {});
}
app.addSystems(Update, write);
"##
    );
    let mut app = gizmo_app("gizmos-store-merge", &body, |app| {
        app.world_mut().resource_mut::<GizmoConfigStore>().config_mut::<DefaultGizmoConfigGroup>().0.line.perspective = true;
    });
    app.update();
    let (c, _) = app.world().resource::<GizmoConfigStore>().config::<DefaultGizmoConfigGroup>();
    // Keys left out keep their values: the width and style from the first insert, perspective
    // from Rust.
    assert!(c.line.width == 4.0 && c.depth_bias == -1.0 && c.line.perspective, "{c:?}");
    assert!(matches!(c.line.style, bevy::gizmos::config::GizmoLineStyle::Dashed { .. }));
}

#[test]
fn a_bad_field_throws_and_changes_nothing() {
    use bevy::gizmos::config::GizmoConfigStore;
    let body = format!(
        "{STORE_HEAD}{}",
        r##"
export function write(store: ResMut<GizmoConfigStore>, l: Local<{ n: u32 }>) {
  if (l.n++ > 0) return;
  try { store.insert(LightGizmoConfigGroup, { line: { width: 9 } }, { draw_al: true } as any); } catch (e) { console.log(`threw ${(e as Error).message}`); }
  try { store.insert("Nope" as any, {}, {}); } catch (e) { console.log(`threw ${(e as Error).message}`); }
}
app.addSystems(Update, write);
"##
    );
    let mut app = gizmo_app("gizmos-store-bad", &body, |_| {});
    app.update();
    let threw = logged(&mut app, "threw ");
    assert!(threw[0].contains("LightGizmoConfigGroup ext") && threw[0].contains("draw_al"), "{threw:?}");
    assert!(threw[1].contains("Nope is not a gizmo group exposed to TS; the groups are DefaultGizmoConfigGroup"), "{threw:?}");
    let (c, ext) = app.world().resource::<GizmoConfigStore>().config::<LightGizmoConfigGroup>();
    assert_eq!(c.line.width, 2.0, "the config was not written either");
    assert!(!ext.draw_all);
}

#[test]
fn disabled_group_draws_nothing() {
    let body = format!(
        "{STORE_HEAD}{}",
        r##"
export function off(store: ResMut<GizmoConfigStore>) { store.insert(DefaultGizmoConfigGroup, { enabled: false }, {}); }
export function draw(g: Gizmos) { g.line([0, 0, 0], [1, 0, 0], "#f00"); }
app.addSystems(Update, off).addSystems(PostUpdate, draw);
"##
    );
    let mut app = gizmo_app("gizmos-disabled", &body, |_| {});
    app.update();
    app.update();
    assert_eq!(drawn::<DefaultGizmoConfigGroup>(&app), Drawn::default());
}

#[test]
fn res_store_cannot_insert() {
    let body = format!(
        "{STORE_HEAD}{}",
        r##"
export function read(store: Res<GizmoConfigStore>, l: Local<{ n: u32 }>) {
  if (l.n++ > 0) return;
  console.log(`groups ${store.groups().length} ${typeof (store as any).insert}`);
}
app.addSystems(Update, read);
"##
    );
    let mut app = gizmo_app("gizmos-store-read", &body, |_| {});
    app.update();
    assert_eq!(logged(&mut app, "groups "), ["5 undefined"]);
}

/// `packages/bevy-ts/test/engine/native-modules.d.ts` (written by `scripts/gen-bevy-types.sh`)
/// holds the engine's `bevy-ts/gizmos` block, so `tsc -p test` checks `gizmos.ts`'s type cases
/// against what a game gets. Rerun the script after changing the declarations.
#[test]
fn the_engine_declares_gizmos() {
    let app = gizmo_app("gizmos-declare", "", |_| {});
    let block = |modules: &str| {
        let start = modules.find("declare module \"bevy-ts/gizmos\" {").expect("bevy-ts/gizmos is declared");
        let end = start + modules[start..].find("\n}\n").unwrap() + 3;
        modules[start..end].to_owned()
    };
    let engine = block(&bevy_ts_cli::types::emit_modules(app.world()));
    let fixture = include_str!("../../../../packages/bevy-ts/test/engine/native-modules.d.ts");
    assert!(block(fixture) == engine, "test/engine/native-modules.d.ts has a stale bevy-ts/gizmos; rerun scripts/gen-bevy-types.sh");
}

/// Every `docs/gizmos.md` coverage row marked works names tests that exist in this file.
#[test]
fn docs_rows_name_real_tests() {
    let doc = include_str!("../../../../docs/gizmos.md");
    let here = include_str!("gizmos.rs");
    let mut missing = Vec::new();
    for row in doc.lines().filter(|l| l.starts_with("| ") && !l.starts_with("| Bevy")) {
        let cells: Vec<&str> = row.trim_matches('|').split(" | ").map(str::trim).collect();
        let [.., status, tests] = cells.as_slice() else { continue };
        if !status.starts_with("works") {
            continue;
        }
        for name in tests.split('`').skip(1).step_by(2).filter(|n| n.contains('_') && n.chars().all(|c| c.is_ascii_lowercase() || c == '_' || c.is_ascii_digit())) {
            if !here.contains(&format!("fn {name}(")) {
                missing.push(name.to_string());
            }
        }
    }
    assert!(missing.is_empty(), "docs/gizmos.md names tests that do not exist: {missing:?}");
}
