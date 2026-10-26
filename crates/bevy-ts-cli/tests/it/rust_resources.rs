//! Rust resources as `Res<R>`/`ResMut<R>` in TS systems (change rust-resources), through the V8
//! backend.

use crate::common;

use bevy::light::GlobalAmbientLight;
use bevy::prelude::*;
use bevy::ui::UiScale;
use bevy_ts_native::BevyTsNativeExt;
use common::*;
use std::sync::{Arc, Mutex};

/// A derive-only resource, as a native crate registers it.
#[derive(Resource, bevy_ts_native::NativeLayout, Default)]
pub struct Wind {
    pub speed: f32,
}

#[derive(Resource, Reflect, Default)]
#[reflect(Resource, Default)]
#[component(immutable)]
pub struct Seed {
    pub v: u32,
}

mod a {
    use bevy::prelude::*;
    #[derive(Resource, Reflect, Default)]
    #[reflect(Resource, Default)]
    pub struct Gust {
        pub v: f32,
    }
}

mod b {
    use bevy::prelude::*;
    #[derive(Resource, Reflect, Default)]
    #[reflect(Resource, Default)]
    pub struct Gust {
        pub v: f32,
    }
}

/// A tiered and an untiered resource that share the short name `Breeze`.
pub mod tiered {
    use bevy::prelude::*;
    #[derive(Resource, Reflect, Default)]
    #[reflect(Resource, Default)]
    pub struct Breeze {
        pub v: f32,
    }
}

pub mod untiered {
    use bevy::prelude::*;
    #[derive(Resource, Default)]
    pub struct Breeze;
}

const HEAD: &str = r#"import { app, f32, nativeResource, Transform, u32, Update } from "bevy-ts";
import type { Local, Option, Res, ResMut } from "bevy-ts";
export class UiScale extends nativeResource("UiScale", { value: f32 }) {}
export class ClearColor extends nativeResource("ClearColor", { v: f32 }) {}
export class Seed extends nativeResource("Seed", { v: u32 }) {}
export class Gust extends nativeResource("Gust", { v: f32 }) {}
export class Nope extends nativeResource("Nope", { v: f32 }) {}
export class Wind extends nativeResource("it::rust_resources::Wind", { speed: f32 }) {}
"#;

fn with_types(dir: &std::path::Path) -> App {
    let mut app = app(&dir.join("game.ts"));
    app.register_type::<GlobalAmbientLight>()
        .register_type::<UiScale>()
        .register_type::<ClearColor>()
        .register_type::<Seed>()
        .register_type::<a::Gust>()
        .register_type::<b::Gust>();
    app
}

fn load_ok(app: &mut App) {
    let r = reload(app, false);
    assert!(r.errors.is_empty(), "{:?}", r.errors);
}

fn run_error(app: &App) -> Option<String> {
    app.world().resource::<bevy_ts::systems::TsSystems>().iter().find_map(|s| s.last_error.as_ref().map(|e| e.message.clone()))
}

#[derive(Clone, Default)]
struct Seen<T: Send + Sync + 'static>(Arc<Mutex<Vec<T>>>);

#[test]
fn a_ts_write_to_a_bevy_resource_is_seen_by_rust_that_frame_and_changed_once() {
    // Imported from the generated `bevy-ts/bevy`, as a game does.
    let src = r#"import { app, Update } from "bevy-ts";
import type { Local, ResMut, u32 } from "bevy-ts";
import { GlobalAmbientLight } from "bevy-ts/bevy";
export function dim(ambient: ResMut<GlobalAmbientLight>, runs: Local<{ n: u32 }>) {
  if (runs.n++ === 0) ambient.brightness = 400;
}
app.addSystems(Update, dim);
"#;
    let dir = project("rust-res-ambient", &[("game.ts", src)]);
    let mut app = with_types(&dir);
    app.insert_resource(GlobalAmbientLight { brightness: 80.0, ..default() });
    let seen = Seen::<(f32, bool)>::default();
    let s = seen.clone();
    app.add_systems(
        Update,
        (move |a: Res<GlobalAmbientLight>| s.0.lock().unwrap().push((a.brightness, a.is_changed()))).after(bevy_ts::BevyTsSet::Update),
    );
    // The insert counts as a change: let one frame pass before loading.
    app.update();
    load_ok(&mut app);
    seen.0.lock().unwrap().clear();
    app.update();
    app.update();
    assert_eq!(*seen.0.lock().unwrap(), vec![(400.0, true), (400.0, false)]);
}

