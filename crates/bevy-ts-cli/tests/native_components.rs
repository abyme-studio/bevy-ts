//! Rust components in TS systems by tier (native decision 5), through the V8 backend.

mod common;

use bevy::prelude::*;
use common::*;
use std::sync::{Arc, Mutex};

#[derive(Component, Reflect, Default)]
#[reflect(Component, Default)]
pub struct Thrust {
    pub power: f32,
    pub dir: Vec3,
}

#[derive(Component, Reflect, Default)]
#[reflect(Component, Default)]
#[component(immutable)]
pub struct Frozen {
    pub v: f32,
}

#[derive(Component, Reflect, Default)]
#[reflect(Component, Default)]
pub struct Weapon {
    pub name: String,
}

#[derive(Reflect, Default, Clone, Copy)]
pub enum Gear {
    #[default]
    Low,
    High,
}

#[derive(Component, Reflect, Default)]
#[reflect(Component, Default)]
pub struct Engine {
    pub on: bool,
    pub gear: Gear,
}

const HEAD: &str = r#"import { app, bool, enumOf, f32, nativeComponent, Update, vec3 } from "bevy-ts";
import type { Mut, Query } from "bevy-ts";
export class Thrust extends nativeComponent("Thrust", { power: f32, dir: vec3 }) {}
export class Frozen extends nativeComponent("Frozen", { v: f32 }) {}
export class Weapon extends nativeComponent("Weapon", { v: f32 }) {}
export class Engine extends nativeComponent("Engine", { on: bool, gear: enumOf("Low", "High") }) {}
"#;

fn with_types(dir: &std::path::Path) -> App {
    let mut app = app(&dir.join("game.ts"));
    app.register_type::<Thrust>().register_type::<Frozen>().register_type::<Weapon>().register_type::<Engine>();
    app
}

#[derive(Resource, Clone, Default)]
struct Seen(Arc<Mutex<Vec<(f32, bool)>>>);

#[test]
fn a_ts_write_is_seen_by_rust_the_same_frame_and_changed_only_on_written_rows() {
    let body = r#"
export function throttle(q: Query<[Mut<Thrust>]>) {
  q.each((t) => {
    if (t.power < 2) t.power += 10;
  });
}
app.addSystems(Update, throttle);
"#;
    let dir = project("native-thrust", &[("game.ts", &format!("{HEAD}{body}"))]);
    let mut app = with_types(&dir);
    for p in [0.0, 1.0, 5.0, 6.0] {
        app.world_mut().spawn(Thrust { power: p, dir: Vec3::Y });
    }
    let seen = Seen::default();
    let s = seen.clone();
    app.add_systems(
        Update,
        (move |q: Query<Ref<Thrust>>| {
            let mut v: Vec<(f32, bool)> = q.iter().map(|t| (t.power, t.is_changed())).collect();
            v.sort_by(|a, b| a.0.total_cmp(&b.0));
            *s.0.lock().unwrap() = v;
        })
        .after(bevy_ts::BevyTsSet::Update),
    );
    // Spawns count as changes: let one frame pass before loading.
    app.update();
    let r = reload(&mut app, false);
    assert!(r.errors.is_empty(), "{:?}", r.errors);
    app.update();
    // The first frame after load writes rows 0 and 1; the Rust system after the stage sees it.
    assert_eq!(*seen.0.lock().unwrap(), vec![(5.0, false), (6.0, false), (10.0, true), (11.0, true)]);
}

#[test]
fn tiers_and_guards_fail_the_load_naming_the_component() {
    let cases = [
        ("Query<[Weapon]>", "Weapon"),
        ("Query<[Mut<Frozen>]>", "immutable"),
    ];
    for (i, (ty, want)) in cases.iter().enumerate() {
        let body = format!("export function s(q: {ty}) {{}}\napp.addSystems(Update, s);\n");
        let dir = project(&format!("native-guard-{i}"), &[("game.ts", &format!("{HEAD}{body}"))]);
        let mut app = with_types(&dir);
        let r = reload(&mut app, false);
        assert!(r.errors.iter().any(|e| e.message.contains(want)), "{ty}: {:?}", r.errors);
    }
}

#[test]
fn enum_and_bool_fields_go_through_accessors_only() {
    let body = r#"
export function shift(q: Query<[Mut<Engine>]>) {
  q.each((e) => {
    e.on = true;
    e.gear = "High";
  });
  let raw = "";
  try {
    for (const t of q.tables) t.write(Engine);
  } catch (err) {
    raw = err.message;
  }
  console.log(`raw ${raw}`);
}
app.addSystems(Update, shift);
"#;
    let dir = project("native-engine", &[("game.ts", &format!("{HEAD}{body}"))]);
    let mut app = with_types(&dir);
    let e = app.world_mut().spawn(Engine::default()).id();
    let r = reload(&mut app, false);
    assert!(r.errors.is_empty(), "{:?}", r.errors);
    app.update();
    app.update();
    let eng = app.world().get::<Engine>(e).unwrap();
    assert!(eng.on && matches!(eng.gear, Gear::High));
    assert!(messages(&app).iter().any(|m| m.contains("raw Engine has fields only accessors may write")), "{:?}", messages(&app));
}

#[test]
fn a_reflect_tier_component_is_a_tsx_element() {
    let src = r#"import { app, ReflectComponent } from "bevy-ts";
export class Weapon extends ReflectComponent { static path = "native_components::Weapon"; declare name: string }
app.scene("armory", () => (
  <entity name="gun">
    <Weapon name="laser" />
  </entity>
));
"#;
    let dir = project("native-weapon", &[("game.tsx", src)]);
    let mut app = app(&dir.join("game.tsx"));
    app.add_plugins(AssetPlugin::default()).register_type::<Weapon>();
    let r = reload(&mut app, false);
    assert!(r.errors.is_empty(), "{:?}", r.errors);
    app.update();
    let world = app.world_mut();
    let names: Vec<String> = world.query::<&Weapon>().iter(world).map(|w| w.name.clone()).collect();
    assert_eq!(names, vec!["laser"]);
}
