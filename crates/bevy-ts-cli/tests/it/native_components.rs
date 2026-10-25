//! Rust components in TS systems by tier (native decision 5), through the V8 backend.

use crate::common;

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
export class Missing extends nativeComponent("Missing", { v: f32 }) {}
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
        // A name this engine never registered (a project engine's component under the prebuilt one).
        ("Query<[Missing]>", "no Rust component named Missing is registered in this engine"),
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
export class Weapon extends ReflectComponent { static path = "it::native_components::Weapon"; declare name: string }
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

/// Two Rust types with one short name, in different modules.
pub mod a {
    use bevy::prelude::*;
    #[derive(Component, Reflect, Default)]
    #[reflect(Component, Default)]
    pub struct Health {
        pub hp: f32,
    }
}

pub mod b {
    use bevy::prelude::*;
    #[derive(Component, Reflect, Default)]
    #[reflect(Component, Default)]
    pub struct Health {
        pub regen: f32,
        pub max: f32,
    }
}

#[test]
fn two_types_with_one_short_name_both_get_layouts_under_their_paths() {
    let body = r#"
import { f32 as F32, nativeComponent as native } from "bevy-ts";
export class AHealth extends native("it::native_components::a::Health", { hp: F32 }) {}
export class BHealth extends native("it::native_components::b::Health", { regen: F32, max: F32 }) {}
export function heal(qa: Query<[Mut<AHealth>]>, qb: Query<[Mut<BHealth>]>) {
  qa.each((h) => { h.hp += 1; });
  qb.each((h) => { h.regen = 2; h.max = 50; });
}
app.addSystems(Update, heal);
"#;
    let dir = project("native-same-short", &[("game.ts", &format!("{HEAD}{body}"))]);
    let mut app = app(&dir.join("game.ts"));
    app.register_type::<a::Health>().register_type::<b::Health>();
    let ea = app.world_mut().spawn(a::Health { hp: 10.0 }).id();
    let eb = app.world_mut().spawn(b::Health::default()).id();
    let r = reload(&mut app, false);
    assert!(r.errors.is_empty(), "{:?}", r.errors);
    app.update();
    let native = app.world().resource::<bevy_ts::layout::NativeLayouts>();
    for path in ["it::native_components::a::Health", "it::native_components::b::Health"] {
        assert!(native.get(path).is_some_and(|n| n.type_path == path), "{path} has no layout");
    }
    assert_eq!(app.world().get::<a::Health>(ea).unwrap().hp, 11.0);
    let bh = app.world().get::<b::Health>(eb).unwrap();
    assert_eq!((bh.regen, bh.max), (2.0, 50.0));
}

#[test]
fn an_ambiguous_short_name_fails_the_load_naming_both_paths() {
    let body = r#"
import { f32 as F32, nativeComponent as native } from "bevy-ts";
export class Health extends native("Health", { hp: F32 }) {}
export function s(q: Query<[Health]>) {}
app.addSystems(Update, s);
"#;
    let dir = project("native-same-short-ambiguous", &[("game.ts", &format!("{HEAD}{body}"))]);
    let mut app = app(&dir.join("game.ts"));
    app.register_type::<a::Health>().register_type::<b::Health>();
    let r = reload(&mut app, false);
    // Every reflected `Health` in this test binary is registered, so the list may name more.
    let named = |m: &str| {
        m.contains("Health names more than one Rust type") && ["it::native_components::a::Health", "it::native_components::b::Health"].iter().all(|p| m.contains(p))
    };
    assert!(r.errors.iter().any(|e| named(&e.message)), "{:?}", r.errors);
}
