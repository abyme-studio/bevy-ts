//! `<VisibilityRange>` in TSX (visibility-range): Bevy reflects no default for it, so
//! `bevy_ts::type_data` registers one; both margins insert and read back as `{ start, end }`.

use bevy::camera::visibility::VisibilityRange;
use bevy::prelude::*;
use bevy_ts::BevyTsPlugin;
use bevy_ts::backend::ModuleSet;
use bevy_ts::fake::{FakeHandle, FakeProgram};
use bevy_ts::generation::{self, ReloadStatus};
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
        .register_type::<VisibilityRange>()
        .add_plugins(BevyTsPlugin::new(move || Box::new(h.backend())))
        .add_plugins(TsxPlugin);
    (app, handle)
}

fn scene(props: Value) -> Value {
    let at = json!({ "file": "lod.tsx", "line": 4, "column": 3 });
    json!({ "roots": [{ "name": "lod", "location": at, "nodes": [{
        "key": "detail",
        "type": "entity",
        "source": [at],
        "components": [{ "type": "VisibilityRange", "kind": "rust", "props": props, "source": at }],
        "children": []
    }] }] })
}

#[test]
fn both_margins_insert_and_read_back() {
    let (mut app, fake) = app();
    let props = json!({ "start_margin": { "start": 0.0, "end": 10.0 }, "end_margin": { "start": 40.0, "end": 50.0 } });
    fake.push(FakeProgram {
        sections: vec![Section { name: "tsx".into(), bytes: serde_json::to_vec(&scene(props)).unwrap(), location: None }],
        ..Default::default()
    });
    let r = generation::apply(app.world_mut(), ModuleSet { entry: PathBuf::from("/game.tsx"), modules: vec![] }, false);
    assert_eq!(r.status, ReloadStatus::Applied, "{:?}", r.errors);
    let world = app.world_mut();
    let e = world.query_filtered::<Entity, With<TsxNode>>().iter(world).next().unwrap();
    let range = world.get::<VisibilityRange>(e).unwrap();
    assert_eq!((range.start_margin.clone(), range.end_margin.clone(), range.use_aabb), (0.0..10.0, 40.0..50.0, false));
    let registry = world.resource::<AppTypeRegistry>().read();
    let read = bevy_ts::reflect_value::read(&registry, range).unwrap();
    assert_eq!(read["start_margin"], json!({ "start": 0.0, "end": 10.0 }));
    assert_eq!(read["end_margin"], json!({ "start": 40.0, "end": 50.0 }));
}
