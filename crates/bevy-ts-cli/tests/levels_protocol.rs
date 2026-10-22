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
    validate_in("bevy-ts-levels", method, result);
}

fn validate_tsx(message: &Value) {
    validate_in("bevy-ts-tsx", "bevy-ts/tsx_tree+watch", message);
}

fn validate_in(krate: &str, method: &str, result: &Value) {
    let doc = contract();
    let m = doc["methods"].as_array().unwrap().iter().find(|m| m["name"] == method).unwrap_or_else(|| panic!("{method} not in the contract"));
    assert_eq!(m["x-bevy-ts-crate"], krate);
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

#[test]
fn a_level_opens_and_closes_over_the_wire() {
    let dir = project("levels-open", &[("package.json", "{}"), ("src/enemies.tsx", ENEMIES), ("assets/levels/arena.bsn", ARENA), ("assets/levels/room.bsn", ROOM)]);
    let mut app = level_app(&dir);
    assert!(reload(&mut app, false).errors.is_empty());
    settle(&mut app);
    ok(&mut app, "bevy-ts/level+watch", None);
    assert_eq!(ok(&mut app, "bevy-ts/level_list", None)["levels"][0]["opener"], "ts");

    let o = ok(&mut app, "bevy-ts/level_open", Some(json!({ "file": "levels/room.bsn" })));
    assert_eq!((o["status"].as_str(), o["level"]["path"].as_str(), o["level"]["opener"].as_str()), (Some("opened"), Some("levels/room.bsn"), Some("editor")), "{o}");
    let lamp = ok(&mut app, "bevy-ts/level_node", Some(json!({ "level": "levels/room.bsn", "namePath": ["Room", "Lamp"] })));
    assert_eq!(lamp["overrides"][0]["component"], "Transform", "{lamp}");
    for (file, reason) in [("levels/nope.bsn", "not-found"), ("../outside.bsn", "invalid")] {
        let f = ok(&mut app, "bevy-ts/level_open", Some(json!({ "file": file })));
        assert_eq!((f["status"].as_str(), f["reason"].as_str()), (Some("failed"), Some(reason)), "{f}");
    }
    let c = ok(&mut app, "bevy-ts/level_close", Some(json!({ "file": "levels/room.bsn" })));
    assert_eq!(c, json!({ "closed": true }));
    assert_eq!(ok(&mut app, "bevy-ts/level_close", Some(json!({ "file": "levels/room.bsn" }))), json!({ "closed": false }));
    let events: Vec<String> = std::iter::from_fn(|| call(&mut app, "bevy-ts/level+watch", None).unwrap())
        .inspect(|m| validate("bevy-ts/level+watch", m))
        .map(|m| format!("{} {}", m["event"].as_str().unwrap(), m["level"].as_str().unwrap()))
        .collect();
    assert_eq!(events, ["opened levels/room.bsn", "loaded levels/room.bsn", "closed levels/room.bsn"]);
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

/// The arena example (`examples/arena`) as a fresh project, `extra` appended to its level, with
/// the tsx and level methods served.
fn arena_app(name: &str, extra: &str) -> App {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/arena");
    let mut files: Vec<(String, String)> = ["enemy.tsx", "player.tsx", "hud.tsx", "game.tsx"].iter().map(|f| (format!("src/{f}"), std::fs::read_to_string(root.join("src").join(f)).unwrap())).collect();
    for f in ["arena.bsn", "room.bsn"] {
        let mut src = std::fs::read_to_string(root.join("assets/levels").join(f)).unwrap();
        if f == "arena.bsn" {
            src.push_str(extra);
        }
        files.push((format!("assets/levels/{f}"), src));
    }
    let mut list: Vec<(&str, &str)> = files.iter().map(|(a, b)| (a.as_str(), b.as_str())).collect();
    list.push(("package.json", "{}"));
    let dir = project(name, &list);
    let mut app = app(&dir.join("src/game.tsx"));
    let assets = dir.join("assets").canonicalize().unwrap();
    app.add_plugins((AssetPlugin { file_path: assets.to_string_lossy().into_owned(), ..Default::default() }, RemotePlugin::default()))
        .init_asset::<Mesh>()
        .init_asset::<StandardMaterial>()
        .add_plugins(LevelsPlugin);
    app.world_mut().resource_mut::<Levels>().asset_root = assets;
    bevy_ts_tsx::protocol::add_methods(&mut app);
    bevy_ts_levels::protocol::add_methods(&mut app);
    assert!(reload(&mut app, false).errors.is_empty());
    settle(&mut app);
    app
}

fn row<'a>(tree: &'a Value, root: &str, key_path: &[&str]) -> &'a Value {
    let r = tree["roots"].as_array().unwrap().iter().find(|r| r["name"] == root).unwrap_or_else(|| panic!("no root {root}"));
    r["nodes"].as_array().unwrap().iter().find(|n| n["keyPath"] == json!(key_path)).unwrap_or_else(|| panic!("no row {key_path:?} in {root}"))
}

