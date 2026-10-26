//! Reflect-tier rows and layout-tier holes in TS systems (reflect-coverage stage 3), through V8:
//! the changed-tick cache, the binary codec on a miss, and writes through `reflect_value::write`.

use crate::common;

use bevy::camera::{OrthographicProjection, PerspectiveProjection, Projection};
use bevy::prelude::*;
use bevy_ts::reflect_codec::CALLS;
use common::*;
use std::sync::atomic::Ordering;

/// Reflect tier: no plain-data leaf.
#[derive(Component, Reflect, Default)]
#[reflect(Component, Default)]
pub struct Badge {
    pub name: String,
    pub tags: Vec<String>,
}

/// Layout tier with one hole: `power` is a column, `tint` is read through reflection.
#[derive(Component, Reflect, Default)]
#[reflect(Component, Default)]
pub struct Lamp {
    pub power: f32,
    pub tint: Color,
}

const HEAD: &str = r#"import { app, chain, f32, nativeComponent, rustComponent, Transform, Update } from "bevy-ts";
import type { Changed, Mut, Query } from "bevy-ts";
export const Projection = rustComponent("Projection");
export const Badge = rustComponent("Badge");
export class Lamp extends nativeComponent("Lamp", { power: f32 }) {}
"#;

fn with_types(name: &str, body: &str) -> App {
    let dir = project(name, &[("game.ts", &format!("{HEAD}{body}"))]);
    let mut app = app(&dir.join("game.ts"));
    app.register_type::<Projection>().register_type::<Badge>().register_type::<Lamp>();
    app
}

fn calls() -> u64 {
    CALLS.load(Ordering::Relaxed)
}

#[test]
fn a_camera_switches_to_orthographic_and_changed_sees_it() {
    let body = r#"
export function ortho(q: Query<[Mut<Projection>]>) {
  q.each((p: any) => {
    if ("Perspective" in p.value) p.value = { Orthographic: {} };
  });
}
export function seen(q: Query<[Projection], Changed<Projection>>) {
  q.each((p: any) => console.log("changed " + Object.keys(p.value)[0]));
}
app.addSystems(Update, chain(ortho, seen));
"#;
    let mut app = with_types("reflect-camera", body);
    let cam = app.world_mut().spawn(Projection::Perspective(PerspectiveProjection::default())).id();
    app.update();
    let r = reload(&mut app, false);
    assert!(r.errors.is_empty(), "{:?}", r.errors);
    app.update();
    assert!(matches!(app.world().get::<Projection>(cam), Some(Projection::Orthographic(_))), "{:?}", app.world().get::<Projection>(cam));
    // Console output reaches the log on a later frame.
    updates(&mut app, 2);
    assert!(messages(&app).iter().any(|m| m == "changed Orthographic"), "{:?}", messages(&app));
}

#[test]
fn unchanged_rows_make_no_host_call_and_untouched_rows_none_at_all() {
    let body = r#"
export function names(q: Query<[Badge]>) {
  let n = 0;
  q.each((b: any) => { if (b.name.startsWith("b")) n++; });
  if (n !== 1000) throw new Error("read " + n);
}
export function positions(q: Query<[Transform, Badge]>) {
  q.each((t) => { if (t.translation.x < 0) throw new Error("x"); });
}
app.addSystems(Update, chain(names, positions));
"#;
    let mut app = with_types("reflect-cache", body);
    for i in 0..1000 {
        app.world_mut().spawn((Transform::default(), Badge { name: format!("b{i}"), tags: vec!["x".into()] }));
    }
    app.update();
    let r = reload(&mut app, false);
    assert!(r.errors.is_empty(), "{:?}", r.errors);
    let before = calls();
    app.update();
    // Tests in this binary run in parallel and share the counter: at least our 1000 misses.
    assert!(calls() - before >= 1000, "{}", calls() - before);
    assert!(messages(&app).iter().all(|m| !m.contains("threw")), "{:?}", messages(&app));
    // The second frame hits the cache for every row; `positions` never reads Badge.
    let mut quiet = None;
    for _ in 0..5 {
        let before = calls();
        app.update();
        if calls() == before {
            quiet = Some(());
            break;
        }
    }
    assert!(quiet.is_some(), "every frame made reflected host calls");
}

