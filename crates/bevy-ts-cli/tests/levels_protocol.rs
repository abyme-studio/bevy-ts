//! `bevy-ts/level_*` against the real engine, every answer validated against
//! `protocol/bevy-ts.openrpc.json`.

mod common;

use bevy::prelude::*;
use bevy::remote::{BrpError, RemoteMethodSystemId, RemoteMethods, RemotePlugin};
use bevy_ts_levels::engine::{Levels, LevelsPlugin};
use common::*;
use serde_json::{Value, json};
use std::path::Path;

fn contract() -> Value {
    serde_json::from_str(bevy_ts_remote::OPENRPC).unwrap()
}

fn rewrite(v: &mut Value) {
    match v {
        Value::Object(o) => {
            if let Some(Value::String(r)) = o.get_mut("$ref") {
                *r = r.replace("#/components/schemas/", "#/definitions/");
            }
            o.values_mut().for_each(rewrite);
        }
        Value::Array(a) => a.iter_mut().for_each(rewrite),
        _ => {}
    }
}

fn validate(method: &str, result: &Value) {
    let doc = contract();
    let m = doc["methods"].as_array().unwrap().iter().find(|m| m["name"] == method).unwrap_or_else(|| panic!("{method} not in the contract"));
    assert_eq!(m["x-bevy-ts-crate"], "bevy-ts-levels");
    let mut schema = m["result"]["schema"].clone();
    let mut defs = doc["components"]["schemas"].clone();
    rewrite(&mut schema);
    rewrite(&mut defs);
    let mut root = json!({ "$schema": "http://json-schema.org/draft-07/schema#", "definitions": defs });
    if let Value::Object(s) = schema {
        root.as_object_mut().unwrap().extend(s);
    }
    let v = jsonschema::draft7::new(&root).unwrap_or_else(|e| panic!("{method}: bad schema: {e}"));
    let errors: Vec<String> = v.iter_errors(result).map(|e| format!("{} at {}", e, e.instance_path)).collect();
    assert!(errors.is_empty(), "{method} answered off-contract: {errors:?}\n{result:#}");
}

fn call(app: &mut App, method: &str, params: Option<Value>) -> Result<Option<Value>, BrpError> {
    let id = *app.world().resource::<RemoteMethods>().get(method).unwrap_or_else(|| panic!("{method} not served"));
    let world = app.world_mut();
    match id {
        RemoteMethodSystemId::Instant(id) => world.run_system_with(id, params).unwrap().map(Some),
        RemoteMethodSystemId::Watching(id) => world.run_system_with(id, params).unwrap(),
    }
}

fn ok(app: &mut App, method: &str, params: Option<Value>) -> Value {
    let v = call(app, method, params).unwrap_or_else(|e| panic!("{method}: {e:?}")).unwrap_or_else(|| panic!("{method}: no message"));
    validate(method, &v);
    v
}

const ENEMIES: &str = r#"import { app, Component, SceneComponent, type f32, type u8 } from "bevy-ts";
import { Transform } from "bevy-ts/bevy";
export class Health extends Component { hp: f32 = 100; max: f32 = 100 }
export class Grunt extends SceneComponent {
  team: u8 = 0;
  static scene({ hp = 100, speed = 2 }: { hp?: f32; speed?: f32 }) {
    return (
      <entity name="grunt">
        <Health initialHp={hp} max={hp} />
        <entity key="eye">
          <Transform translation={[0, 0.6, 0.5]} />
        </entity>
      </entity>
    );
  }
}
app.level("levels/arena.bsn");
"#;

const ARENA: &str = "// The arena.
#GruntB
@enemies::Grunt { @hp: 250.0 }
Transform { translation: Vec3 { x: 3.0 } }
Children [
    #eye
    Transform { translation: Vec3 { y: 0.8 } }
]
--
#RoomA
:\"levels/room.bsn\"
Children [
    #Lamp
    Transform { translation: Vec3 { y: 3.0 } }
]
";

const ROOM: &str = "#Room
Transform
Children [
    #Lamp
    Transform { translation: Vec3 { y: 2.0 } }
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
        .add_plugins(LevelsPlugin)
        .add_plugins(RemotePlugin::default());
    bevy_ts_levels::protocol::add_methods(&mut app);
    app.world_mut().resource_mut::<Levels>().asset_root = assets;
    app.insert_resource(bevy_ts::generation::DevSource(std::sync::Arc::new(bevy_ts_cli::compile::Compiler::new(dir.join("src/enemies.tsx"), true).unwrap())));
    app
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
    panic!("the level never loaded: {:?}", messages(app));
}

