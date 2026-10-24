//! Reflect-tier rows and layout-tier holes in TS systems (reflect-coverage stage 3), through V8:
//! the changed-tick cache, the binary codec on a miss, and writes through `reflect_value::write`.

mod common;

use bevy::camera::{PerspectiveProjection, Projection};
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
    else if (b.name === "new") b.value.tags.push("again");
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