#[test]
fn a_hole_reads_and_writes_and_column_views_stay_valid() {
    let body = r##"
export function light(q: Query<[Mut<Transform>, Mut<Lamp>]>) {
  q.each((t, l: any) => {
    if (l.power !== 0) return;
    const before = t.translation;
    l.tint = "#ff0000";
    t.translation.x = 5;
    l.power = 2;
    console.log("tint " + JSON.stringify(l.tint) + " x " + before.x);
  });
}
app.addSystems(Update, light);
"##;
    let mut app = with_types("reflect-hole", body);
    let e = app.world_mut().spawn((Transform::default(), Lamp::default())).id();
    app.update();
    let r = reload(&mut app, false);
    assert!(r.errors.is_empty(), "{:?}", r.errors);
    app.update();
    let lamp = app.world().get::<Lamp>(e).unwrap();
    assert_eq!(lamp.power, 2.0);
    assert_eq!(lamp.tint.to_srgba(), Srgba::RED);
    assert_eq!(app.world().get::<Transform>(e).unwrap().translation.x, 5.0);
    updates(&mut app, 2);
    assert!(messages(&app).iter().any(|m| m.starts_with("tint {\"Srgba\"") && m.ends_with("x 5")), "{:?}", messages(&app));
}

#[test]
fn a_struct_row_has_its_fields_and_writes_them() {
    let body = r#"
export function rename(q: Query<[Mut<Badge>]>) {
  q.each((b: any) => {
    if (b.name === "old") b.name = "new";
    else if (b.name === "new" && b.value.tags.length === 0) b.value.tags.push("again");
    else if (b.name === "new") {
      // An in-place change, then a field assign in the same run: both are written.
      b.value.tags.push("both");
      b.name = "last";
    }
  });
}
app.addSystems(Update, rename);
"#;
    let mut app = with_types("reflect-struct", body);
    let e = app.world_mut().spawn(Badge { name: "old".into(), tags: vec![] }).id();
    app.update();
    let r = reload(&mut app, false);
    assert!(r.errors.is_empty(), "{:?}", r.errors);
    app.update();
    assert_eq!(app.world().get::<Badge>(e).unwrap().name, "new");
    // A `Mut` value changed in place is written back at the end of the run.
    app.update();
    assert_eq!(app.world().get::<Badge>(e).unwrap().tags, vec!["again".to_string()]);
    app.update();
    let b = app.world().get::<Badge>(e).unwrap();
    assert_eq!((b.name.as_str(), b.tags.clone()), ("last", vec!["again".to_string(), "both".to_string()]));
    assert!(messages(&app).iter().all(|m| !m.contains("threw")), "{:?}", messages(&app));
}

#[test]
fn reflected_rows_are_never_tables() {
    let body = r#"
export function raw(q: Query<[Badge]>) {
  try { for (const t of q.tables) t.read(Badge as any); } catch (e) { console.log(String(e)); }
}
app.addSystems(Update, raw);
"#;
    let mut app = with_types("reflect-tables", body);
    app.world_mut().spawn(Badge::default());
    app.update();
    reload(&mut app, false);
    updates(&mut app, 3);
    assert!(messages(&app).iter().any(|m| m.contains("never a column")), "{:?}", messages(&app));
}

#[test]
fn an_optional_reflect_tier_term_is_its_row_or_undefined() {
    let body = r#"
import type { Entity, Option } from "bevy-ts";
export function look(q: Query<[Entity, Lamp, Option<Projection>]>) {
  const seen: string[] = [];
  for (const [, , p] of q) seen.push(p === undefined ? "none" : JSON.stringify((p as any).value).includes("Perspective") ? "persp" : "other");
  console.log(`rows ${seen.sort().join(",")}`);
}
app.addSystems(Update, look);
"#;
    let mut app = with_types("reflect-option", body);
    app.world_mut().spawn((Lamp::default(), Projection::Perspective(PerspectiveProjection::default())));
    app.world_mut().spawn(Lamp::default());
    app.update();
    let r = reload(&mut app, false);
    assert!(r.errors.is_empty(), "{:?}", r.errors);
    updates(&mut app, 3);
    assert!(messages(&app).iter().any(|m| m == "rows none,persp"), "{:?}", messages(&app));
}

