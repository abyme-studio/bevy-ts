//! `bevy-ts/tsx_*` against real TSX in V8: each answer validated against the contract, and the
//! editor round trip: a tool rewrites a controlled prop in a `.tsx` file, saves, and the watch
//! stream names the node whose values were written.

use crate::common;

use bevy::prelude::*;
use bevy::remote::{BrpError, RemoteMethodSystemId, RemoteMethods, RemotePlugin};
use bevy_ts::generation::ReloadStatus;
use common::*;
use serde_json::{Value, json};
use std::path::Path;

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
    let doc: Value = serde_json::from_str(bevy_ts_remote::OPENRPC).unwrap();
    let m = doc["methods"].as_array().unwrap().iter().find(|m| m["name"] == method).unwrap();
    assert_eq!(m["x-bevy-ts-crate"], "bevy-ts-tsx");
    let mut schema = m["result"]["schema"].clone();
    let mut defs = doc["components"]["schemas"].clone();
    rewrite(&mut schema);
    rewrite(&mut defs);
    let mut root = json!({ "$schema": "http://json-schema.org/draft-07/schema#", "definitions": defs });
    if let Value::Object(s) = schema {
        root.as_object_mut().unwrap().extend(s);
    }
    let v = jsonschema::draft7::new(&root).unwrap();
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

const ENEMY: &str = r##"import { Component, f32 } from "bevy-ts";
import { Transform } from "bevy-ts/bevy";

export class Health extends Component { static path = "game::Health"; hp: f32 = 100; max: f32 = 100 }
export const Enemy = ({ at, hp = 100 }: { at: [number, number, number]; hp?: number }) => (
  <entity name="enemy">
    <Transform translation={at} />
    <Health initialHp={hp} max={hp} />
  </entity>
);
"##;

fn game(b_at: &str) -> String {
    format!(
        "import {{ app }} from \"bevy-ts\";\nimport {{ Enemy }} from \"./enemy.tsx\";\n\napp.scene(\"arena\", () => (\n  <>\n    <Enemy key=\"a\" at={{[-3, 0, -4]}} />\n    <Enemy key=\"b\" at={{{b_at}}} hp={{250}} />\n  </>\n));\n"
    )
}

fn setup(name: &str) -> (std::path::PathBuf, App) {
    let dir = project(name, &[("enemy.tsx", ENEMY), ("game.tsx", &game("[3, 0, -4]"))]);
    let mut app = app(&dir.join("game.tsx"));
    app.add_plugins(RemotePlugin::default());
    bevy_ts_tsx::protocol::add_methods(&mut app);
    assert_eq!(reload(&mut app, false).status, ReloadStatus::Applied);
    app.update();
    (dir, app)
}

fn node_of<'a>(tree: &'a Value, key: &str) -> &'a Value {
    tree["roots"][0]["nodes"].as_array().unwrap().iter().find(|n| n["keyPath"] == json!([key])).unwrap()
}

#[test]
fn the_tree_a_node_and_its_provenance_and_find() {
    let (_dir, mut app) = setup("tsxp-tree");
    let tree = ok(&mut app, "bevy-ts/tsx_tree", None);
    assert_eq!(tree["roots"][0]["name"], "arena");
    let b = node_of(&tree, "b");
    assert_eq!(b["type"], "Enemy");
    assert!(b["source"]["file"].as_str().unwrap().ends_with("enemy.tsx"));
    assert_eq!(b["tombstoned"], false);

    // Enemy b's chain: the <entity> in enemy.tsx, then <Enemy key="b"> in game.tsx; its props.
    let n = ok(&mut app, "bevy-ts/tsx_node", Some(json!({ "entity": b["entity"] })));
    let chain: Vec<(String, u64)> = n["source"]
        .as_array()
        .unwrap()
        .iter()
        .map(|s| (Path::new(s["file"].as_str().unwrap()).file_name().unwrap().to_string_lossy().to_string(), s["line"].as_u64().unwrap()))
        .collect();
    assert_eq!(chain, vec![("enemy.tsx".to_string(), 6), ("game.tsx".to_string(), 7)]);
    let health = &n["components"]["game::Health"];
    assert_eq!(health["fields"]["hp"], json!({ "provenance": "initial", "value": 250 }));
    assert_eq!(health["fields"]["max"], json!({ "provenance": "controlled", "value": 250 }));
    assert_eq!(health["source"]["line"], 8);
    assert_eq!(ok(&mut app, "bevy-ts/tsx_node", Some(json!({ "node": b["node"] })))["node"], b["node"]);

    // One prefab element maps to every instance; a call site to its own.
    let enemy_file = n["source"][0]["file"].as_str().unwrap().to_string();
    let all = ok(&mut app, "bevy-ts/tsx_find", Some(json!({ "file": enemy_file, "line": 6 })));
    assert_eq!(all["matches"].as_array().unwrap().len(), 2);
    assert!(all["matches"].as_array().unwrap().iter().all(|m| m["depth"] == 0));
    let one = ok(&mut app, "bevy-ts/tsx_find", Some(json!({ "file": "game.tsx", "line": 7 })));
    assert_eq!(one["matches"].as_array().unwrap().len(), 1);
    assert_eq!(one["matches"][0]["depth"], 1);

    // A tombstoned node is listed as such.
    app.world_mut().despawn(Entity::from_bits(node_of(&tree, "a")["entity"].as_u64().unwrap()));
    let r = reload(&mut app, false);
    assert_eq!(r.status, ReloadStatus::Applied);
    let tree = ok(&mut app, "bevy-ts/tsx_tree", None);
    assert_eq!(node_of(&tree, "a")["tombstoned"], true);
}

#[test]
fn an_edit_written_to_source_round_trips_and_the_watch_names_it() {
    let (dir, mut app) = setup("tsxp-watch");
    let first = ok(&mut app, "bevy-ts/tsx_tree+watch", None);
    assert!(first["snapshot"].is_array());
    let b = node_of(&json!({ "roots": first["snapshot"] }), "b").clone();
    app.update();
    assert_eq!(call(&mut app, "bevy-ts/tsx_tree+watch", None).unwrap(), None, "nothing reconciled: no message");

    // A tool drags b: it rewrites the literal and saves.
    write(&dir, "game.tsx", &game("[5, 0, -4]"));
    assert_eq!(reload(&mut app, false).status, ReloadStatus::Applied);
    app.update();
    let m = ok(&mut app, "bevy-ts/tsx_tree+watch", None);
    assert_eq!(m["written"], json!([b["node"]]), "{m:#}");
    let e = Entity::from_bits(b["entity"].as_u64().unwrap());
    assert_eq!(app.world().get::<Transform>(e).unwrap().translation, Vec3::new(5.0, 0.0, -4.0));

    // An initial prop edited in source does not change the running entity.
    write(&dir, "game.tsx", &game("[5, 0, -4]").replace("hp={250}", "hp={300}"));
    assert_eq!(reload(&mut app, false).status, ReloadStatus::Applied);
    let n = ok(&mut app, "bevy-ts/tsx_node", Some(json!({ "entity": b["entity"] })));
    assert_eq!(n["components"]["game::Health"]["fields"]["hp"]["value"], 250, "initial: not re-applied");
    assert_eq!(n["components"]["game::Health"]["fields"]["max"]["value"], 300, "controlled: written");
}
