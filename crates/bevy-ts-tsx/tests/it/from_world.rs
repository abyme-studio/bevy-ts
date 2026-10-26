//! A reflect-tier component whose only default is a `FromWorld` that reads a resource: TSX builds
//! it from the live world, as a script's `C.of(...)` does, and one that panics there is not
//! inserted, logged naming its type.

use bevy::prelude::*;
use bevy_ts::BevyTsPlugin;
use bevy_ts::backend::ModuleSet;
use bevy_ts::fake::{FakeHandle, FakeProgram};
use bevy_ts::generation::{self, ReloadReport, ReloadStatus};
use bevy_ts::log::ScriptLog;
use bevy_ts::manifest::Section;
use bevy_ts_tsx::{TsxNode, TsxPlugin};
use serde_json::{Value, json};
use std::path::PathBuf;

#[derive(Resource)]
struct Zoom(f32);

/// Its `scale` comes from `Zoom`, so an empty world cannot build it.
#[derive(Component, Reflect)]
#[reflect(Component, FromWorld)]
struct Lens {
    scale: f32,
    offset: f32,
}

impl FromWorld for Lens {
    fn from_world(world: &mut World) -> Self {
        Lens { scale: world.resource::<Zoom>().0, offset: 0.0 }
    }
}

fn app() -> (App, FakeHandle) {
    let handle = FakeHandle::default();
    let h = handle.clone();
    let mut app = App::new();
    app.add_plugins((MinimalPlugins, AssetPlugin::default()))
        .init_asset::<Mesh>()
        .init_asset::<StandardMaterial>()
        .register_type::<Lens>()
        .add_plugins(BevyTsPlugin::new(move || Box::new(h.backend())))
        .add_plugins(TsxPlugin);
    (app, handle)
}

fn lens(props: Value) -> Value {
    let at = json!({ "file": "lens.tsx", "line": 4, "column": 3 });
    json!({ "roots": [{ "name": "lenses", "location": at, "nodes": [{
        "key": "lens",
        "type": "entity",
        "source": [at],
        "components": [{ "type": "it::from_world::Lens", "kind": "rust", "props": props, "source": at }],
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

#[test]
fn a_from_world_that_reads_a_resource_is_built_from_the_live_world() {
    let (mut app, fake) = app();
    app.insert_resource(Zoom(3.0));
    let r = load(&mut app, &fake, &lens(json!({ "offset": 0.5 })));
    assert_eq!(r.status, ReloadStatus::Applied, "{:?}", r.errors);
    let world = app.world_mut();
    let l = world.query_filtered::<&Lens, With<TsxNode>>().single(world).unwrap();
    assert_eq!((l.scale, l.offset), (3.0, 0.5));
}

#[test]
fn a_from_world_that_panics_is_logged_naming_its_type() {
    let (mut app, fake) = app();
    let r = load(&mut app, &fake, &lens(json!({})));
    assert_eq!(r.status, ReloadStatus::Applied, "{:?}", r.errors);
    let log: Vec<String> = app.world().resource::<ScriptLog>().since(0).map(|e| e.message.clone()).collect();
    assert!(log.iter().any(|m| m.contains("it::from_world::Lens's FromWorld panicked")), "{log:?}");
    let world = app.world_mut();
    assert_eq!(world.query::<&Lens>().iter(world).count(), 0);
}
