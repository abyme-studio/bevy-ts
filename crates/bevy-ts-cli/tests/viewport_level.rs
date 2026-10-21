//! Viewport edits to level-placed entities: levels routes them `bsn`, and bevy-ts writes the
//! level file itself (viewport V1 / levels D5).
#![cfg(target_os = "macos")]

mod common;

use bevy::prelude::*;
use bevy::remote::{RemoteMethodSystemId, RemoteMethods, RemotePlugin};
use bevy::window::ExitCondition;
use bevy::winit::WinitPlugin;
use bevy_ts::generation::DevSource;
use bevy_ts_cli::compile::Compiler;
use bevy_ts_levels::engine::{Levels, LevelsPlugin};
use bevy_ts_v8::V8Options;
use common::{project, reload};
use serde_json::{Value, json};
use std::path::Path;
use std::sync::Arc;

const ENEMIES: &str = r##"import { app, SceneComponent, type u8 } from "bevy-ts";
import { Transform } from "bevy-ts/bevy";
export class Grunt extends SceneComponent {
  team: u8 = 0;
  static scene() {
    return (
      <entity name="grunt">
        <Transform translation={[0, 0, -4]} />
        <mesh shape="cuboid" size={[1, 1, 1]} color="#c33" />
      </entity>
    );
  }
}
app.level("levels/arena.bsn");
"##;

const ARENA: &str = "// Two grunts.
#GruntA
@enemies::Grunt
Transform { translation: Vec3 { x: -3.0 } }
--
#GruntB
@enemies::Grunt { team: 1 }
Transform { translation: Vec3 { x: 3.0 } }
";

fn viewport_level_app(dir: &Path) -> App {
    let assets = dir.join("assets");
    let mut app = App::new();
    app.add_plugins(
        DefaultPlugins
            .build()
            .disable::<WinitPlugin>()
            .set(WindowPlugin { primary_window: None, exit_condition: ExitCondition::DontExit, ..default() })
            .set(AssetPlugin { file_path: assets.to_string_lossy().into_owned(), ..default() }),
    )
    .add_plugins(RemotePlugin::default())
    .add_plugins(bevy_ts_viewport::ViewportPlugin { windowed: false })
    .add_plugins(bevy_ts::BevyTsPlugin::new(bevy_ts_v8::factory(V8Options::default())).dev(true))
    .add_plugins(bevy_ts_tsx::TsxPlugin)
    .add_plugins(LevelsPlugin);
    app.world_mut().resource_mut::<Levels>().asset_root = assets;
    app.insert_resource(DevSource(Arc::new(Compiler::new(dir.join("src/enemies.tsx"), true).unwrap())));
    bevy_ts_remote::add_methods(&mut app);
    bevy_ts_levels::protocol::add_methods(&mut app);
    app.finish();
    app.cleanup();
    app
}

fn call(app: &mut App, method: &str, params: Value) -> Value {
    let id = match app.world().resource::<RemoteMethods>().get(method) {
        Some(RemoteMethodSystemId::Instant(id)) => *id,
        _ => panic!("{method} not served"),
    };
    app.world_mut().run_system_with(id, (!params.is_null()).then_some(params)).unwrap().unwrap_or_else(|e| panic!("{method}: {}", e.message))
}

fn edits(app: &mut App, key: u64) -> Vec<Value> {
    let id = match app.world().resource::<RemoteMethods>().get("bevy-ts/viewport_edits+watch") {
        Some(RemoteMethodSystemId::Watching(id)) => *id,
        _ => panic!("no edits watch"),
    };
    let v = app.world_mut().run_system_with(id, Some(json!({ "k": key }))).unwrap().unwrap().unwrap();
    v["edits"].as_array().unwrap().clone()
}

fn named(app: &mut App, name: &str) -> Entity {
    let world = app.world_mut();
    world.query::<(Entity, &Name)>().iter(world).find(|(_, n)| n.as_str() == name).map(|(e, _)| e).unwrap_or_else(|| panic!("no {name}"))
}

fn settle(app: &mut App) {
    for _ in 0..300 {
        app.update();
        let levels = app.world().resource::<Levels>();
        if !levels.roots.is_empty() && levels.roots.values().all(|r| !r.dirty && !r.applied.is_empty()) {
            return;
        }
        std::thread::sleep(std::time::Duration::from_millis(2));
    }
    panic!("the level never loaded");
}