#[test]
fn every_level_method_answers_on_contract() {
    let dir = project("levels-protocol", &[("package.json", "{}"), ("src/enemies.tsx", ENEMIES), ("assets/levels/arena.bsn", ARENA), ("assets/levels/room.bsn", ROOM)]);
    let mut app = level_app(&dir);
    assert!(reload(&mut app, false).errors.is_empty());
    settle(&mut app);
    let arena = || std::fs::read_to_string(dir.join("assets/levels/arena.bsn")).unwrap();
    let room = || std::fs::read_to_string(dir.join("assets/levels/room.bsn")).unwrap();

    let first = ok(&mut app, "bevy-ts/level+watch", None);
    assert_eq!(first["event"], "loaded");

    let list = ok(&mut app, "bevy-ts/level_list", None);
    let l = &list["levels"][0];
    assert_eq!(l["path"], "levels/arena.bsn");
    assert_eq!(l["files"], json!(["levels/arena.bsn", "levels/room.bsn"]));
    let codes: Vec<&str> = l["extensions"].as_array().unwrap().iter().map(|e| e["code"].as_str().unwrap()).collect();
    assert_eq!(codes, ["E1", "E2", "E3", "E3"]);
    let hash = l["hash"].as_str().unwrap().to_string();

    let node = ok(&mut app, "bevy-ts/level_node", Some(json!({ "level": "levels/arena.bsn", "namePath": ["GruntB"] })));
    assert_eq!(node["prefab"], "enemies::Grunt");
    assert_eq!(node["overrides"][0]["component"], "Transform");
    assert_eq!(node["overrides"][0]["location"]["line"], 4);
    // The scene component's @props beside its fields: what the level sets, and the defaults.
    let props = node["props"].as_array().expect("props");
    assert_eq!(props.len(), 2, "{node}");
    assert_eq!((&props[0]["name"], &props[0]["value"], &props[0]["overridden"], &props[0]["type"]), (&json!("hp"), &json!(250.0), &json!(true), &json!({ "kind": "f32" })), "{node}");
    assert_eq!((&props[0]["location"]["file"], &props[0]["location"]["line"]), (&json!("levels/arena.bsn"), &json!(3)), "{node}");
    assert_eq!((&props[1]["name"], &props[1]["value"], &props[1]["overridden"]), (&json!("speed"), &json!(2), &json!(false)), "{node}");
    assert!(props[1].get("location").is_none_or(Value::is_null), "{node}");

    // A write, then its inverse.
    let w = ok(&mut app, "bevy-ts/level_write", Some(json!({
        "id": 3, "file": "levels/arena.bsn", "base": hash,
        "ops": [{ "op": "set", "path": ["GruntB"], "component": "Transform", "field": ["translation", "x"], "value": 5.0, "containers": ["Vec3"] }],
    })));
    assert_eq!((w["status"].as_str(), w["lines"].clone()), (Some("written"), json!([[4, 4]])));
    assert_eq!(arena(), ARENA.replace("x: 3.0", "x: 5.0"));
    // The write's reconcile reports `loaded`, then the write itself.
    let written = std::iter::from_fn(|| call(&mut app, "bevy-ts/level+watch", None).unwrap()).inspect(|m| validate("bevy-ts/level+watch", m)).find(|m| m["event"] == "written").expect("written");
    assert_eq!((written["event"].as_str(), written["id"].as_u64()), (Some("written"), Some(3)));
    let undo = ok(&mut app, "bevy-ts/level_write", Some(json!({ "id": 4, "file": "levels/arena.bsn", "base": w["hash"], "ops": w["inverse"] })));
    assert_eq!(undo["status"], "written", "{undo}");
    assert_eq!(arena(), ARENA);
    // A stale base whose field changed: a conflict.
    let c = ok(&mut app, "bevy-ts/level_write", Some(json!({
        "id": 5, "file": "levels/arena.bsn", "base": w["hash"],
        "ops": [{ "op": "set", "path": ["GruntB"], "component": "Transform", "field": ["translation", "x"], "value": 6.0, "containers": ["Vec3"] }],
    })));
    assert_eq!((c["status"].as_str(), c["reason"].as_str(), c["disk"].as_str()), (Some("conflict"), Some("changed on disk"), Some("3.0")), "{c}");

    // Apply into the included room: one transaction across two files.
    let lamp = {
        let world = app.world_mut();
        world.query::<(Entity, &Name)>().iter(world).find(|(_, n)| n.as_str() == "Lamp").map(|(e, _)| e).unwrap()
    };
    let a = ok(&mut app, "bevy-ts/level_apply", Some(json!({ "entity": lamp.to_bits(), "component": "Transform", "field": "translation" })));
    assert_eq!(a["status"], "written", "{a}");
    assert!(room().contains("Transform { translation: Vec3 { y: 3.0, x: 0.0, z: 0.0 } }"), "{}", room());
    assert!(!arena().contains("y: 3.0"), "{}", arena());
    assert_eq!(app.world().get::<Transform>(lamp).unwrap().translation.y, 3.0, "the lamp stays where it is");

    // Apply to the prefab: TSX is next, so the editor gets the edit and the batch that follows.
    let eye = ok(&mut app, "bevy-ts/level_node", Some(json!({ "level": "levels/arena.bsn", "namePath": ["GruntB", "eye"] })));
    let t = ok(&mut app, "bevy-ts/level_apply", Some(json!({ "entity": eye["entity"], "component": "Transform", "field": "translation" })));
    assert_eq!(t["status"], "tsx", "{t}");
    assert!(t["edit"]["element"]["file"].as_str().unwrap().ends_with("enemies.tsx"), "{t}");
    // The element that sets `translation`, `<Transform>`, not the eye's `<entity>` above it.
    assert_eq!((t["edit"]["element"]["line"].as_u64(), t["edit"]["prop"].as_str()), (Some(11), Some("translation")), "{t}");
    assert_eq!(t["edit"]["chain"][0], t["edit"]["element"], "{t}");
    assert_eq!(t["then"]["ops"][0]["op"], "revert");

    // Apply an @prop: TSX is next, so the edit is the scene's destructured default.
    let grunt = ok(&mut app, "bevy-ts/level_node", Some(json!({ "level": "levels/arena.bsn", "namePath": ["GruntB"] })));
    let a = ok(&mut app, "bevy-ts/level_apply", Some(json!({ "entity": grunt["entity"], "component": "enemies::Grunt", "field": "@hp", "id": 9 })));
    assert_eq!(a["status"], "tsx", "{a}");
    assert_eq!((a["edit"]["target"].as_str(), a["edit"]["prop"].as_str(), &a["edit"]["value"]), (Some("default"), Some("hp"), &json!(250.0)), "{a}");
    let site = a["edit"]["element"]["file"].as_str().unwrap();
    assert!(site.starts_with('/') && site.ends_with("enemies.tsx"), "an absolute path, like every other location: {a}");
    assert_eq!((a["edit"]["element"]["line"].as_u64(), a["edit"]["element"]["column"].as_u64()), (Some(6), Some(18)), "{a}");
    assert_eq!(a["then"]["ops"], json!([{ "op": "revert_prop", "path": ["GruntB"], "prop": "hp", "field": false }]), "{a}");
    // The editor wrote the default; the batch removes the override.
    let w = ok(&mut app, "bevy-ts/level_write", Some(a["then"].clone()));
    assert_eq!(w["status"], "written", "{w}");
    assert!(arena().contains("#GruntB\n@enemies::Grunt\nTransform"), "{}", arena());
    assert_eq!(w["inverse"][0]["op"], "set_prop", "{w}");
    settle(&mut app);
    let after = ok(&mut app, "bevy-ts/level_node", Some(json!({ "level": "levels/arena.bsn", "namePath": ["GruntB"] })));
    assert_eq!((&after["props"][0]["value"], &after["props"][0]["overridden"]), (&json!(100), &json!(false)), "{after}");
    // A prop nothing overrides: nothing to apply.
    let none = ok(&mut app, "bevy-ts/level_apply", Some(json!({ "entity": grunt["entity"], "component": "enemies::Grunt", "field": "@speed" })));
    assert_eq!((none["status"].as_str(), none["reason"].as_str()), (Some("refused"), Some("no override")), "{none}");
}