#[test]
fn arena_tree_rows_say_their_level() {
    let mut app = arena_app("levels-tree-rows", "--\n#Spot\nTransform { translation: Vec3 { y: 1.0 } }\n");
    let tree = call(&mut app, "bevy-ts/tsx_tree", None).unwrap().unwrap();
    let arena = "level:levels/arena.bsn";
    // Written in the file: a placed instance, an inclusion, a plain entity.
    assert_eq!(row(&tree, arena, &["EnemyB"])["level"], json!({ "file": "levels/arena.bsn", "namePath": ["EnemyB"], "written": true }));
    assert_eq!(row(&tree, arena, &["RoomA"])["level"], json!({ "file": "levels/arena.bsn", "namePath": ["RoomA"], "written": true }));
    assert_eq!(row(&tree, arena, &["Spot"])["level"], json!({ "file": "levels/arena.bsn", "namePath": ["Spot"], "written": true }));
    // Not written there: the prefab's eye, and the included room's guard and its eye.
    assert_eq!(row(&tree, arena, &["EnemyB", "eye"])["level"], json!({ "file": "levels/arena.bsn", "namePath": ["EnemyB", "eye"], "written": false }));
    assert_eq!(row(&tree, arena, &["RoomA", "Guard"])["level"], json!({ "file": "levels/arena.bsn", "namePath": ["RoomA", "Guard"], "written": false }));
    assert_eq!(row(&tree, arena, &["RoomA", "Guard", "eye"])["level"], json!({ "file": "levels/arena.bsn", "namePath": ["RoomA", "Guard", "eye"], "written": false }));
    // The TSX scene: its root names its sidecar (none on disk), its rows are TSX-only.
    let scene = tree["roots"].as_array().unwrap().iter().find(|r| r["name"] == "arena").expect("the arena scene");
    assert_eq!(scene["sidecar"], "levels/arena.bsn", "{scene}");
    assert!(scene["nodes"].as_array().unwrap().iter().all(|n| n.get("level").is_none()), "{scene}");
    assert!(tree["roots"].as_array().unwrap().iter().find(|r| r["name"] == arena).unwrap().get("sidecar").is_none(), "a level root is no app.scene root");
    // The watch's snapshot carries the same rows.
    let snapshot = call(&mut app, "bevy-ts/tsx_tree+watch", None).unwrap().unwrap();
    let watched = json!({ "roots": snapshot["snapshot"] });
    assert_eq!(row(&watched, arena, &["EnemyB", "eye"])["level"], row(&tree, arena, &["EnemyB", "eye"])["level"]);

    // `patches`: what the file adds that no prefab gives.
    let b = call(&mut app, "bevy-ts/level_node", Some(json!({ "level": "levels/arena.bsn", "namePath": ["EnemyB"] }))).unwrap().unwrap();
    assert_eq!(b["patches"], json!([]), "{b}");
    let spot = call(&mut app, "bevy-ts/level_node", Some(json!({ "level": "levels/arena.bsn", "namePath": ["Spot"] }))).unwrap().unwrap();
    assert_eq!(spot["patches"], json!(["Transform"]), "{spot}");
    let room = call(&mut app, "bevy-ts/level_node", Some(json!({ "level": "levels/arena.bsn", "namePath": ["RoomA"] }))).unwrap().unwrap();
    assert_eq!(room["patches"], json!([]), "room.bsn gives its Transform: {room}");
}

