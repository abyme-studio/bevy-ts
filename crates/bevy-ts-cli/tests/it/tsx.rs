//! TSX scenes end to end: oxc's development JSX transform, the `bevy-ts/jsx-runtime` running in
//! V8, `app.scene`, and the Rust reconciler, across reloads of real files.

use crate::common;

use bevy::prelude::*;
use bevy_ts::generation::ReloadStatus;
use bevy_ts_tsx::{TsxNode, TsxTree};
use common::*;
use std::path::Path;

const ENEMY: &str = r##"import { Component, f32 } from "bevy-ts";
import { Transform } from "bevy-ts/bevy";

export class Health extends Component { static path = "game::Health"; hp: f32 = 100; max: f32 = 100 }
export const Enemy = ({ at, hp = 100 }: { at: [number, number, number]; hp?: number }) => (
  <entity name="enemy">
    <Transform translation={at} />
    <Health initialHp={hp} max={hp} />
    <entity key="eye"><Transform translation={[0, 0.6, 0.5]} /></entity>
    <mesh shape="cuboid" size={[1, 1, 1]} color="#c33" />
  </entity>
);
"##;

fn game(body: &str) -> String {
    format!("import {{ app }} from \"bevy-ts\";\nimport {{ Enemy }} from \"./enemy.tsx\";\n\napp.scene(\"arena\", () => (\n  <>\n{body}  </>\n));\n")
}

fn node(app: &mut App, path: &[&str]) -> Option<(Entity, TsxNode)> {
    let world = app.world_mut();
    world.query::<(Entity, &TsxNode)>().iter(world).find(|(_, n)| n.key_path == path).map(|(e, n)| (e, n.clone()))
}

fn x(app: &App, e: Entity) -> f32 {
    app.world().get::<Transform>(e).unwrap().translation.x
}

/// The test app, with assets so `<mesh>` can build cubes.
fn tsx_app(entry: &Path) -> App {
    let mut app = app(entry);
    app.add_plugins(AssetPlugin::default()).init_asset::<Mesh>().init_asset::<StandardMaterial>();
    app
}

fn setup(name: &str, body: &str) -> (std::path::PathBuf, App) {
    let dir = project(name, &[("enemy.tsx", ENEMY), ("game.tsx", &game(body))]);
    let mut app = tsx_app(&dir.join("game.tsx"));
    let r = reload(&mut app, false);
    assert_eq!(r.status, ReloadStatus::Applied, "{:?}", r.errors);
    (dir, app)
}

fn save(app: &mut App, dir: &Path, file: &str, src: &str) -> bevy_ts::generation::ReloadReport {
    write(dir, file, src);
    reload(app, false)
}

#[test]
fn a_scene_mounts_and_a_reload_writes_only_its_changes() {
    let body = "    <Enemy key=\"a\" at={[-3, 0, -4]} />\n    <Enemy key=\"b\" at={[3, 0, -4]} hp={250} />\n";
    let (dir, mut app) = setup("tsx-mount", body);
    let (a, _) = node(&mut app, &["a"]).expect("enemy a");
    let (b, nb) = node(&mut app, &["b"]).expect("enemy b");
    assert_eq!(x(&app, a), -3.0);
    assert!(node(&mut app, &["b", "eye"]).is_some(), "the child entity");
    assert_eq!(app.world().get::<Name>(b).unwrap().as_str(), "enemy");
    assert_eq!(app.world().get::<Mesh3d>(a).unwrap().0, app.world().get::<Mesh3d>(b).unwrap().0, "two cubes, one mesh");
    // The source chain: the <entity> in enemy.tsx, then the <Enemy key="b"> in game.tsx.
    let chain: Vec<String> = nb.source.iter().map(|s| format!("{}:{}", Path::new(&s.file).file_name().unwrap().to_str().unwrap(), s.line)).collect();
    assert_eq!(chain, vec!["enemy.tsx:6", "game.tsx:7"]);
    assert_eq!(nb.types, vec!["Enemy"]);

    let body = "\n\n    <Enemy key=\"b\" at={[5, 0, -4]} hp={250} />\n    <Enemy key=\"c\" at={[0, 0, -6]} />\n";
    let r = save(&mut app, &dir, "game.tsx", &game(body));
    assert_eq!(r.status, ReloadStatus::Applied, "{:?}", r.errors);
    let last = app.world().resource::<TsxTree>().last.clone();
    assert_eq!((last.spawned, last.despawned, last.written, last.inserted, last.removed), (2, 1, 1, 0, 0), "c and its eye, a, b's translation");
    let (b2, nb2) = node(&mut app, &["b"]).unwrap();
    assert_eq!(b2, b, "b keeps its entity");
    assert_eq!(x(&app, b), 5.0);
    assert_eq!(nb2.source[1].line, 8, "a blank line moved the call site, not the identity");
    assert!(app.world().get_entity(a).is_err());
    assert!(node(&mut app, &["c"]).is_some());
}

#[test]
fn a_typo_in_a_field_fails_the_reload_with_its_tsx_location() {
    let (dir, mut app) = setup("tsx-typo", "    <Enemy key=\"a\" at={[0, 0, 0]} />\n");
    let r = save(&mut app, &dir, "enemy.tsx", &ENEMY.replace("max={hp}", "mx={hp}"));
    assert_eq!(r.status, ReloadStatus::Failed);
    let e = &r.errors[0];
    assert!(e.message.contains("no field mx"), "{e:?}");
    let loc = e.location.as_ref().expect("located");
    assert!(loc.file.ends_with("enemy.tsx"), "{loc:?}");
    assert_eq!(loc.line, 8);
    assert!(node(&mut app, &["a"]).is_some(), "the world is untouched");
}