/// `ref.get` in TSX reads a reflect-tier component and a layout-tier hole, and re-runs its
/// binding when Rust changes either.
#[test]
fn ref_get_reads_reflected_values_and_reruns_on_change() {
    use bevy::light::DirectionalLight;
    use bevy_ts_tsx::TsxNode;
    let src = r##"import { app, ref, rustComponent } from "bevy-ts";
import { DirectionalLight } from "bevy-ts/bevy";
export const Projection = rustComponent("Projection");
const sun = ref();
app.scene("main", () => (
  <>
    <entity key="sun" ref={sun}><DirectionalLight /><Projection value={{ Perspective: {} }} /></entity>
    <node key="hud">
      <text key="cam">{() => { const p = sun.get(Projection); return p ? (JSON.stringify(p).includes("Orthographic") ? "ortho" : "persp") : "none"; }}</text>
      <text key="color">{() => JSON.stringify(sun.get(DirectionalLight).color)}</text>
    </node>
  </>
));
"##;
    let dir = project("reflect-ref-get", &[("game.tsx", src)]);
    let mut app = app(&dir.join("game.tsx"));
    app.register_type::<Projection>().register_type::<DirectionalLight>();
    let r = reload(&mut app, false);
    assert!(r.errors.is_empty(), "{:?}", r.errors);
    app.update();
    let find = |app: &mut App, path: &[&str]| {
        let world = app.world_mut();
        world.query::<(Entity, &TsxNode)>().iter(world).find(|(_, n)| n.key_path == path).map(|(e, _)| e).unwrap()
    };
    let text = |app: &mut App, path: &[&str]| {
        let e = find(app, path);
        app.world().get::<Text>(e).map(|t| t.0.clone()).unwrap_or_default()
    };
    let sun = find(&mut app, &["sun"]);
    assert_eq!(text(&mut app, &["hud", "cam"]), "persp", "{:?}", logs(&app).into_iter().map(|l| l.message).collect::<Vec<_>>());
    *app.world_mut().get_mut::<Projection>(sun).unwrap() = Projection::Orthographic(OrthographicProjection::default_3d());
    updates(&mut app, 2);
    assert_eq!(text(&mut app, &["hud", "cam"]), "ortho");
    app.world_mut().get_mut::<DirectionalLight>(sun).unwrap().color = Color::srgb(0.25, 0.5, 0.75);
    updates(&mut app, 2);
    let color = text(&mut app, &["hud", "color"]);
    assert!(color.contains("Srgba") && color.contains("0.25"), "{color}");
}

#[test]
fn a_computed_component_refuses_a_write() {
    let body = r#"
export const GlobalTransform = rustComponent("GlobalTransform");
export function nudge(q: Query<[Mut<GlobalTransform>]>) {
  q.each((g: any) => { g.value = 0; });
}
app.addSystems(Update, nudge);
"#;
    let mut app = with_types("reflect-computed", body);
    let e = app.world_mut().spawn((Transform::from_xyz(1.0, 2.0, 3.0), GlobalTransform::from_xyz(1.0, 2.0, 3.0))).id();
    app.update();
    let r = reload(&mut app, false);
    assert!(r.errors.is_empty(), "{:?}", r.errors);
    app.update();
    let err = app.world().resource::<bevy_ts::systems::TsSystems>().iter().find_map(|s| s.last_error().map(|e| e.message.clone()));
    assert!(err.as_deref().is_some_and(|m| m.contains("GlobalTransform is computed by Bevy, so it cannot be written")), "{err:?}");
    assert_eq!(app.world().get::<GlobalTransform>(e).unwrap().translation(), Vec3::new(1.0, 2.0, 3.0));
}