#[test]
fn a_read_of_a_bevy_resource_does_not_move_its_tick() {
    let body = r#"
export function show(scale: Res<UiScale>) {
  console.log(`scale ${scale.value}`);
}
app.addSystems(Update, show);
"#;
    let dir = project("rust-res-scale", &[("game.ts", &format!("{HEAD}{body}"))]);
    let mut app = with_types(&dir);
    app.insert_resource(UiScale(2.0));
    let seen = Seen::<bool>::default();
    let s = seen.clone();
    app.add_systems(Update, (move |u: Res<UiScale>| s.0.lock().unwrap().push(u.is_changed())).after(bevy_ts::BevyTsSet::Update));
    app.update();
    load_ok(&mut app);
    seen.0.lock().unwrap().clear();
    updates(&mut app, 2);
    assert!(messages(&app).iter().any(|m| m == "scale 2"), "{:?}", messages(&app));
    assert_eq!(*seen.0.lock().unwrap(), vec![false, false]);
}

#[test]
fn unusable_resources_fail_the_load_naming_the_system_and_resource() {
    let cases = [
        ("Res<ClearColor>", "Res<ClearColor>: ClearColor is a Rust resource without a native layout ("),
        ("Res<Transform>", "Transform is a component, not a resource"),
        ("ResMut<Seed>", "ResMut<Seed>: Seed is immutable"),
        ("Res<Gust>", "Gust names more than one Rust type (it::rust_resources::a::Gust, it::rust_resources::b::Gust)"),
        ("Res<Nope>", "Res<Nope>: no resource named Nope"),
        ("Query<[AmbientData]>", "GlobalAmbientLight is a resource, not a component"),
    ];
    for (i, (ty, want)) in cases.iter().enumerate() {
        // Transform is a component class in JS too; name it through a Rust-side spelling.
        let ty = ty.replace("Res<Transform>", "Res<TransformRes>");
        let extra = "import { nativeComponent } from \"bevy-ts\";\nimport type { Query } from \"bevy-ts\";\nexport class TransformRes extends nativeResource(\"Transform\", { v: f32 }) {}\nexport class AmbientData extends nativeComponent(\"GlobalAmbientLight\", { brightness: f32 }) {}\n";
        let body = format!("{extra}export function s(r: {ty}) {{}}\napp.addSystems(Update, s);\n");
        let dir = project(&format!("rust-res-refuse-{i}"), &[("game.ts", &format!("{HEAD}{body}"))]);
        let mut app = with_types(&dir);
        app.register_type::<Transform>();
        let r = reload(&mut app, false);
        assert!(r.errors.iter().any(|e| e.message.starts_with("system game.ts#s: ") && e.message.contains(want)), "{ty}: {:?}", r.errors);
    }
}

#[test]
fn an_absent_resource_fails_the_run_and_an_optional_one_is_undefined() {
    let body = r#"
export function need(scale: Res<UiScale>) {
  console.log(`need ${scale.value}`);
}
app.addSystems(Update, need);
"#;
    let dir = project("rust-res-absent", &[("game.ts", &format!("{HEAD}{body}"))]);
    let mut app = with_types(&dir);
    load_ok(&mut app);
    app.update();
    let err = run_error(&app).expect("the run failed");
    assert!(err.contains("resource UiScale does not exist"), "{err}");

    let body = r#"
export function maybe(scale: Option<Res<UiScale>>) {
  console.log(`maybe ${scale === undefined ? "absent" : scale.value}`);
}
app.addSystems(Update, maybe);
"#;
    let dir = project("rust-res-optional", &[("game.ts", &format!("{HEAD}{body}"))]);
    let mut app = with_types(&dir);
    load_ok(&mut app);
    app.update();
    app.insert_resource(UiScale(3.0));
    updates(&mut app, 2);
    assert_eq!(run_error(&app), None);
    let m = messages(&app);
    assert!(m.iter().any(|m| m == "maybe absent") && m.iter().any(|m| m == "maybe 3"), "{m:?}");
}

#[test]
fn an_optional_ts_resource_inserted_later_is_written_in_place() {
    let src = r#"import { app, Resource, Update, type u32 } from "bevy-ts";
import type { Option, ResMut } from "bevy-ts";
export class Score extends Resource { value: u32 = 0 }
export function add(score: Option<ResMut<Score>>) {
  if (score === undefined) console.log("no score");
  else score.value += 1;
}
app.addSystems(Update, add);
"#;
    let dir = project("rust-res-ts-optional", &[("game.ts", src)]);
    let mut app = app(&dir.join("game.ts"));
    load_ok(&mut app);
    updates(&mut app, 2);
    assert!(messages(&app).iter().any(|m| m == "no score"), "{:?}", messages(&app));
    let (id, _) = bevy_ts::plugin_api::component_by_name(app.world(), "game::Score").expect("Score");
    let bytes = bevy_ts::plugin_api::encode_component(app.world(), "game::Score", &serde_json::Map::new()).unwrap();
    let mut e = app.world_mut().spawn(bevy::ecs::resource::IsResource::new(id));
    unsafe { bevy_ts::commands::insert_bytes(&mut e, id, &bytes) };
    updates(&mut app, 2);
    assert_eq!(read_res(&app, "game::Score"), Some(serde_json::json!({ "value": 2 })));
    assert_eq!(run_error(&app), None);
}