#[test]
fn a_level_tree_lists_its_nodes_in_file_order() {
    // Crate comes last in the file, and its children Zed then Bolt: not alphabetical.
    let mut app = arena_app("levels-tree-order", "--\n#Crate\nTransform\nChildren [\n    #Zed\n    Transform\n    --\n    #Bolt\n    Transform\n]\n");
    let order = |v: &Value| -> Vec<Vec<String>> {
        let r = v["roots"].as_array().unwrap().iter().find(|r| r["name"] == "level:levels/arena.bsn").expect("the arena level");
        r["nodes"].as_array().unwrap().iter().map(|n| serde_json::from_value(n["keyPath"].clone()).unwrap()).collect()
    };
    let tree = call(&mut app, "bevy-ts/tsx_tree", None).unwrap().unwrap();
    let paths = order(&tree);
    let top: Vec<&str> = paths.iter().filter(|p| p.len() == 1).map(|p| p[0].as_str()).collect();
    assert_eq!(top, ["EnemyA", "EnemyB", "RoomA", "RoomB", "Crate"], "{paths:?}");
    let at = |k: &[&str]| paths.iter().position(|p| p == k).unwrap_or_else(|| panic!("no {k:?} in {paths:?}"));
    assert!(at(&["Crate"]) < at(&["Crate", "Zed"]) && at(&["Crate", "Zed"]) < at(&["Crate", "Bolt"]), "{paths:?}");
    // Children follow their parent, before its next sibling.
    assert_eq!(at(&["RoomA", "Guard"]), at(&["RoomA"]) + 1, "{paths:?}");
    // The watch's snapshot lists them the same way.
    let snapshot = call(&mut app, "bevy-ts/tsx_tree+watch", None).unwrap().unwrap();
    assert_eq!(order(&json!({ "roots": snapshot["snapshot"] })), paths);
}

#[test]
fn the_arena_catalog_lists_its_scene_components() {
    let mut app = arena_app("levels-catalog", "");
    let catalog = ok(&mut app, "bevy-ts/level_catalog", None);
    let scenes = catalog["scenes"].as_array().unwrap();
    assert_eq!(scenes.iter().map(|s| s["name"].as_str().unwrap()).collect::<Vec<_>>(), ["enemy::Enemy"], "{catalog:#}");
    let enemy = &scenes[0];
    assert!(enemy["file"].as_str().unwrap().ends_with("src/enemy.tsx"), "{enemy}");
    assert_eq!(enemy["line"], 13, "{enemy}");
    let props = enemy["props"].as_array().unwrap();
    assert_eq!(props.iter().map(|p| p["name"].as_str().unwrap()).collect::<Vec<_>>(), ["hp", "at"], "{enemy}");
    assert_eq!((&props[0]["type"], &props[0]["default"], &props[0]["required"]), (&json!({ "kind": "f32" }), &json!(100), &json!(false)), "{enemy}");
    assert_eq!(props[1]["required"], false, "{enemy}");
}

const ORDER: &str = "#A
Transform
Children [
    #a1
    Transform
    --
    #a2
    Transform
]
--
#B
Transform
";

/// What a watching client holds: each root's rows, and each parent's children in order.
#[derive(Debug, Default, PartialEq)]
struct Client {
    rows: std::collections::BTreeMap<String, Value>,
    /// By root and parent (its JSON text: a node id or null).
    lists: std::collections::BTreeMap<(String, String), Vec<Value>>,
}

impl Client {
    fn from_roots(roots: &Value) -> Client {
        let mut c = Client::default();
        for r in roots.as_array().unwrap() {
            for n in r["nodes"].as_array().unwrap() {
                c.rows.insert(n["node"].as_str().unwrap().to_string(), n["keyPath"].clone());
                c.lists.entry((r["name"].as_str().unwrap().to_string(), n["parent"].to_string())).or_default().push(n["node"].clone());
            }
        }
        c
    }

    fn apply(&mut self, m: &Value) {
        for n in m["added"].as_array().into_iter().flatten() {
            self.rows.insert(n["node"].as_str().unwrap().to_string(), n["keyPath"].clone());
        }
        for id in m["removed"].as_array().into_iter().flatten() {
            self.rows.remove(id.as_str().unwrap());
            let key = id.to_string();
            self.lists.retain(|(_, p), _| *p != key);
            self.lists.values_mut().for_each(|l| l.retain(|c| c != id));
        }
        for o in m["ordered"].as_array().into_iter().flatten() {
            self.lists.insert((o["root"].as_str().unwrap().to_string(), o["parent"].to_string()), o["children"].as_array().unwrap().clone());
        }
        self.lists.retain(|_, l| !l.is_empty());
    }
}

