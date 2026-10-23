//! Levels placing TS scene components, against the real V8 backend: `@enemies::Grunt { … }` in a
//! `.bsn` file expanded by its `static scene`, overridden per instance, and reloaded.

use crate::common;

use bevy::prelude::*;
use bevy_ts_levels::engine::{Levels, LevelsPlugin};
use bevy_ts_tsx::TsxNode;
use common::*;
use std::path::Path;

const ENEMIES: &str = r#"import { app, Component, SceneComponent, type f32, type u8 } from "bevy-ts";
import { Transform } from "bevy-ts/bevy";
export class Health extends Component { hp: f32 = 100; max: f32 = 100 }
export class Grunt extends SceneComponent {
  team: u8 = 0;
  static scene({ hp = 100 }: { hp?: f32 }) {
    return (
      <entity name="grunt">
        <Health initialHp={hp} max={hp} />
        <Transform translation={[0, 0, -4]} />
        <entity key="eye"><Transform translation={[0, 0.6, 0.5]} /></entity>
      </entity>
    );
  }
}
app.level("levels/arena.bsn");
"#;

const ARENA: &str = "// Two grunts.
#GruntA
@enemies::Grunt
Transform { translation: Vec3 { x: -3.0 } }
--
#GruntB
@enemies::Grunt { @hp: 250.0, team: 1 }
Transform { translation: Vec3 { x: 3.0 } }
Children [
    #eye
    Transform { translation: Vec3 { y: 0.8 } }
]
";

fn level_app(dir: &Path) -> App {
    let mut app = App::new();
    // Canonical: Bevy's file watcher panics on a symlinked root (macOS /var).
    let assets = dir.join("assets").canonicalize().unwrap();
    app.add_plugins((MinimalPlugins, AssetPlugin { file_path: assets.to_string_lossy().into_owned(), ..Default::default() }))
        .init_asset::<Mesh>()
        .init_asset::<StandardMaterial>()
        .add_plugins(bevy_ts::BevyTsPlugin::new(bevy_ts_v8::factory(Default::default())).dev(true))
        .add_plugins(bevy_ts_tsx::TsxPlugin)
        .add_plugins(LevelsPlugin);
    app.world_mut().resource_mut::<Levels>().asset_root = assets;
    app.insert_resource(bevy_ts::generation::DevSource(std::sync::Arc::new(bevy_ts_cli::compile::Compiler::new(dir.join("src/enemies.tsx"), true).unwrap())));
    app
}


fn named(app: &mut App, name: &str) -> Entity {
    let world = app.world_mut();
    world.query::<(Entity, &Name)>().iter(world).find(|(_, n)| n.as_str() == name).map(|(e, _)| e).unwrap_or_else(|| panic!("no {name}"))
}

fn health(app: &App, e: Entity) -> (f32, f32) {
    let (id, _) = bevy_ts::plugin_api::component_by_name(app.world(), "enemies::Health").unwrap();
    let ptr = app.world().get_by_id(e, id).unwrap();
    // SAFETY: Health is two f32s.
    let b = unsafe { std::slice::from_raw_parts(ptr.as_ptr(), 8) };
    (f32::from_le_bytes(b[0..4].try_into().unwrap()), f32::from_le_bytes(b[4..8].try_into().unwrap()))
}

fn set_hp(app: &mut App, e: Entity, v: f32) {
    let (id, _) = bevy_ts::plugin_api::component_by_name(app.world(), "enemies::Health").unwrap();
    let mut m = app.world_mut().get_mut_by_id(e, id).unwrap();
    // SAFETY: Health is two f32s.
    unsafe { std::ptr::copy_nonoverlapping(v.to_le_bytes().as_ptr(), m.as_mut().as_ptr(), 4) };
}

fn errors(app: &App) -> Vec<String> {
    logs(app).into_iter().filter(|e| e.level == bevy_ts::log::LogLevel::Error).map(|e| format!("{} {}", e.location.map(|l| l.to_string()).unwrap_or_default(), e.message)).collect()
}

