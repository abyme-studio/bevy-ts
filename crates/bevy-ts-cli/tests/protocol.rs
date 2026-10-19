//! Calls every method of the contract against the cube example and validates each response
//! against `protocol/bevy-ts.openrpc.json`.

mod common;

use bevy::prelude::*;
use bevy::remote::{BrpError, RemoteMethodSystemId, RemoteMethods, RemotePlugin};
use bevy_ts::components::TsComponents;
use common::*;
use serde_json::{Value, json};
use std::collections::BTreeSet;
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

/// Validates a method's result against its schema in the contract.
fn validate(method: &str, result: &Value) {
    let doc = contract();
    let m = doc["methods"].as_array().unwrap().iter().find(|m| m["name"] == method).unwrap();
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

fn cube_app() -> (App, Entity) {
    cube_app_on(None)
}

fn cube_app_on(port: Option<u16>) -> (App, Entity) {
    let src = std::fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/cube/src/game.ts")).unwrap();
    let name = format!("protocol-{}", std::thread::current().name().unwrap_or("x").replace("::", "-"));
    let dir = project(&name, &[("game.ts", &src)]);
    let mut app = app(&dir.join("game.ts"));
    app.add_plugins(RemotePlugin::default());
    if let Some(port) = port {
        app.add_plugins(bevy::remote::http::RemoteHttpPlugin::default().with_port(port));
    }
    // TransformPlugin registers this in a full app.
    app.register_type::<Transform>();
    bevy_ts_remote::add_methods(&mut app);
    assert_eq!(reload(&mut app, false).status, bevy_ts::generation::ReloadStatus::Applied);
    app.update();
    let id = app.world().resource::<TsComponents>().live("game::Mover").unwrap().id;
    let world = app.world_mut();
    let cube = QueryBuilder::<Entity>::new(world).with_id(id).build().single(world).unwrap();
    (app, cube)
}

#[test]
fn every_method_answers_on_contract() {
    let (mut app, cube) = cube_app();
    let e = cube.to_bits();
    let served: BTreeSet<String> = app.world().resource::<RemoteMethods>().methods().into_iter().filter(|m| m.starts_with("bevy-ts/")).collect();
    // Feature crates' methods (bevy-ts-tsx's) are served only with their plugin's methods added.
    let listed: BTreeSet<String> =
        bevy_ts_remote::protocol::METHODS.iter().filter(|m| m.crate_name == bevy_ts_remote::CRATE).map(|m| m.name.to_string()).collect();
    assert_eq!(served, listed);

    let hello = ok(&mut app, "bevy-ts/hello", None);
    assert_eq!(hello["protocol"], contract()["info"]["version"]);
    assert_eq!(hello["generation"], 1);
    assert_eq!(hello["dev"]["reload"], true);
    ok(&mut app, "bevy-ts/discover", None);

    // Schemas: one renderer for both kinds.
    let s = ok(&mut app, "bevy-ts/schema", Some(json!({ "names": ["game::Mover", "bevy_transform::components::transform::Transform"] })));
    assert_eq!(s["game::Mover"]["kind"], "Struct");
    assert_eq!(s["game::Mover"]["x-bevy-ts"]["layout"]["size"], 8);
    let t = &s["bevy_transform::components::transform::Transform"];
    assert_eq!(t["kind"], "Struct");
    assert!(t.get("x-bevy-ts").is_none());
    ok(&mut app, "bevy-ts/schema", Some(json!({ "source": "ts" })));

    let g = ok(&mut app, "bevy-ts/get_components", Some(json!({ "entity": e, "components": ["game::Mover", "bevy_transform::components::transform::Transform"] })));
    assert_eq!(g["components"]["game::Mover"], json!({ "speed": 1.0, "amplitude": 2.0 }));
    assert!(g["components"]["bevy_transform::components::transform::Transform"].is_object(), "{g:#}");

    // A watch: first message has everything, then only what moved.
    let watch = json!({ "entity": e, "components": ["game::Mover"] });
    let first = ok(&mut app, "bevy-ts/get_components+watch", Some(watch.clone()));
    assert_eq!(first["components"]["game::Mover"]["amplitude"], 2.0);
    app.update();
    assert_eq!(call(&mut app, "bevy-ts/get_components+watch", Some(watch.clone())).unwrap(), None, "bob writes Transform, never Mover");
    ok(&mut app, "bevy-ts/mutate_components", Some(json!({ "entity": e, "component": "game::Mover", "path": ".amplitude", "value": 4 })));
    app.update();
    let next = ok(&mut app, "bevy-ts/get_components+watch", Some(watch.clone()));
    assert_eq!(next["components"]["game::Mover"]["amplitude"], 4.0);

    let wrong = call(&mut app, "bevy-ts/mutate_components", Some(json!({ "entity": e, "component": "game::Mover", "path": ".amplitude", "value": "wide" })))
        .unwrap_err();
    assert!(wrong.message.contains(".amplitude expects f32"), "{wrong:?}");
    let bad_path = call(&mut app, "bevy-ts/mutate_components", Some(json!({ "entity": e, "component": "game::Mover", "path": ".amp", "value": 1 })))
        .unwrap_err();
    assert!(bad_path.message.contains(".amp"), "{bad_path:?}");
    ok(&mut app, "bevy-ts/mutate_components", Some(json!({ "entity": e, "component": "bevy_transform::components::transform::Transform", "path": ".scale.x", "value": 2.0 })));
    assert_eq!(app.world().get::<Transform>(cube).unwrap().scale.x, 2.0);

    let snap = ok(&mut app, "bevy-ts/entities", None);
    let me = snap["entities"].as_array().unwrap().iter().find(|x| x["entity"] == e).unwrap();
    assert!(me["components"].as_array().unwrap().contains(&json!("game::Mover")));

    let systems = ok(&mut app, "bevy-ts/systems", None);
    assert_eq!(systems[0]["name"], "bob");

    let r = ok(&mut app, "bevy-ts/reload", Some(json!({})));
    assert_eq!(r["status"], "applied");
    assert_eq!(r["generation"], 2);

    ok(&mut app, "bevy-ts/pause", Some(json!({ "paused": true })));
    ok(&mut app, "bevy-ts/step", Some(json!({ "frames": 1 })));
    ok(&mut app, "bevy-ts/pause", Some(json!({ "paused": false })));
}

#[test]
fn streams_send_snapshots_then_deltas() {
    let (mut app, cube) = cube_app();
    // Log and reload streams: a new subscriber gets recent history.
    let log = ok(&mut app, "bevy-ts/log+watch", None);
    assert!(log["entries"].as_array().unwrap().iter().any(|e| e["message"].as_str().unwrap().contains("generation 1 applied")));
    app.update();
    assert_eq!(call(&mut app, "bevy-ts/reload+watch", None).unwrap(), None, "a new reload subscriber starts now");
    app.update();
    reload(&mut app, false);
    let r = ok(&mut app, "bevy-ts/reload+watch", None);
    assert_eq!(r["reports"].as_array().unwrap().last().unwrap()["status"], "applied");

    // Entities: snapshot first, then one message per changed frame, none for quiet frames.
    let snap = ok(&mut app, "bevy-ts/entities+watch", None);
    assert!(snap["snapshot"].is_array());
    let seq = snap["seq"].as_u64().unwrap();
    app.update();
    assert_eq!(call(&mut app, "bevy-ts/entities+watch", None).unwrap(), None);
    let bullet = app.world_mut().spawn((Name::new("bullet"), ChildOf(cube))).id();
    app.update();
    let m = ok(&mut app, "bevy-ts/entities+watch", None);
    assert_eq!(m["seq"].as_u64().unwrap(), seq + 1);
    let spawned = &m["spawned"][0];
    assert_eq!(spawned["entity"], bullet.to_bits());
    assert_eq!(spawned["name"], "bullet");
    assert_eq!(spawned["parent"], cube.to_bits());
    app.world_mut().despawn(bullet);
    app.update();
    let m = ok(&mut app, "bevy-ts/entities+watch", None);
    assert_eq!(m["despawned"], json!([bullet.to_bits()]));
    assert_eq!(m["seq"].as_u64().unwrap(), seq + 2);
}

#[test]
fn stock_and_bevy_ts_share_the_port() {
    use std::io::{Read, Write};
    let port = 15900 + (std::process::id() % 90) as u16;
    let (mut app, _) = cube_app_on(Some(port));
    let client = std::thread::spawn(move || {
        let post = |body: &str| -> String {
            for _ in 0..200 {
                if let Ok(mut s) = std::net::TcpStream::connect(("127.0.0.1", port)) {
                    let req = format!("POST / HTTP/1.1\r\nHost: localhost\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len());
                    s.write_all(req.as_bytes()).unwrap();
                    let mut out = String::new();
                    s.read_to_string(&mut out).unwrap();
                    return out;
                }
                std::thread::sleep(std::time::Duration::from_millis(20));
            }
            panic!("server never came up");
        };
        let stock = post(r#"{"jsonrpc":"2.0","id":1,"method":"world.list_components","params":null}"#);
        let ours = post(r#"{"jsonrpc":"2.0","id":2,"method":"bevy-ts/hello"}"#);
        (stock, ours)
    });
    while !client.is_finished() {
        app.update();
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
    let (stock, ours) = client.join().unwrap();
    assert!(stock.contains("\"result\""), "{stock}");
    let version = format!("\"protocol\":\"{}\"", contract()["info"]["version"].as_str().unwrap());
    assert!(ours.contains(&version), "{ours}");
}

#[test]
fn a_paused_game_keeps_edits_and_steps_one_frame() {
    let (mut app, cube) = cube_app();
    let e = cube.to_bits();
    ok(&mut app, "bevy-ts/pause", Some(json!({ "paused": true })));
    app.update();
    ok(&mut app, "bevy-ts/mutate_components", Some(json!({ "entity": e, "component": "bevy_transform::components::transform::Transform", "path": ".translation.y", "value": 3.0 })));
    let t0 = app.world().resource::<Time<Virtual>>().elapsed();
    for _ in 0..5 {
        app.update();
    }
    assert_eq!(app.world().get::<Transform>(cube).unwrap().translation.y, 3.0, "no system overwrote the edit");
    assert_eq!(app.world().resource::<Time<Virtual>>().elapsed(), t0, "virtual time is paused");

    std::thread::sleep(std::time::Duration::from_millis(5));
    ok(&mut app, "bevy-ts/step", Some(json!({ "frames": 1 })));
    app.update();
    let y = app.world().get::<Transform>(cube).unwrap().translation.y;
    assert_ne!(y, 3.0, "bob ran in the stepped frame");
    let t1 = app.world().resource::<Time<Virtual>>().elapsed();
    assert!(t1 > t0, "virtual time advanced one frame");
    for _ in 0..3 {
        app.update();
    }
    assert_eq!(app.world().get::<Transform>(cube).unwrap().translation.y, y, "paused again");
    assert_eq!(app.world().resource::<Time<Virtual>>().elapsed(), t1);
    let err = call(&mut app, "bevy-ts/pause", Some(json!({ "paused": false }))).map(|_| ());
    assert!(err.is_ok());
    let not_paused = call(&mut app, "bevy-ts/step", None).unwrap_err();
    assert!(not_paused.message.contains("while paused"));
}

#[test]
fn a_watch_stays_quiet_for_rows_a_system_does_not_write() {
    let dir = project(
        "quiet-watch",
        &[(
            "game.ts",
            "import { app } from \"bevy-ts\";\nimport { Transform } from \"bevy-ts/bevy\";\nlet first = -1;\napp.system(\"one\", { write: [Transform], run(q) { q.each((e, t) => { if (first < 0) first = e; if (e === first) t.translation.x += 1; }); } });\n",
        )],
    );
    let mut app = app(&dir.join("game.ts"));
    app.add_plugins(RemotePlugin::default());
    app.register_type::<Transform>();
    bevy_ts_remote::add_methods(&mut app);
    let a = app.world_mut().spawn(Transform::default()).id();
    let b = app.world_mut().spawn(Transform::default()).id();
    reload(&mut app, false);
    app.update();
    let written = if app.world().get::<Transform>(a).unwrap().translation.x > 0.0 { a } else { b };
    let quiet = if written == a { b } else { a };
    let path = "bevy_transform::components::transform::Transform";
    for (e, expect_messages) in [(quiet, false), (written, true)] {
        let params = json!({ "entity": e.to_bits(), "components": [path] });
        ok(&mut app, "bevy-ts/get_components+watch", Some(params.clone()));
        for _ in 0..3 {
            app.update();
            let m = call(&mut app, "bevy-ts/get_components+watch", Some(params.clone())).unwrap();
            assert_eq!(m.is_some(), expect_messages, "{e}: {m:?}");
        }
    }
}