/// Each level root's file hashes as of its last apply.
fn applied(app: &App) -> Vec<std::collections::BTreeMap<String, u64>> {
    app.world().resource::<Levels>().roots.values().map(|r| r.applied.clone()).collect()
}

/// After a write: runs frames, polling the watch each one so it stays subscribed, until the level
/// has applied the change (its file hashes move from `last`, which for a file saved outside waits
/// on the asset watcher), settled and gone quiet; returns the messages sent and updates `last`.
#[track_caller]
fn watch_until_settled(app: &mut App, last: &mut Vec<std::collections::BTreeMap<String, u64>>) -> Vec<Value> {
    let mut out = Vec::new();
    let mut quiet = 0;
    for _ in 0..300 {
        app.update();
        quiet += 1;
        if let Some(m) = call(app, "bevy-ts/tsx_tree+watch", None).unwrap() {
            assert!(m.get("snapshot").is_none(), "the watch resubscribed: {m}");
            out.push(m);
            quiet = 0;
        }
        // The watch can trail the apply by a frame: done once a few frames bring nothing more.
        let settled = app.world().resource::<Levels>().roots.values().all(|r| !r.dirty);
        if settled && applied(app) != *last && !out.is_empty() && quiet >= 5 {
            *last = applied(app);
            return out;
        }
        std::thread::sleep(std::time::Duration::from_millis(2));
    }
    panic!("no watch message: {:?}", messages(app));
}