#[test]
fn a_level_places_scene_components_with_props_and_overrides() {
    let dir = project("levels-scene", &[("package.json", "{}"), ("src/enemies.tsx", ENEMIES), ("assets/levels/arena.bsn", ARENA)]);
    let mut app = level_app(&dir);
    let r = reload(&mut app, false);
    assert!(r.errors.is_empty(), "{:?}", r.errors);
    settle_levels(&mut app);
    assert!(errors(&app).is_empty(), "{:?}", errors(&app));
    let (a, b) = (named(&mut app, "GruntA"), named(&mut app, "GruntB"));
    // The level's Transform patch layers over the prefab's.
    assert_eq!(app.world().get::<Transform>(a).unwrap().translation, Vec3::new(-3.0, 0.0, -4.0));
    assert_eq!(health(&app, a), (100.0, 100.0));
    assert_eq!(health(&app, b), (250.0, 250.0));
    let (grunt, _) = bevy_ts::plugin_api::component_by_name(app.world(), "enemies::Grunt").unwrap();
    // SAFETY: Grunt is one u8.
    assert_eq!(unsafe { *app.world().get_by_id(b, grunt).unwrap().as_ptr() }, 1, "the component is on the instance root with its field");
    // E3: b's eye is overridden, a's is not.
    let eye = |app: &mut App, parent: Entity| {
        let world = app.world_mut();
        world.query::<(&TsxNode, &Transform, &ChildOf)>().iter(world).find(|(n, _, c)| c.parent() == parent && n.key_path.last().map(String::as_str) == Some("eye")).map(|(_, t, _)| t.translation).unwrap()
    };
    assert_eq!(eye(&mut app, b), Vec3::new(0.0, 0.8, 0.5));
    assert_eq!(eye(&mut app, a), Vec3::new(0.0, 0.6, 0.5));

    // Rule 5: a changed prop re-expands, and `initialHp` keeps its running value.
    set_hp(&mut app, b, 90.0);
    write(&dir, "assets/levels/arena.bsn", &ARENA.replace("@hp: 250.0", "@hp: 400.0"));
    app.world().resource::<AssetServer>().reload("levels/arena.bsn");
    until(&mut app, "the edited level never reloaded", |app| health(app, b).1 == 400.0);
    assert_eq!(named(&mut app, "GruntB"), b, "the same entity");
    assert_eq!(health(&app, b), (90.0, 400.0));

    // Rule 9: a new generation re-expands every instance; an unchanged one writes nothing.
    let r = reload(&mut app, false);
    assert!(r.errors.is_empty(), "{:?}", r.errors);
    assert_eq!(health(&app, b), (90.0, 400.0));
    assert_eq!(named(&mut app, "GruntB"), b);
}

#[test]
fn a_level_naming_an_unknown_prop_places_the_instance_without_it() {
    let arena = ARENA.replace("@hp: 250.0", "@speed: 2.0");
    let dir = project("levels-scene-bad", &[("package.json", "{}"), ("src/enemies.tsx", ENEMIES), ("assets/levels/arena.bsn", &arena)]);
    let mut app = level_app(&dir);
    assert!(reload(&mut app, false).errors.is_empty());
    settle_levels(&mut app);
    assert!(errors(&app).is_empty(), "{:?}", errors(&app));
    // Under `bevy-ts dev` the misfit prop is a placeholder entry, warned at its line.
    let b = named(&mut app, "GruntB");
    assert_eq!(health(&app, b), (100.0, 100.0));
    let warnings: Vec<String> = logs(&app).into_iter().filter(|e| e.level == bevy_ts::log::LogLevel::Warn).map(|e| format!("{} {}", e.location.map(|l| l.to_string()).unwrap_or_default(), e.message)).collect();
    assert!(warnings.iter().any(|m| m.starts_with("levels/arena.bsn:7:1 levels: warning: enemies::Grunt has no scene prop speed")), "{warnings:?}");
}

/// A newtype's field is `value` in TS, but a level keeps Bevy's reflection syntax (`ZIndex(2)`).
#[test]
fn a_level_holding_a_newtype_still_loads() {
    let level = "#Badge\nZIndex(2)\n";
    let dir = project("levels-newtype", &[("package.json", "{}"), ("src/enemies.tsx", ENEMIES), ("assets/levels/arena.bsn", level)]);
    let mut app = level_app(&dir);
    let r = reload(&mut app, false);
    assert!(r.errors.is_empty(), "{:?}", r.errors);
    settle_levels(&mut app);
    assert!(errors(&app).is_empty(), "{:?}", errors(&app));
    let badge = named(&mut app, "Badge");
    assert_eq!(app.world().get::<ZIndex>(badge).unwrap().0, 2);
}
