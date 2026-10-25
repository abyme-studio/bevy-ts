//! A save and a load of 10k `Save`-marked entities, each with `Transform` and two TS components
//! (openspec world-saves task 5.3). Prints one line: the main thread's share of each (the save's
//! walk and serialise in the op, the load's write into the world in one frame), the file's size,
//! and each end to end (with the `IoTaskPool`'s write, or read and deserialise).

use bevy::asset::AssetPlugin;
use bevy::prelude::*;
use bevy_ts::BevyTsPlugin;
use bevy_ts::backend::ModuleSet;
use bevy_ts::commands::insert_bytes;
use bevy_ts::fake::{FakeHandle, FakeProgram};
use bevy_ts::generation::{self, ReloadStatus};
use bevy_ts::layout::FieldType;
use bevy_ts::manifest::{ComponentDecl, FieldDecl, Manifest};
use bevy_ts::plugin_api;
use bevy_ts::saves::{Save, SaveDir, WorldLoaded, WorldSaved};
use serde_json::{Value, json};
use std::path::PathBuf;
use std::time::{Duration, Instant};

const ENTITIES: usize = 10_000;

fn field(name: &str, ty: FieldType) -> FieldDecl {
    FieldDecl { name: name.into(), ty, default: None, hints: Default::default() }
}

fn ms(d: Duration) -> f64 {
    d.as_secs_f64() * 1e3
}

fn insert(world: &mut World, e: Entity, c: &str, v: Value) {
    let (id, _) = plugin_api::component_by_name(world, c).unwrap();
    let bytes = plugin_api::encode_component(world, c, v.as_object().unwrap()).unwrap();
    // SAFETY: encoded with the component's own layout.
    unsafe { insert_bytes(&mut world.entity_mut(e), id, &bytes) };
}

fn wait<M: Message>(app: &mut App) -> Duration {
    let mut cursor = app.world().resource::<Messages<M>>().get_cursor();
    let started = Instant::now();
    loop {
        let t = Instant::now();
        app.update();
        let frame = t.elapsed();
        if cursor.read(app.world().resource::<Messages<M>>()).next().is_some() {
            return frame;
        }
        assert!(started.elapsed() < Duration::from_secs(120), "timed out");
    }
}

fn main() {
    let dir = std::env::temp_dir().join(format!("world-saves-bench-{}", std::process::id()));
    let fake = FakeHandle::default();
    let h = fake.clone();
    let mut app = App::new();
    app.add_plugins(MinimalPlugins)
        .add_plugins(AssetPlugin::default())
        .add_plugins(BevyTsPlugin::new(move || Box::new(h.backend())));
    app.register_type::<Name>().insert_resource(SaveDir(dir.clone()));
    fake.push(FakeProgram {
        manifest: Manifest {
            components: vec![
                ComponentDecl { name: "game::Health".into(), fields: vec![field("hp", FieldType::F32), field("max", FieldType::F32)], ..Default::default() },
                ComponentDecl { name: "game::Mover".into(), fields: vec![field("velocity", FieldType::Vec3), field("target", FieldType::Entity)], ..Default::default() },
            ],
            ..Default::default()
        },
        ..Default::default()
    });
    let r = generation::apply(app.world_mut(), ModuleSet { entry: PathBuf::from("/game.ts"), modules: vec![] }, false);
    assert_eq!(r.status, ReloadStatus::Applied, "{:?}", r.errors);

    let world = app.world_mut();
    let mut last = Entity::PLACEHOLDER;
    for i in 0..ENTITIES {
        let e = world.spawn((Save, Transform::from_xyz(i as f32, 0.0, -(i as f32)))).id();
        insert(world, e, "game::Health", json!({ "hp": i % 100, "max": 100 }));
        insert(world, e, "game::Mover", json!({ "velocity": [1, 0, i % 7], "target": last.to_bits() }));
        last = e;
    }
    app.update();

    let t = Instant::now();
    bevy_ts::saves::save(app.world_mut(), "bench", Default::default());
    let save_main = t.elapsed();
    wait::<WorldSaved>(&mut app);
    let save_total = t.elapsed();
    let saved = app.world().resource::<Messages<WorldSaved>>().iter_current_update_messages().last().cloned();
    assert_eq!(saved.and_then(|s| s.error), None);
    let bytes = std::fs::metadata(dir.join("bench.scn.ron")).unwrap().len();

    let world = app.world_mut();
    let all: Vec<Entity> = world.query_filtered::<Entity, With<Save>>().iter(world).collect();
    for e in all {
        world.despawn(e);
    }
    let t = Instant::now();
    bevy_ts::saves::load(app.world_mut(), "bench", None);
    let load_main = wait::<WorldLoaded>(&mut app);
    let load_total = t.elapsed();
    let world = app.world_mut();
    assert_eq!(world.query_filtered::<Entity, With<Save>>().iter(world).count(), ENTITIES);

    println!(
        "entities={ENTITIES} save_main_ms={:.1} save_total_ms={:.1} file_kb={} load_main_ms={:.1} load_total_ms={:.1}",
        ms(save_main),
        ms(save_total),
        bytes / 1024,
        ms(load_main),
        ms(load_total)
    );
    let _ = std::fs::remove_dir_all(dir);
}