#[test]
fn watch_deltas_keep_a_client_in_tsx_tree_order() {
    let dir = project("levels-watch-order", &[("package.json", "{}"), ("src/enemies.tsx", ENEMIES), ("assets/levels/arena.bsn", ORDER)]);
    let mut app = level_app(&dir);
    bevy_ts_tsx::protocol::add_methods(&mut app);
    assert!(reload(&mut app, false).errors.is_empty());
    settle(&mut app);
    let level = "level:levels/arena.bsn";
    let tree = |app: &mut App| call(app, "bevy-ts/tsx_tree", None).unwrap().unwrap();
    let id = |t: &Value, k: &[&str]| row(t, level, k)["node"].clone();
    let first = call(&mut app, "bevy-ts/tsx_tree+watch", None).unwrap().unwrap();
    validate_tsx(&first);
    let mut client = Client::from_roots(&first["snapshot"]);
    let mut last = applied(&app);
    let t = tree(&mut app);
    assert_eq!(client, Client::from_roots(&t["roots"]));
    let (a, b, a1, a2) = (id(&t, &["A"]), id(&t, &["B"]), id(&t, &["A", "a1"]), id(&t, &["A", "a2"]));
    assert_eq!((&row(&t, level, &["A", "a1"])["parent"], &row(&t, level, &["A"])["parent"]), (&a, &Value::Null));
    assert_eq!(row(&t, level, &["A"])["root"], level);
    let lists = |ms: &[Value]| -> Vec<Value> { ms.iter().flat_map(|m| m["ordered"].as_array().cloned().unwrap_or_default()).collect() };
    let has = |ms: &[Value], key: &str| ms.iter().any(|m| m.get(key).is_some());

    // A move within the file: B before A, an order change only.
    std::fs::write(dir.join("assets/levels/arena.bsn"), ORDER.replace("#A\n", "#TMP\n").replace("--\n#B\nTransform\n", "").replacen("#TMP\n", "#B\nTransform\n--\n#A\n", 1)).unwrap();
    let ms = watch_until_settled(&mut app, &mut last);
    ms.iter().for_each(validate_tsx);
    assert!(!has(&ms, "added") && !has(&ms, "removed"), "{ms:?}");
    assert!(lists(&ms).contains(&json!({ "root": level, "parent": null, "children": [b, a] })), "{ms:?}");
    ms.iter().for_each(|m| client.apply(m));
    assert_eq!(client, Client::from_roots(&tree(&mut app)["roots"]));

    // An add between siblings: C at index 1 under A.
    let base = ok(&mut app, "bevy-ts/level_list", None)["levels"][0]["hash"].clone();
    let w = ok(&mut app, "bevy-ts/level_write", Some(json!({ "file": "levels/arena.bsn", "base": base, "ops": [{ "op": "restore", "parent": ["A"], "index": 1, "text": "#C\nTransform" }] })));
    assert_eq!(w["status"], "written", "{w}");
    let ms = watch_until_settled(&mut app, &mut last);
    ms.iter().for_each(validate_tsx);
    let t = tree(&mut app);
    let c = id(&t, &["A", "C"]);
    let added: Vec<Value> = ms.iter().flat_map(|m| m["added"].as_array().cloned().unwrap_or_default()).collect();
    assert!(added.iter().any(|r| r["node"] == c && r["parent"] == a), "{ms:?}");
    assert!(lists(&ms).contains(&json!({ "root": level, "parent": a, "children": [a1, c, a2] })), "{ms:?}");
    ms.iter().for_each(|m| client.apply(m));
    assert_eq!(client, Client::from_roots(&t["roots"]));

    // A reparent: a2 from A to B.
    let w = ok(&mut app, "bevy-ts/level_write", Some(json!({ "file": "levels/arena.bsn", "base": w["hash"], "ops": [{ "op": "reparent", "path": ["A", "a2"], "parent": ["B"] }] })));
    assert_eq!(w["status"], "written", "{w}");
    let ms = watch_until_settled(&mut app, &mut last);
    ms.iter().for_each(validate_tsx);
    let t = tree(&mut app);
    let parents: Vec<Value> = lists(&ms).iter().map(|l| l["parent"].clone()).collect();
    assert!(parents.contains(&a) && parents.contains(&b), "{ms:?}");
    ms.iter().for_each(|m| client.apply(m));
    assert_eq!(client, Client::from_roots(&t["roots"]));
    assert_eq!(row(&t, level, &["B", "a2"])["parent"], b);

    // A rename of a node with children: its children's list comes under its new id.
    let w = ok(&mut app, "bevy-ts/level_write", Some(json!({ "file": "levels/arena.bsn", "base": w["hash"], "ops": [{ "op": "rename", "path": ["A"], "to": "A2" }] })));
    assert_eq!(w["status"], "written", "{w}");
    let ms = watch_until_settled(&mut app, &mut last);
    ms.iter().for_each(validate_tsx);
    let t = tree(&mut app);
    ms.iter().for_each(|m| client.apply(m));
    assert_eq!(client, Client::from_roots(&t["roots"]));
    let (a, a1, c) = (id(&t, &["A2"]), id(&t, &["A2", "a1"]), id(&t, &["A2", "C"]));

    // Removing a parent: its whole subtree is removed, and its list is not sent.
    let w = ok(&mut app, "bevy-ts/level_write", Some(json!({ "file": "levels/arena.bsn", "base": w["hash"], "ops": [{ "op": "delete", "path": ["A2"] }] })));
    assert_eq!(w["status"], "written", "{w}");
    let ms = watch_until_settled(&mut app, &mut last);
    ms.iter().for_each(validate_tsx);
    let removed: Vec<Value> = ms.iter().flat_map(|m| m["removed"].as_array().cloned().unwrap_or_default()).collect();
    for n in [&a, &a1, &c] {
        assert!(removed.contains(n), "{n} not removed: {ms:?}");
    }
    assert!(lists(&ms).iter().all(|l| l["parent"] != a), "{ms:?}");
    ms.iter().for_each(|m| client.apply(m));
    assert_eq!(client, Client::from_roots(&tree(&mut app)["roots"]));
}

/// level-placeholders: a boss whose class is deleted, a grunt whose scene throws, and a prop
/// that no longer fits, over the wire; then the boss's class comes back.
const SQUAD: &str = r#"import { app, Component, SceneComponent, type f32 } from "bevy-ts";
import { Transform } from "bevy-ts/bevy";
export class Health extends Component { hp: f32 = 100; max: f32 = 100 }
export class Grunt extends SceneComponent {
  static scene({ hp = 100 }: { hp?: f32 }) {
    if (hp > 1000) throw new Error(`no grunt has ${hp} hp`);
    return <entity name="grunt"><Health initialHp={hp} max={hp} /></entity>;
  }
}
export class Hat extends SceneComponent {
  static scene() {
    return <entity name="hat"><Transform translation={[0, 1, 0]} /></entity>;
  }
}
//BOSS
app.level("levels/squad.bsn");
"#;

