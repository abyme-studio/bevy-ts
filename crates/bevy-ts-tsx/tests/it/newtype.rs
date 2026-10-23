//! One-field tuple structs in TSX: `<WorldAssetRoot value="..." />` sets field `0`, a handle
//! loaded from the path; any other prop fails naming `value`.

use bevy::prelude::*;
use bevy::world_serialization::{WorldAsset, WorldAssetRoot};
use bevy_ts::BevyTsPlugin;
use bevy_ts::backend::ModuleSet;
use bevy_ts::fake::{FakeHandle, FakeProgram};
use bevy_ts::generation::{self, ReloadReport, ReloadStatus};
use bevy_ts::manifest::Section;
use bevy_ts_tsx::{TsxNode, TsxPlugin};
use serde_json::{Value, json};
use std::path::PathBuf;

fn app() -> (App, FakeHandle) {
    let handle = FakeHandle::default();
    let h = handle.clone();
    let mut app = App::new();
    app.add_plugins((MinimalPlugins, AssetPlugin::default()))
        .init_asset::<Mesh>()
        .init_asset::<StandardMaterial>()
        .init_asset::<WorldAsset>()
        .register_type::<WorldAssetRoot>()
        .add_plugins(BevyTsPlugin::new(move || Box::new(h.backend())))
        .add_plugins(TsxPlugin);
    (app, handle)
}

fn fox(props: Value) -> Value {
    let at = json!({ "file": "fox.tsx", "line": 7, "column": 3 });
    json!({ "roots": [{ "name": "foxes", "location": at, "nodes": [{
        "key": "fox",
        "type": "entity",
        "source": [at],
        "components": [{ "type": "WorldAssetRoot", "kind": "rust", "props": props, "source": at }],
        "children": []
    }] }] })
}

fn load(app: &mut App, fake: &FakeHandle, section: &Value) -> ReloadReport {
    fake.push(FakeProgram {
        sections: vec![Section { name: "tsx".into(), bytes: serde_json::to_vec(section).unwrap(), location: None }],
        ..Default::default()
    });
    generation::apply(app.world_mut(), ModuleSet { entry: PathBuf::from("/game.tsx"), modules: vec![] }, false)
}

fn the_fox(app: &mut App) -> Entity {
    let world = app.world_mut();
    world.query_filtered::<Entity, With<TsxNode>>().iter(world).next().unwrap()
}

fn path(app: &App, e: Entity) -> String {
    app.world().get::<WorldAssetRoot>(e).unwrap().0.path().unwrap().to_string()
}

#[test]
fn value_sets_the_loaded_handle_and_a_change_writes_it() {
    let (mut app, fake) = app();
    let r = load(&mut app, &fake, &fox(json!({ "value": "models/Fox.glb#Scene0" })));
    assert_eq!(r.status, ReloadStatus::Applied, "{:?}", r.errors);
    let e = the_fox(&mut app);
    assert_eq!(path(&app, e), "models/Fox.glb#Scene0");
    let r = load(&mut app, &fake, &fox(json!({ "value": "models/Other.glb#Scene0" })));
    assert_eq!(r.status, ReloadStatus::Applied, "{:?}", r.errors);
    assert_eq!(the_fox(&mut app), e, "the same entity");
    assert_eq!(path(&app, e), "models/Other.glb#Scene0");
}

#[test]
fn a_field_name_fails_naming_value() {
    let (mut app, fake) = app();
    let r = load(&mut app, &fake, &fox(json!({ "path": "models/Fox.glb#Scene0" })));
    assert_eq!(r.status, ReloadStatus::Failed);
    assert!(r.errors[0].message.contains("WorldAssetRoot takes `value`"), "{}", r.errors[0].message);
    let at = r.errors[0].location.as_ref().unwrap();
    assert_eq!((at.file.as_str(), at.line), ("fox.tsx", 7));
}

#[test]
fn initial_value_is_written_on_insert_only() {
    let (mut app, fake) = app();
    let r = load(&mut app, &fake, &fox(json!({ "initialValue": "models/Fox.glb#Scene0" })));
    assert_eq!(r.status, ReloadStatus::Applied, "{:?}", r.errors);
    let e = the_fox(&mut app);
    assert_eq!(path(&app, e), "models/Fox.glb#Scene0");
    let r = load(&mut app, &fake, &fox(json!({ "initialValue": "models/Other.glb#Scene0" })));
    assert_eq!(r.status, ReloadStatus::Applied, "{:?}", r.errors);
    assert_eq!(path(&app, e), "models/Fox.glb#Scene0", "an initial value is not written again");
}

#[test]
fn value_and_initial_value_together_fail() {
    let (mut app, fake) = app();
    let r = load(&mut app, &fake, &fox(json!({ "value": "a.glb#Scene0", "initialValue": "b.glb#Scene0" })));
    assert_eq!(r.status, ReloadStatus::Failed);
    assert!(r.errors[0].message.contains("pick one"), "{}", r.errors[0].message);
}