/// Agent town's civ kit: `ResMut` of Bevy's reflect-tier shadow-map resources (`size: usize` has
/// no native layout) writes back, and Bevy sees the change.
#[test]
fn res_mut_of_a_reflect_tier_resource_writes_back_with_change_detection() {
    use bevy::light::{DirectionalLightShadowMap, PointLightShadowMap};
    let body = r#"
export const DirectionalLightShadowMap = rustComponent("DirectionalLightShadowMap");
export const PointLightShadowMap = rustComponent("PointLightShadowMap");
export function shadows(d: ResMut<DirectionalLightShadowMap>, p: ResMut<PointLightShadowMap>) {
  if ((d as any).size !== 4096) (d as any).size = 4096;
  if ((p as any).size !== 4096) (p as any).size = 4096;
}
app.addSystems(Update, shadows);
"#;
    let dir = project("reflect-res", &[("game.ts", &format!("{}{body}", HEAD.replace("import type { Changed, Mut, Query }", "import type { Changed, Mut, Query, ResMut }")))]);
    let mut app = app(&dir.join("game.ts"));
    app.register_type::<DirectionalLightShadowMap>().register_type::<PointLightShadowMap>();
    app.insert_resource(DirectionalLightShadowMap::default()).insert_resource(PointLightShadowMap::default());
    #[derive(Resource, Default)]
    struct Seen(Vec<(usize, bool, usize, bool)>);
    app.init_resource::<Seen>();
    app.add_systems(
        Update,
        (|d: Res<DirectionalLightShadowMap>, p: Res<PointLightShadowMap>, mut seen: ResMut<Seen>| {
            seen.0.push((d.size, d.is_changed(), p.size, p.is_changed()));
        })
        .after(bevy_ts::BevyTsSet::Update),
    );
    app.update();
    let r = reload(&mut app, false);
    assert!(r.errors.is_empty(), "{:?}", r.errors);
    app.update();
    app.update();
    let seen = &app.world().resource::<Seen>().0;
    let n = seen.len();
    // The frame after load writes both; the next sees them unchanged (the cached size is 4096).
    assert_eq!(seen[n - 2], (4096, true, 4096, true), "{seen:?}");
    assert_eq!(seen[n - 1], (4096, false, 4096, false), "{seen:?}");
    assert!(messages(&app).iter().all(|m| !m.contains("threw")), "{:?}", messages(&app));
}