#[test]
fn a_throwing_scene_and_a_duplicate_component_fail_with_locations() {
    let (dir, mut app) = setup("tsx-throw", "    <Enemy key=\"a\" at={[0, 0, 0]} />\n");
    let throwing = game("    {(() => { throw new Error(\"no scene today\"); })()}\n");
    let r = save(&mut app, &dir, "game.tsx", &throwing);
    assert_eq!(r.status, ReloadStatus::Failed);
    assert!(r.errors[0].message.contains("no scene today"), "{:?}", r.errors);
    write(&dir, "game.tsx", &game("    <Enemy key=\"a\" at={[0, 0, 0]} />\n"));
    let twice = ENEMY.replace("<Health initialHp={hp} max={hp} />", "<Health initialHp={hp} />\n    <Health max={hp} />");
    let r = save(&mut app, &dir, "enemy.tsx", &twice);
    assert_eq!(r.status, ReloadStatus::Failed);
    assert!(r.errors[0].message.contains("given twice"), "{:?}", r.errors);
    assert_eq!(r.errors[0].location.as_ref().unwrap().line, 9, "{:?}", r.errors);
    assert!(node(&mut app, &["a"]).is_some());
}

#[test]
fn both_spellings_of_a_field_fail_naming_it() {
    let (dir, mut app) = setup("tsx-both", "    <Enemy key=\"a\" at={[0, 0, 0]} />\n");
    let r = save(&mut app, &dir, "enemy.tsx", &ENEMY.replace("max={hp}", "max={hp} hp={hp}"));
    assert_eq!(r.status, ReloadStatus::Failed);
    assert!(r.errors[0].message.contains("game::Health.hp"), "{:?}", r.errors);
}

#[test]
fn a_ts_entry_mounts_and_unmounts_an_enemy_at_runtime() {
    let body = "    <Enemy key=\"a\" at={[0, 0, 0]} />\n";
    let src = format!(
        "{}\napp.entry(\"click\", (payload, ctx) => {{\n  if (text(payload) === \"spawn\") ctx.commands.mount(<Enemy key={{`e${{ctx.frame}}`}} at={{[0, 0, -6]}} />);\n  else ctx.commands.unmount(text(payload));\n}});\n",
        game(body).replace("import { app }", "import { app, text }")
    );
    let dir = project("tsx-mount-runtime", &[("enemy.tsx", ENEMY), ("game.tsx", &src)]);
    let mut app = tsx_app(&dir.join("game.tsx"));
    let r = reload(&mut app, false);
    assert_eq!(r.status, ReloadStatus::Applied, "{:?}", r.errors);
    updates(&mut app, 3);
    bevy_ts::plugin_api::call_entry(app.world_mut(), "click", b"spawn").unwrap();
    let world = app.world_mut();
    let (key, root, chain) = world
        .query::<&TsxNode>()
        .iter(world)
        .find(|n| n.root == "$runtime" && n.key_path.len() == 1)
        .map(|n| (n.key_path[0].clone(), n.root.clone(), n.source.clone()))
        .expect("mounted under the runtime root");
    assert_eq!(root, "$runtime");
    assert!(key.starts_with('e'), "{key}");
    assert!(chain[1].file.ends_with("game.tsx"), "{chain:?}");
    assert!(node(&mut app, &[&key, "eye"]).is_some());
    // Reloads leave it be.
    reload(&mut app, false);
    let (e, _) = node(&mut app, &[&key]).expect("survives a reload");
    bevy_ts::plugin_api::call_entry(app.world_mut(), "click", key.as_bytes()).unwrap();
    assert!(app.world().get_entity(e).is_err(), "unmounted");
}

#[test]
fn a_string_prop_reaches_its_entity_and_a_reload_rewrites_it() {
    let tag = |label: &str| {
        format!(
            "import {{ app, Component }} from \"bevy-ts\";\nexport class Tag extends Component {{ label: string = \"crate\" }}\napp.scene(\"arena\", () => (\n  <>\n    <entity key=\"door\"><Tag label=\"{label}\" /></entity>\n    <entity key=\"box\"><Tag /></entity>\n  </>\n));\n"
        )
    };
    let dir = project("tsx-string", &[("game.tsx", &tag("door"))]);
    let mut app = tsx_app(&dir.join("game.tsx"));
    let r = reload(&mut app, false);
    assert_eq!(r.status, ReloadStatus::Applied, "{:?}", r.errors);
    let (door, _) = node(&mut app, &["door"]).unwrap();
    let (crate_, _) = node(&mut app, &["box"]).unwrap();
    assert_eq!(read_ts(&app, door, "game::Tag").unwrap(), serde_json::json!({ "label": "door" }));
    assert_eq!(read_ts(&app, crate_, "game::Tag").unwrap(), serde_json::json!({ "label": "crate" }));
    let r = save(&mut app, &dir, "game.tsx", &tag("gate"));
    assert_eq!(r.status, ReloadStatus::Applied, "{:?}", r.errors);
    assert_eq!(read_ts(&app, door, "game::Tag").unwrap(), serde_json::json!({ "label": "gate" }), "a controlled field is written in place");
}