#[test]
fn a_derive_only_resource_from_a_native_crate() {
    let body = r#"
export function blow(wind: ResMut<Wind>) {
  wind.speed += 2;
}
app.addSystems(Update, blow);
"#;
    let dir = project("rust-res-wind", &[("game.ts", &format!("{HEAD}{body}"))]);
    let mut app = with_types(&dir);
    app.ts_resource_layout::<Wind>().insert_resource(Wind { speed: 1.0 });
    let seen = Seen::<f32>::default();
    let s = seen.clone();
    app.add_systems(Update, (move |w: Res<Wind>| s.0.lock().unwrap().push(w.speed)).after(bevy_ts::BevyTsSet::Update));
    load_ok(&mut app);
    updates(&mut app, 2);
    assert_eq!(*seen.0.lock().unwrap(), vec![3.0, 5.0]);
}

#[test]
fn resource_exists_sees_a_rust_resource() {
    let body = r#"
import { resourceExists, systems } from "bevy-ts";
export function guarded(scale: Res<UiScale>) {
  console.log(`guarded ${scale.value}`);
}
app.addSystems(Update, systems(guarded).runIf(resourceExists(UiScale)));
"#;
    let dir = project("rust-res-exists", &[("game.ts", &format!("{HEAD}{body}"))]);
    let mut app = with_types(&dir);
    load_ok(&mut app);
    app.update();
    assert_eq!(run_error(&app), None);
    app.insert_resource(UiScale(4.0));
    updates(&mut app, 2);
    assert!(messages(&app).iter().any(|m| m == "guarded 4"), "{:?}", messages(&app));
}

#[test]
fn resource_exists_refuses_an_ambiguous_short_name() {
    let body = r#"
import { resourceExists, systems } from "bevy-ts";
export function s() {}
app.addSystems(Update, systems(s).runIf(resourceExists(Gust)));
"#;
    let dir = project("rust-res-exists-ambiguous", &[("game.ts", &format!("{HEAD}{body}"))]);
    let mut app = with_types(&dir);
    let r = reload(&mut app, false);
    assert!(r.errors.iter().any(|e| e.message.contains("resourceExists(Gust): Gust names more than one Rust type")), "{:?}", r.errors);
}

#[test]
fn resource_changed_refuses_an_ambiguous_short_name() {
    let body = r#"
import { resourceChanged, systems } from "bevy-ts";
export function s() {}
app.addSystems(Update, systems(s).runIf(resourceChanged(Gust)));
"#;
    let dir = project("rust-res-changed-ambiguous", &[("game.ts", &format!("{HEAD}{body}"))]);
    let mut app = with_types(&dir);
    let r = reload(&mut app, false);
    let named = |m: &str| m.contains("resourceChanged(Gust): Gust names more than one Rust type") && m.contains("a::Gust") && m.contains("b::Gust");
    assert!(r.errors.iter().any(|e| named(&e.message)), "{:?}", r.errors);
}

#[test]
fn resource_exists_and_equals_reports_its_own_name() {
    let body = r#"
import { resourceExistsAndEquals, systems } from "bevy-ts";
export function s() {}
app.addSystems(Update, systems(s).runIf(resourceExistsAndEquals(Gust.of({ v: 1 }))));
"#;
    let dir = project("rust-res-exists-equals-ambiguous", &[("game.ts", &format!("{HEAD}{body}"))]);
    let mut app = with_types(&dir);
    let r = reload(&mut app, false);
    assert!(r.errors.iter().any(|e| e.message.contains("resourceExistsAndEquals(Gust): Gust names more than one Rust type")), "{:?}", r.errors);
}

#[test]
fn a_short_name_a_tiered_and_an_untiered_resource_share_fails_the_load_naming_both_paths() {
    let body = r#"
export class Breeze extends nativeResource("Breeze", { v: f32 }) {}
export function s(b: Res<Breeze>) {}
app.addSystems(Update, s);
"#;
    let dir = project("rust-res-tiered-untiered", &[("game.ts", &format!("{HEAD}{body}"))]);
    let mut app = with_types(&dir);
    app.register_type::<tiered::Breeze>();
    app.world_mut().register_component::<untiered::Breeze>();
    let r = reload(&mut app, false);
    let named = |m: &str| m.contains("Breeze names more than one Rust type") && m.contains("it::rust_resources::tiered::Breeze") && m.contains("it::rust_resources::untiered::Breeze");
    assert!(r.errors.iter().any(|e| named(&e.message)), "{:?}", r.errors);
}
