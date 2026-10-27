//! Calls every method of the contract against the cube example and validates each response
//! against `protocol/bevy-ts.openrpc.json`.

use crate::common;

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

/// A project holding the cube example's two files.
fn cube_project(name: &str) -> crate::common::TestDir {
    let example = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/cube/src");
    let read = |f: &str| std::fs::read_to_string(example.join(f)).unwrap();
    project(name, &[("game.ts", &read("game.ts")), ("cube.tsx", &read("cube.tsx"))])
}

fn cube_app() -> (App, Entity) {
    cube_app_on(None)
}

fn cube_app_on(port: Option<u16>) -> (App, Entity) {
    let name = format!("protocol-{}", std::thread::current().name().unwrap_or("x").replace("::", "-"));
    let dir = cube_project(&name);
    let mut app = app(&dir.join("game.ts"));
    app.add_plugins(RemotePlugin::default());
    if let Some(port) = port {
        app.add_plugins(bevy::remote::http::RemoteHttpPlugin::default().with_port(port));
    }
    // TransformPlugin registers this in a full app.
    app.register_type::<Transform>();
    // The cube's scene has a `<mesh>`.
    app.add_plugins(AssetPlugin::default()).init_asset::<Mesh>().init_asset::<StandardMaterial>();
    bevy_ts_remote::add_methods(&mut app);
    let r = reload(&mut app, false);
    assert_eq!(r.status, bevy_ts::generation::ReloadStatus::Applied, "{:?}", r.errors);
    app.update();
    let id = app.world().resource::<TsComponents>().live("game::Mover").unwrap().id;
    let world = app.world_mut();
    let cube = QueryBuilder::<Entity>::new(world).with_id(id).build().single(world).unwrap();
    app.insert_resource(dir);
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
    // Without the viewport plugin bevy_remote answers its methods with method-not-found.
    assert!(app.world().resource::<RemoteMethods>().get("bevy-ts/viewport_attach").is_none());

    let hello = ok(&mut app, "bevy-ts/hello", None);
    assert_eq!(hello["protocol"], contract()["info"]["version"]);
    assert_eq!(hello["viewport"], false);
    assert_eq!(hello["generation"], 1);
    assert_eq!(hello["dev"]["reload"], true);
    // Protocol 1.4: bevy-ts's own anchors are exposed sets; no engine identity resource here.
    assert!(hello["sets"].as_array().unwrap().iter().any(|s| s["name"] == "bevy:transform-propagate"), "{hello}");
    assert!(hello["params"].as_array().is_none_or(|a| a.is_empty()));
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
    let bob = systems.as_array().unwrap().iter().find(|s| s["name"] == "bob").expect("bob listed");
    assert_eq!(bob["id"], "game.ts#bob");
    assert_eq!(bob["stage"], "update");
    assert_eq!(bob["write"], json!(["Transform"]));
    assert_eq!(bob["params"][1]["kind"], "query");

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
    let port = common::free_port();
    let (mut app, _) = cube_app_on(Some(port));
    let client = std::thread::spawn(move || {
        let post = |body: &str| -> String {
            let start = std::time::Instant::now();
            while start.elapsed() < BOUND {
                if let Ok(mut s) = std::net::TcpStream::connect(("127.0.0.1", port)) {
                    let req = format!("POST / HTTP/1.1\r\nHost: localhost\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len());
                    s.write_all(req.as_bytes()).unwrap();
                    let mut out = String::new();
                    s.read_to_string(&mut out).unwrap();
                    return out;
                }
                std::thread::sleep(std::time::Duration::from_millis(20));
            }
            panic!("the server never came up within {BOUND:?}");
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
            "import { app, Transform, Update } from \"bevy-ts\";\nimport type { Entity, Mut, Query } from \"bevy-ts\";\nlet first = -1;\nexport function one(q: Query<[Entity, Mut<Transform>]>) { q.each((e, t) => { if (first < 0) first = e as any; if ((e as any) === first) t.translation.x += 1; }); }\napp.addSystems(Update, one);\n",
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

fn editor(s: &Value, path: &str) -> Value {
    s[path]["x-bevy-ts-editor"].clone()
}

#[test]
fn ts_components_are_the_games_and_addable_while_declared() {
    let dir = cube_project("protocol-editor-ts");
    let mut app = app(&dir.join("game.ts"));
    app.add_plugins(RemotePlugin::default());
    app.add_plugins(AssetPlugin::default()).init_asset::<Mesh>().init_asset::<StandardMaterial>();
    bevy_ts_remote::add_methods(&mut app);
    reload(&mut app, false);
    app.update();
    let s = ok(&mut app, "bevy-ts/schema", Some(json!({ "names": ["game::Mover"] })));
    assert_eq!(editor(&s, "game::Mover"), json!({ "origin": "game", "internal": false, "addable": true, "category": "game" }));

    // A reload drops Mover while the cube still holds its data.
    write(&dir, "game.ts", "import { app } from \"bevy-ts\";\nexport {};\n");
    reload(&mut app, false);
    let s = ok(&mut app, "bevy-ts/schema", Some(json!({ "names": ["game::Mover"] })));
    assert_eq!(s["game::Mover"]["x-bevy-ts"]["declared"], false, "{s:#}");
    assert_eq!(editor(&s, "game::Mover")["addable"], false);
}

/// The engine `bevy-ts dev --headless` runs: Bevy's default plugins, bevy-ts, tsx and levels, no
/// ports, with the cube example loaded.
fn bare_engine(identity: bevy_ts::engine::EngineIdentity) -> App {
    let name = format!("protocol-{}", std::thread::current().name().unwrap_or("x").replace("::", "-"));
    let dir = cube_project(&name);
    let engine = bevy_ts_cli::Engine { identity };
    let mut app = bevy_ts_cli::cli::base_app(&engine, bevy_ts_cli::Prebuilt, bevy_ts_cli::cli::Display::Headless, Default::default(), None, true, Default::default(), &[], false);
    app.insert_resource(bevy_ts::generation::DevSource(std::sync::Arc::new(bevy_ts_cli::compile::Compiler::new(dir.join("game.ts"), true).unwrap())));
    app.add_plugins(RemotePlugin::default());
    bevy_ts_remote::add_methods(&mut app);
    app.finish();
    app.cleanup();
    reload(&mut app, false);
    app.update();
    app
}

#[test]
fn the_engines_components_are_its_own() {
    let mut app = bare_engine(bevy_ts::engine::EngineIdentity::prebuilt("0.0.0", "test"));
    let s = ok(&mut app, "bevy-ts/schema", None);
    let global = "bevy_transform::components::global_transform::GlobalTransform";
    let transform = "bevy_transform::components::transform::Transform";
    assert_eq!(editor(&s, global), json!({ "origin": "engine", "internal": true, "addable": false, "category": "transform" }));
    assert_eq!(editor(&s, transform), json!({ "origin": "engine", "internal": false, "addable": true, "category": "transform" }));
    assert_eq!(editor(&s, "bevy_ts_levels::engine::MissingComponents")["internal"], true, "level_node.missing draws placeholders");
    assert_eq!(editor(&s, "game::Mover")["origin"], "game");
    assert_eq!(editor(&s, "bevy_camera::clear_color::ClearColor")["addable"], false, "a resource is never added to an entity");

    let mut plugin = Vec::new();
    for (path, e) in s.as_object().unwrap() {
        let e = &e["x-bevy-ts-editor"];
        assert!(e.is_object(), "{path} has no x-bevy-ts-editor");
        // This test binary's own reflected types (crate `it`) are auto-registered; they are no crate's.
        if e["origin"] == "plugin" && !path.starts_with("it::") {
            plugin.push(path.clone());
        }
        if e["internal"] == true {
            assert_eq!(e["origin"], "engine", "{path}");
        }
    }
    assert!(plugin.is_empty(), "a bare engine has plugin components; list their crates in BEVY_CRATES: {plugin:?}");
    let unknown: Vec<_> = bevy_ts_remote::editor::INTERNAL.iter().filter(|p| s.get(**p).is_none()).collect();
    assert!(unknown.is_empty(), "INTERNAL names components the registry does not know: {unknown:?}");
}

#[test]
fn the_native_crates_components_are_the_games() {
    // This test binary's crate stands in for the project's native crate.
    let native = bevy_ts::engine::NativeIdentity { name: "it".into(), version: "0.1.0".into() };
    let mut app = bare_engine(bevy_ts::engine::EngineIdentity::project("test", Some(native)));
    app.register_type::<Fuel>();
    let s = ok(&mut app, "bevy-ts/schema", Some(json!({ "names": ["it::protocol::Fuel"] })));
    assert_eq!(editor(&s, "it::protocol::Fuel"), json!({ "origin": "game", "internal": false, "addable": true, "category": "game" }));
}

#[derive(Component, Reflect, Default)]
#[reflect(Component, Default)]
struct Fuel {
    litres: f32,
}

/// Prints every registered component of the bare engine with its editor info, for completing
/// `INTERNAL`: `cargo test -p bevy-ts-cli --test it protocol::list_components -- --ignored --nocapture`.
#[test]
#[ignore]
fn list_components() {
    let mut app = bare_engine(bevy_ts::engine::EngineIdentity::prebuilt("0.0.0", "test"));
    let s = ok(&mut app, "bevy-ts/schema", None);
    for (path, e) in s.as_object().unwrap() {
        println!("{path} {}", e["x-bevy-ts-editor"]);
    }
}

#[derive(Resource)]
pub struct Paused(pub bool);

// A spelling no other test claims: claims are process-wide (native_writes.rs).
#[derive(bevy::ecs::system::SystemParam)]
pub struct Pause<'w> {
    paused: ResMut<'w, Paused>,
}

#[bevy_ts_native::export(ts = "Pause", module = "bevy-ts/test", stands_for = "ResMut<Paused>")]
impl Pause<'_> {
    pub fn pause(&mut self) {
        self.paused.0 = true;
    }
}

#[test]
fn the_handshake_lists_an_exports_claim() {
    use bevy_ts_native::BevyTsNativeExt;
    let (mut app, _) = cube_app();
    app.ts_param::<Pause<'static>>();
    let hello = ok(&mut app, "bevy-ts/hello", None);
    let pause = hello["params"].as_array().unwrap().iter().find(|p| p["name"] == "Pause").cloned().unwrap();
    // Protocol 1.27: `standsFor`, the Bevy spelling a TS param writes to get this export.
    assert_eq!(pause["standsFor"], "ResMut<Paused>", "{pause}");
    assert_eq!(pause["methods"], json!(["pause"]));
}

#[test]
fn a_custom_schedules_system_lists_as_stage_schedule_with_its_label() {
    let src = r#"import { app, ScheduleLabel, Update, Last } from "bevy-ts";
export class Waves extends ScheduleLabel { static path = "game::Waves" }
export function spawnWave() {}
export function tally() {}
app.insertScheduleAfter(Update, Waves).addSystems(Waves, spawnWave).addSystems(Last, tally);
"#;
    let dir = project("protocol-schedule", &[("game.ts", src)]);
    let mut app = app(&dir.join("game.ts"));
    app.add_plugins(RemotePlugin::default());
    bevy_ts_remote::add_methods(&mut app);
    let r = reload(&mut app, false);
    assert!(r.errors.is_empty(), "{:?}", r.errors);
    app.update();
    let systems = ok(&mut app, "bevy-ts/systems", None);
    let rows = systems.as_array().unwrap();
    let wave = rows.iter().find(|s| s["name"] == "spawnWave").unwrap();
    assert_eq!(wave["stage"], "schedule");
    assert_eq!(wave["schedule"], json!({ "kind": "schedule", "label": "game::Waves" }));
    let tally = rows.iter().find(|s| s["name"] == "tally").unwrap();
    assert_eq!(tally["stage"], "last");
}

/// A reflected component holding a `Duration` (reflect-coverage: seconds on the wire).
#[derive(Component, Reflect, Default)]
#[reflect(Component, Default)]
struct Cooldown {
    wait: std::time::Duration,
}

/// reflect-coverage's remote-protocol scenarios: a reflected value read is handed back unchanged,
/// a hole takes a colour form, and a `Duration` reads as seconds.
#[test]
fn reflected_components_read_and_write_in_the_shape_tsx_takes() {
    use bevy::camera::{OrthographicProjection, Projection, ScalingMode};
    use bevy::light::DirectionalLight;
    let (mut app, _) = cube_app();
    app.register_type::<Projection>().register_type::<DirectionalLight>().register_type::<Cooldown>();
    let projection = Projection::Orthographic(OrthographicProjection { scale: 2.0, scaling_mode: ScalingMode::FixedVertical { viewport_height: 12.0 }, ..OrthographicProjection::default_3d() });
    let world = app.world_mut();
    let camera = world.spawn(projection.clone()).id();
    let light = world.spawn(DirectionalLight::default()).id();
    let timer = world.spawn(Cooldown { wait: std::time::Duration::from_millis(1500) }).id();
    let path = "bevy_camera::projection::Projection";
    let got = ok(&mut app, "bevy-ts/get_components", Some(json!({ "entity": camera.to_bits(), "components": [path] })));
    let read = got["components"][path].clone();
    assert_eq!(read["Orthographic"]["scaling_mode"], json!({ "FixedVertical": { "viewport_height": 12.0 } }), "{got:#}");
    // (a) The same JSON sent back with an empty path changes nothing.
    ok(&mut app, "bevy-ts/mutate_components", Some(json!({ "entity": camera.to_bits(), "component": path, "path": "", "value": read })));
    let after = app.world().get::<Projection>(camera).unwrap();
    assert!(after.reflect_partial_eq(&projection).unwrap_or(false), "{after:?}");
    // (b) A hole written in any colour form TSX takes.
    let light_path = "bevy_light::directional_light::DirectionalLight";
    ok(&mut app, "bevy-ts/mutate_components", Some(json!({ "entity": light.to_bits(), "component": light_path, "path": ".color", "value": "#ffd7a8" })));
    let colour = app.world().get::<DirectionalLight>(light).unwrap().color;
    assert_eq!(colour, Color::Srgba(Srgba::hex("#ffd7a8").unwrap()));
    // (c) A Duration on the wire is seconds.
    let cooldown = "it::protocol::Cooldown";
    let got = ok(&mut app, "bevy-ts/get_components", Some(json!({ "entity": timer.to_bits(), "components": [cooldown] })));
    assert_eq!(got["components"][cooldown], json!({ "wait": 1.5 }), "{got:#}");
}

#[test]
fn string_and_list_fields_read_edit_and_type_on_contract() {
    let src = r#"import { Component, type f32, type u32 } from "bevy-ts";
export class Tag extends Component { label: string = "crate"; hp: f32 = 1; scores: u32[] = [] }
"#;
    let dir = project("protocol-strings", &[("game.ts", src)]);
    let mut app = app(&dir.join("game.ts"));
    app.add_plugins(RemotePlugin::default());
    bevy_ts_remote::add_methods(&mut app);
    let r = reload(&mut app, false);
    assert_eq!(r.status, bevy_ts::generation::ReloadStatus::Applied, "{:?}", r.errors);
    let e = spawn_ts(&mut app, "game::Tag", json!({ "label": "door" }), ());
    let got = ok(&mut app, "bevy-ts/get_components", Some(json!({ "entity": e.to_bits(), "components": ["game::Tag"] })));
    assert_eq!(got["components"]["game::Tag"], json!({ "label": "door", "hp": 1.0, "scores": [] }));

    ok(&mut app, "bevy-ts/mutate_components", Some(json!({ "entity": e.to_bits(), "component": "game::Tag", "path": ".label", "value": "gate" })));
    assert_eq!(read_ts(&app, e, "game::Tag").unwrap()["label"], json!("gate"));
    let bad = call(&mut app, "bevy-ts/mutate_components", Some(json!({ "entity": e.to_bits(), "component": "game::Tag", "path": ".label", "value": 3 }))).unwrap_err();
    assert!(bad.message.contains(".label expects string"), "{}", bad.message);
    assert_eq!(read_ts(&app, e, "game::Tag").unwrap()["label"], json!("gate"), "a bad value writes nothing");
    ok(&mut app, "bevy-ts/mutate_components", Some(json!({ "entity": e.to_bits(), "component": "game::Tag", "path": ".scores", "value": [3, 1] })));
    assert_eq!(read_ts(&app, e, "game::Tag").unwrap()["scores"], json!([3, 1]));
    // One item, in place (stage 5); past the end fails naming the length and writes nothing.
    let before = tick(&mut app);
    app.world_mut().increment_change_tick();
    ok(&mut app, "bevy-ts/mutate_components", Some(json!({ "entity": e.to_bits(), "component": "game::Tag", "path": ".scores[0]", "value": 9 })));
    assert_eq!(read_ts(&app, e, "game::Tag").unwrap()["scores"], json!([9, 1]));
    assert_eq!(changed_since(&mut app, "game::Tag", before), vec![e], "an item edit marks the row changed");
    let past = call(&mut app, "bevy-ts/mutate_components", Some(json!({ "entity": e.to_bits(), "component": "game::Tag", "path": ".scores[2]", "value": 1 }))).unwrap_err();
    assert!(past.message.contains(".scores[2]: [2] is past the end: the list has 2 items"), "{}", past.message);
    let wrong = call(&mut app, "bevy-ts/mutate_components", Some(json!({ "entity": e.to_bits(), "component": "game::Tag", "path": ".scores[0]", "value": -1 }))).unwrap_err();
    assert!(wrong.message.contains(".scores[0]"), "{}", wrong.message);
    assert_eq!(read_ts(&app, e, "game::Tag").unwrap()["scores"], json!([9, 1]));

    let s = ok(&mut app, "bevy-ts/schema", Some(json!({ "names": ["game::Tag"] })));
    assert_eq!(s["game::Tag"]["properties"]["label"], json!({ "type": "string" }));
    let fields = &s["game::Tag"]["x-bevy-ts"]["layout"]["fields"];
    let label = fields.as_array().unwrap().iter().find(|f| f["name"] == "label").unwrap();
    assert_eq!((label["type"].clone(), label["offset"].clone()), (json!({ "kind": "string" }), json!(8)), "after the 8-byte header");
    let scores = fields.as_array().unwrap().iter().find(|f| f["name"] == "scores").unwrap();
    assert_eq!(scores["type"], json!({ "kind": "list", "of": { "kind": "u32" } }));
    assert_eq!(s["game::Tag"]["properties"]["scores"]["type"], json!("array"));
}

#[test]
fn a_list_of_objects_reads_edits_and_types_on_contract() {
    let src = r#"import { Component, type Entity, type f32, type Vec3 } from "bevy-ts";
type Kind = "hit" | "miss";
interface Hit { at: Vec3; by: Entity; kind: Kind; hp: f32 }
export class Trail extends Component { hits: Hit[] = []; points: Vec3[] = [] }
"#;
    let dir = project("protocol-objects", &[("game.ts", src)]);
    let mut app = app(&dir.join("game.ts"));
    app.add_plugins(RemotePlugin::default());
    bevy_ts_remote::add_methods(&mut app);
    let r = reload(&mut app, false);
    assert_eq!(r.status, bevy_ts::generation::ReloadStatus::Applied, "{:?}", r.errors);
    let e = spawn_ts(&mut app, "game::Trail", json!({ "points": [[1, 2, 3]] }), ());
    let hits = json!([{ "at": [1.0, 0.0, 0.0], "by": 5, "kind": "miss", "hp": 2.0 }]);
    ok(&mut app, "bevy-ts/mutate_components", Some(json!({ "entity": e.to_bits(), "component": "game::Trail", "path": ".hits", "value": hits })));
    let got = ok(&mut app, "bevy-ts/get_components", Some(json!({ "entity": e.to_bits(), "components": ["game::Trail"] })));
    assert_eq!(got["components"]["game::Trail"], json!({ "hits": hits, "points": [[1.0, 2.0, 3.0]] }));
    let partial = call(&mut app, "bevy-ts/mutate_components", Some(json!({ "entity": e.to_bits(), "component": "game::Trail", "path": ".hits", "value": [{ "at": [0, 0, 0] }] }))).unwrap_err();
    assert!(partial.message.contains("member by is missing"), "{}", partial.message);
    // An item's member and a vector item's lane, in place.
    let at = |path: &str, value: serde_json::Value| json!({ "entity": e.to_bits(), "component": "game::Trail", "path": path, "value": value });
    ok(&mut app, "bevy-ts/mutate_components", Some(at(".hits[0].hp", json!(3))));
    ok(&mut app, "bevy-ts/mutate_components", Some(at(".hits[0].at.y", json!(4))));
    ok(&mut app, "bevy-ts/mutate_components", Some(at(".points[0].z", json!(7))));
    let trail = read_ts(&app, e, "game::Trail").unwrap();
    assert_eq!(trail["hits"][0], json!({ "at": [1.0, 4.0, 0.0], "by": 5, "kind": "miss", "hp": 3.0 }));
    assert_eq!(trail["points"], json!([[1.0, 2.0, 7.0]]));
    let bad = call(&mut app, "bevy-ts/mutate_components", Some(at(".hits[0].kind", json!("graze")))).unwrap_err();
    assert!(bad.message.contains(".hits[0].kind"), "{}", bad.message);

    // `ok` validates the schema's answer against the protocol's: a `struct` item is on contract.
    let s = ok(&mut app, "bevy-ts/schema", Some(json!({ "names": ["game::Trail"] })));
    let fields = &s["game::Trail"]["x-bevy-ts"]["layout"]["fields"];
    let hits = fields.as_array().unwrap().iter().find(|f| f["name"] == "hits").unwrap();
    assert_eq!(hits["type"]["of"]["kind"], json!("struct"));
    assert_eq!(hits["type"]["of"]["fields"][1], json!({ "name": "by", "type": { "kind": "entity" } }));
    let item = &s["game::Trail"]["properties"]["hits"]["items"];
    assert_eq!((item["type"].clone(), item["required"].clone()), (json!("object"), json!(["at", "by", "kind", "hp"])));
}