const BOSS: &str = r#"export class Boss extends SceneComponent {
  static scene({ hp = 500 }: { hp?: f32 }) {
    return <entity name="boss"><Health initialHp={hp} max={hp} /><entity key="eye"><Transform translation={[0, 0.5, 0]} /></entity></entity>;
  }
}"#;

const SQUAD_BSN: &str = "#Boss
@enemies::Boss { @hp: 900.0 }
Transform { translation: Vec3 { x: 2.0 } }
Children [
    #eye
    Transform { translation: Vec3 { y: 0.8 } }
    --
    #hat
    @enemies::Hat
]
--
#GruntA
@enemies::Grunt
--
#GruntB
@enemies::Grunt { @hp: 5000.0 }
--
#GruntC
@enemies::Grunt { @hp: \"lots\" }
--
#Crate
Transform { translation: Vec3 { x: 4.0 } }
game::Loot { coins: 3 }
--
#Lamp
bevy_transform::components::transform::Transform { glow: 1.0 }
";

#[test]
fn placeholders_over_the_wire() {
    let dir = project("levels-placeholders", &[("package.json", "{}"), ("src/enemies.tsx", SQUAD), ("assets/levels/squad.bsn", SQUAD_BSN)]);
    let mut app = level_app(&dir);
    bevy_ts_tsx::protocol::add_methods(&mut app);
    assert!(reload(&mut app, false).errors.is_empty());
    settle(&mut app);
    let root = "level:levels/squad.bsn";
    let tree = ok_tsx(&mut app);
    // The deleted boss: its row marks it, its saved text and kept overrides are on level_node.
    assert_eq!(row(&tree, root, &["Boss"])["level"]["missing"], json!([{ "component": "enemies::Boss", "reason": "unregistered", "scene": true }]), "{tree}");
    assert!(tree["roots"].as_array().unwrap().iter().find(|r| r["name"] == root).unwrap()["nodes"].as_array().unwrap().iter().all(|n| n["keyPath"] != json!(["Boss", "eye"])), "the eye is kept, not spawned");
    row(&tree, root, &["Boss", "hat"]);
    let boss = ok(&mut app, "bevy-ts/level_node", Some(json!({ "level": "levels/squad.bsn", "namePath": ["Boss"] })));
    assert_eq!(boss["prefab"], "enemies::Boss", "{boss}");
    assert_eq!(boss["patches"], json!(["Transform"]), "the row's own scene component is not a patch: {boss}");
    let m = &boss["missing"][0];
    assert_eq!(m["scene"], true, "{boss}");
    assert_eq!((&m["reason"], &m["fields"], &m["location"]["line"]), (&json!("unregistered"), &json!({ "@hp": "900.0" }), &json!(2)), "{boss}");
    assert_eq!(m["overrides"], json!([{ "path": ["eye"], "fields": { "Transform.translation": "Vec3 { y: 0.8 }" } }]), "{boss}");
    // A scene that throws fails its own instance only.
    assert!(row(&tree, root, &["GruntA"])["level"].get("missing").is_none());
    let b = ok(&mut app, "bevy-ts/level_node", Some(json!({ "level": "levels/squad.bsn", "namePath": ["GruntB"] })));
    assert_eq!(b["missing"][0]["reason"], "failed", "{b}");
    assert!(b["missing"][0]["message"].as_str().unwrap().contains("no grunt has 5000 hp"), "{b}");
    assert_eq!(b["missing"][0]["expected"]["file"], "src/enemies.tsx", "{b}");
    // A misfit prop: the grunt expands without it.
    let c = ok(&mut app, "bevy-ts/level_node", Some(json!({ "level": "levels/squad.bsn", "namePath": ["GruntC"] })));
    assert_eq!(c["prefab"], "enemies::Grunt", "{c}");
    assert_eq!((&c["missing"][0]["reason"], &c["missing"][0]["fields"]), (&json!("mismatch"), &json!({ "@hp": "\"lots\"" })), "{c}");
    // A missing patch component: not the row's scene, and listed with the patches.
    assert_eq!(row(&tree, root, &["Crate"])["level"]["missing"], json!([{ "component": "game::Loot", "reason": "unregistered" }]), "{tree}");
    let crate_ = ok(&mut app, "bevy-ts/level_node", Some(json!({ "level": "levels/squad.bsn", "namePath": ["Crate"] })));
    assert!(crate_["missing"][0].get("scene").is_none(), "{crate_}");
    assert_eq!(crate_["patches"], json!(["Transform", "game::Loot"]), "{crate_}");
    // A patch that does not fit, written by its full path: inserted as `Transform`, listed once
    // by the name the file writes.
    let lamp = ok(&mut app, "bevy-ts/level_node", Some(json!({ "level": "levels/squad.bsn", "namePath": ["Lamp"] })));
    assert_eq!(lamp["missing"][0]["reason"], "mismatch", "{lamp}");
    assert_eq!(lamp["patches"], json!(["bevy_transform::components::transform::Transform"]), "{lamp}");
    // Diagnostics: a warning per entry on the list and the watch's loaded.
    let list = ok(&mut app, "bevy-ts/level_list", None);
    let warnings: Vec<&str> = list["levels"][0]["diagnostics"].as_array().unwrap().iter().filter_map(|d| d["message"].as_str()).filter(|m| m.starts_with("warning: ")).collect();
    assert_eq!(warnings.len(), 5, "{list}");
    let loaded = ok(&mut app, "bevy-ts/level+watch", None);
    assert_eq!(loaded["diagnostics"].as_array().map(Vec::len), Some(5), "{loaded}");
    // Writes to the boss's props are refused; its text stays.
    let hash = list["levels"][0]["hash"].clone();
    let w = ok(&mut app, "bevy-ts/level_write", Some(json!({ "file": "levels/squad.bsn", "base": hash, "ops": [{ "op": "set_prop", "path": ["Boss"], "prop": "hp", "value": 1.0 }] })));
    assert_eq!((&w["status"], &w["reason"]), (&json!("refused"), &json!("missing")), "{w}");

    // Play and back keeps the marker.
    let e = row(&tree, root, &["Boss"])["entity"].as_u64().map(Entity::from_bits).unwrap();
    for mode in [bevy_ts::EngineMode::Edit, bevy_ts::EngineMode::Play] {
        app.world_mut().resource_mut::<NextState<bevy_ts::EngineMode>>().set(mode);
        app.update();
        assert!(app.world().get::<bevy_ts_levels::engine::MissingComponents>(e).is_some(), "{mode:?}");
    }

    // The boss's class comes back: the same entity expands in place, and the marks go.
    write(&dir, "src/enemies.tsx", &SQUAD.replace("//BOSS", BOSS));
    assert!(reload(&mut app, false).errors.is_empty());
    settle(&mut app);
    let tree = ok_tsx(&mut app);
    assert_eq!(row(&tree, root, &["Boss"])["entity"].as_u64().map(Entity::from_bits), Some(e), "no respawn");
    assert!(row(&tree, root, &["Boss"])["level"].get("missing").is_none(), "{tree}");
    let eye = row(&tree, root, &["Boss", "eye"])["entity"].as_u64().map(Entity::from_bits).unwrap();
    assert_eq!(app.world().get::<Transform>(eye).unwrap().translation, Vec3::new(0.0, 0.8, 0.0), "the kept override merged by name");
    let boss = ok(&mut app, "bevy-ts/level_node", Some(json!({ "level": "levels/squad.bsn", "namePath": ["Boss"] })));
    assert!(boss.get("missing").is_none(), "{boss}");
    let (health, _) = bevy_ts::plugin_api::component_by_name(app.world(), "enemies::Health").unwrap();
    // SAFETY: Health is two f32s.
    let hp = unsafe { *(app.world().get_by_id(e, health).unwrap().as_ptr() as *const f32) };
    assert_eq!(hp, 900.0);
}

fn ok_tsx(app: &mut App) -> Value {
    let tree = call(app, "bevy-ts/tsx_tree", None).unwrap().unwrap();
    validate_in("bevy-ts-tsx", "bevy-ts/tsx_tree", &tree);
    tree
}