#[test]
fn an_edit_to_a_level_placed_grunt_is_written_to_its_level() {
    let dir = project("viewport-level", &[("src/enemies.tsx", ENEMIES), ("assets/levels/arena.bsn", ARENA)]);
    let mut app = viewport_level_app(&dir);
    assert_eq!(reload(&mut app, false).status, bevy_ts::generation::ReloadStatus::Applied);
    settle(&mut app);
    call(&mut app, "bevy-ts/viewport_mode", json!({ "mode": "edit" }));
    app.update();
    let b = named(&mut app, "GruntB");
    let sel = call(&mut app, "bevy-ts/viewport_select", json!({ "entities": [b.to_bits()] }));
    let route = &sel["items"][0]["routes"]["translation"];
    assert_eq!(route["kind"], "bsn", "{sel:#}");
    assert_eq!(route["target"]["file"], "levels/arena.bsn");
    assert_eq!(route["namePath"], json!(["GruntB"]));

    // What a drag along x leaves behind: the live write, then one edit on release.
    let before = *app.world().get::<Transform>(b).unwrap();
    let mut after = before;
    after.translation.x = 5.0;
    *app.world_mut().get_mut::<Transform>(b).unwrap() = after;
    let id = bevy_ts_viewport::edits::record(app.world_mut(), b, "translation", before, after);
    for _ in 0..3 {
        app.update();
    }
    let e = edits(&mut app, 1);
    let edit = e.iter().find(|x| x["id"] == id).unwrap();
    assert_eq!(edit["route"]["kind"], "bsn");
    assert_eq!(edit["status"], "written", "{edit:#}");
    let file = std::fs::read_to_string(dir.join("assets/levels/arena.bsn")).unwrap();
    assert!(file.contains("x: 5.0") && file.contains("x: -3.0"), "only b's x changed:\n{file}");
    settle(&mut app);
    let b = named(&mut app, "GruntB");
    assert_eq!(app.world().get::<Transform>(b).unwrap().translation.x, 5.0, "no jump after the level reloads");
}

#[test]
fn a_dropped_gltf_is_written_as_a_world_asset_root_and_composes() {
    let model = std::fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/arena/assets/models/crate.gltf")).unwrap();
    let dir = project("viewport-level-drop", &[("src/enemies.tsx", ENEMIES), ("assets/levels/arena.bsn", ARENA), ("assets/models/crate.gltf", &model)]);
    let mut app = viewport_level_app(&dir);
    assert_eq!(reload(&mut app, false).status, bevy_ts::generation::ReloadStatus::Applied);
    settle(&mut app);
    let base = call(&mut app, "bevy-ts/level_list", Value::Null)["levels"][0]["hash"].clone();
    let w = call(&mut app, "bevy-ts/level_write", json!({ "file": "levels/arena.bsn", "base": base, "ops": [{ "op": "add_entity", "parent": [], "asset": "models/crate.gltf" }] }));
    assert_eq!(w["status"], "written", "{w:#}");
    let file = std::fs::read_to_string(dir.join("assets/levels/arena.bsn")).unwrap();
    assert!(file.contains("#crate\nWorldAssetRoot(\"models/crate.gltf#Scene0\")"), "{file}");
    // The write reconciles the level: the crate spawns.
    for _ in 0..500 {
        app.update();
        let world = app.world_mut();
        if world.query::<&Name>().iter(world).any(|n| n.as_str() == "crate") {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(2));
    }
    let errors: Vec<String> = common::logs(&app).into_iter().filter(|e| e.level == bevy_ts::log::LogLevel::Error).map(|e| e.message).collect();
    assert!(errors.is_empty(), "{errors:?}");
    let krate = named(&mut app, "crate");
    assert!(app.world().get::<bevy::world_serialization::WorldAssetRoot>(krate).is_some(), "the crate has its scene root");
    // The glTF's scene spawns under it: a mesh descends from the crate.
    let mut meshes = false;
    for _ in 0..500 {
        app.update();
        let world = app.world_mut();
        meshes = world.query_filtered::<Entity, With<Mesh3d>>().iter(world).any(|m| std::iter::successors(Some(m), |e| world.get::<ChildOf>(*e).map(ChildOf::parent)).any(|e| e == krate));
        if meshes {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(2));
    }
    assert!(meshes, "the crate's mesh spawned under it");
}