/// reflect-bindable: a `GlobalTransform` row has Bevy's read methods, each equal to Bevy's own on a
/// rotated, scaled, parented entity, read after propagation.
#[test]
fn global_transform_reads_as_bevys() {
    let body = r#"
import { GlobalTransform, PostUpdate, systems, systemSet } from "bevy-ts";
import type { Entity } from "bevy-ts";
const v = (x: { x: number; y: number; z: number }) => [x.x, x.y, x.z];
export function read(q: Query<[Entity, GlobalTransform]>) {
  q.each((e, g) => {
    const r = g.rotation();
    const [s, r2, t2] = g.toScaleRotationTranslation();
    const ct = g.computeTransform();
    const lanes = [
      ...v(g.translation()), r.x, r.y, r.z, r.w, ...v(g.scale()),
      ...v(s), r2.x, r2.y, r2.z, r2.w, ...v(t2),
      ...v(ct.translation), ...v(ct.scale),
      ...g.affine(), ...g.toMatrix(),
      ...v(g.right()), ...v(g.left()), ...v(g.up()), ...v(g.down()), ...v(g.back()), ...v(g.forward()),
      ...v(g.transformPoint([0, 0, 1])), ...v(g.transformVector([1, 2, 3])),
      ...g.mulTransform({ translation: [1, 0, 0], scale: [2, 2, 2] }).value,
    ];
    console.log(`gt ${e} ${JSON.stringify(lanes)}`);
  });
  const all: [Entity, readonly number[]][] = [];
  q.each((e, g) => { all.push([e, Array.from(g.value)]); });
  q.each((e, g) => {
    for (const [p, value] of all) {
      const t = g.reparentedTo({ value: value as GlobalTransform["value"] });
      console.log(`rp ${e} ${p} ${JSON.stringify([...v(t.translation), t.rotation.x, t.rotation.y, t.rotation.z, t.rotation.w, ...v(t.scale)])}`);
    }
  });
}
app.addSystems(PostUpdate, systems(read).after(systemSet("bevy:transform-propagate")));
"#;
    let mut app = with_types("reflect-global-transform", body);
    app.add_plugins(bevy::transform::TransformPlugin);
    let parent = app
        .world_mut()
        .spawn(Transform::from_xyz(0.0, 2.0, 0.0).with_rotation(Quat::from_rotation_y(std::f32::consts::FRAC_PI_2)).with_scale(Vec3::splat(2.0)))
        .id();
    let child = app.world_mut().spawn((Transform::from_xyz(1.0, 0.0, 0.0).with_rotation(Quat::from_rotation_x(0.3)), ChildOf(parent))).id();
    app.update();
    let r = reload(&mut app, false);
    assert!(r.errors.is_empty(), "{:?}", r.errors);
    app.update();
    app.update();
    bevy_ts::runtime::pump(app.world_mut());
    let logs = messages(&app);
    for e in [parent, child] {
        let g = *app.world().get::<GlobalTransform>(e).unwrap();
        let (s, r, t) = g.to_scale_rotation_translation();
        let ct = g.compute_transform();
        let mut want: Vec<f32> = Vec::new();
        want.extend(g.translation().to_array());
        want.extend(g.rotation().to_array());
        want.extend(g.scale().to_array());
        want.extend(s.to_array());
        want.extend(r.to_array());
        want.extend(t.to_array());
        want.extend(ct.translation.to_array());
        want.extend(ct.scale.to_array());
        want.extend(g.affine().to_cols_array());
        want.extend(g.to_matrix().to_cols_array());
        for d in [g.right(), g.left(), g.up(), g.down(), g.back(), g.forward()] {
            want.extend(d.to_array());
        }
        want.extend(g.transform_point(Vec3::Z).to_array());
        want.extend(g.transform_vector(Vec3::new(1.0, 2.0, 3.0)).to_array());
        want.extend(g.mul_transform(Transform::from_xyz(1.0, 0.0, 0.0).with_scale(Vec3::splat(2.0))).affine().to_cols_array());
        let prefix = format!("gt {} ", e.to_bits());
        let line = logs.iter().rev().find_map(|m| m.strip_prefix(&prefix)).unwrap_or_else(|| panic!("no read of {e}: {logs:?}"));
        let got: Vec<f32> = serde_json::from_str(line).unwrap();
        assert_eq!(got.len(), want.len(), "{line}");
        for (i, (a, b)) in got.iter().zip(&want).enumerate() {
            assert!((a - b).abs() < 1e-5, "lane {i} of {e}: got {a}, Bevy has {b}\n{line}");
        }
    }
    // `reparentedTo`, the child relative to its parent: Bevy's, which is the child's own Transform.
    let (pg, cg) = (*app.world().get::<GlobalTransform>(parent).unwrap(), *app.world().get::<GlobalTransform>(child).unwrap());
    let rel = cg.reparented_to(&pg);
    let want: Vec<f32> = [rel.translation.to_array().as_slice(), &rel.rotation.to_array(), &rel.scale.to_array()].concat();
    let prefix = format!("rp {} {} ", child.to_bits(), parent.to_bits());
    let line = logs.iter().rev().find_map(|m| m.strip_prefix(&prefix)).unwrap_or_else(|| panic!("no reparentedTo: {logs:?}"));
    let got: Vec<f32> = serde_json::from_str(line).unwrap();
    for (i, (a, b)) in got.iter().zip(&want).enumerate() {
        assert!((a - b).abs() < 1e-5, "reparentedTo lane {i}: got {a}, Bevy has {b}\n{line}");
    }
}
