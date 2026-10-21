//! Change `asset-catalog`: a `.bsn` level's thumbnail is drawn from a picture of the level that
//! the game, its TS and the protocol never see. The measurement (task 3.4) is ignored:
//! `cargo test --release -p bevy-ts-cli --test viewport_thumbnail -- --ignored --nocapture`.
#![cfg(target_os = "macos")]

mod common;

use bevy::prelude::*;
use bevy::remote::{RemoteMethodSystemId, RemoteMethods, RemotePlugin};
use bevy::window::ExitCondition;
use bevy::winit::WinitPlugin;
use bevy_ts::generation::{DevSource, ReloadStatus};
use bevy_ts_cli::compile::Compiler;
use bevy_ts_levels::engine::{Levels, LevelsPlugin};
use bevy_ts_v8::V8Options;
use bevy_ts_viewport::edit::EditorOnly;
use common::{messages, project, reload};
use serde_json::{Value, json};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};

/// Counts what TS sees with `Foe`, every frame.
const SEEN: &str = "
export function seen(foes: Query<[Transform], With<Foe>>) { console.log(`foes ${foes.len}`); }
app.addSystems(Update, seen);
";

/// The arena's sources and levels, plus `extra` level files, in a project of its own.
fn arena(name: &str, extra: &[(&str, String)]) -> PathBuf {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/arena");
    let mut files: Vec<(String, String)> = ["enemy.tsx", "player.tsx", "hud.tsx", "game.tsx"]
        .iter()
        .map(|f| (format!("src/{f}"), std::fs::read_to_string(root.join("src").join(f)).unwrap()))
        .collect();
    files[3].1.push_str(SEEN);
    for f in ["arena.bsn", "room.bsn"] {
        files.push((format!("assets/levels/{f}"), std::fs::read_to_string(root.join("assets/levels").join(f)).unwrap()));
    }
    files.extend(extra.iter().map(|(p, s)| (p.to_string(), s.clone())));
    files.push(("package.json".into(), "{}".into()));
    let list: Vec<(&str, &str)> = files.iter().map(|(a, b)| (a.as_str(), b.as_str())).collect();
    project(name, &list)
}

fn app(dir: &Path) -> App {
    let assets = dir.join("assets").canonicalize().unwrap();
    let mut app = App::new();
    app.add_plugins(
        DefaultPlugins
            .build()
            .disable::<WinitPlugin>()
            .set(WindowPlugin { primary_window: None, exit_condition: ExitCondition::DontExit, ..default() })
            .set(AssetPlugin { file_path: assets.display().to_string(), ..default() }),
    )
    .add_plugins(RemotePlugin::default())
    .add_plugins(bevy_ts_viewport::ViewportPlugin { windowed: false })
    .add_plugins(bevy_ts::BevyTsPlugin::new(bevy_ts_v8::factory(V8Options::default())).dev(true))
    .add_plugins(bevy_ts_tsx::TsxPlugin)
    .add_plugins(LevelsPlugin)
    .insert_resource(bevy_ts_viewport::assets::AssetRoot(assets.clone()))
    .insert_resource(DevSource(Arc::new(Compiler::new(dir.join("src/game.tsx"), true).unwrap())));
    app.world_mut().resource_mut::<Levels>().asset_root = assets;
    bevy_ts_remote::add_methods(&mut app);
    app.finish();
    app.cleanup();
    assert_eq!(reload(&mut app, false).status, ReloadStatus::Applied);
    for _ in 0..500 {
        app.update();
        let levels = app.world().resource::<Levels>();
        if !levels.roots.is_empty() && levels.roots.values().all(|r| !r.dirty && !r.applied.is_empty()) {
            break;
        }
        std::thread::sleep(Duration::from_millis(1));
    }
    for _ in 0..3 {
        app.update();
    }
    app
}

fn call(app: &mut App, method: &str, params: Value) -> Value {
    let id = match app.world().resource::<RemoteMethods>().get(method) {
        Some(RemoteMethodSystemId::Instant(id)) => *id,
        _ => panic!("{method} not served"),
    };
    app.world_mut().run_system_with(id, (!params.is_null()).then_some(params)).unwrap().unwrap_or_else(|e| panic!("{method}: {}", e.message))
}

fn watch(app: &mut App, method: &str) -> Option<Value> {
    let id = match app.world().resource::<RemoteMethods>().get(method) {
        Some(RemoteMethodSystemId::Watching(id)) => *id,
        _ => panic!("{method} not served"),
    };
    app.world_mut().run_system_with(id, None).unwrap().unwrap()
}

/// The most `Foe`s any TS frame saw since the log was last read.
fn foes_seen(app: &App) -> usize {
    messages(app).iter().filter_map(|m| m.strip_prefix("foes ")).filter_map(|n| n.parse().ok()).max().unwrap_or(0)
}

fn editor_only(app: &mut App) -> Vec<Entity> {
    let world = app.world_mut();
    world.query_filtered::<Entity, With<EditorOnly>>().iter(world).collect()
}

fn decode(b64: &str) -> Image {
    let table = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let (mut bits, mut n, mut out) = (0u32, 0, vec![]);
    for c in b64.bytes().filter(|c| *c != b'=') {
        bits = bits << 6 | table.iter().position(|t| *t == c).unwrap() as u32;
        n += 6;
        if n >= 8 {
            n -= 8;
            out.push((bits >> n) as u8);
        }
    }
    Image::from_buffer(&out, bevy::image::ImageType::Extension("png"), bevy::image::CompressedImageFormats::NONE, true, bevy::image::ImageSampler::Default, bevy::asset::RenderAssetUsages::all()).unwrap()
}

/// Asks until the PNG arrives; returns it, the frames it took and each frame's time.
fn thumbnail(app: &mut App, path: &str, size: u32, mut during: impl FnMut(&mut App)) -> (Image, u32, Vec<f64>) {
    let first = call(app, "bevy-ts/asset_thumbnail", json!({ "path": path, "size": size }));
    assert_eq!(first["pending"], true, "{first}");
    let (mut frames, mut times) = (0, vec![]);
    loop {
        let t = Instant::now();
        app.update();
        times.push(t.elapsed().as_secs_f64() * 1e3);
        frames += 1;
        during(app);
        let r = call(app, "bevy-ts/asset_thumbnail", json!({ "path": path, "size": size }));
        if let Some(p) = r["png"].as_str() {
            return (decode(p), frames, times);
        }
        assert_eq!(r["pending"], true, "{r}");
        assert!(frames < 2000, "the thumbnail never finished");
        std::thread::sleep(Duration::from_millis(1));
    }
}

#[test]
fn the_arena_levels_thumbnail_leaves_the_game_alone() {
    let dir = arena("viewport-thumbnail", &[]);
    let mut app = app(&dir);
    assert_eq!(foes_seen(&app), 4, "{:?}", messages(&app));
    let tree = call(&mut app, "bevy-ts/tsx_tree", Value::Null).to_string();
    watch(&mut app, "bevy-ts/entities+watch").unwrap();
    let editor = editor_only(&mut app);
    let ts: Vec<_> = app.world().resource::<bevy_ts::components::TsComponents>().all().iter().map(|c| c.id).collect();

    let mut pictured = 0;
    let (img, _, _) = thumbnail(&mut app, "levels/arena.bsn", 128, |app| {
        let new: Vec<Entity> = editor_only(app).into_iter().filter(|e| !editor.contains(e)).collect();
        pictured = pictured.max(new.len());
        for e in new {
            let er = app.world().entity(e);
            assert!(ts.iter().all(|id| !er.contains_id(*id)), "a thumbnail entity has a TS component");
            for c in er.archetype().components() {
                let name = app.world().components().get_name(*c).unwrap().to_string();
                assert!(!name.contains("RigidBody") && !name.contains("Collider"), "{name} on a thumbnail entity");
            }
        }
        if let Some(delta) = watch(app, "bevy-ts/entities+watch") {
            assert!(delta["spawned"].as_array().is_none_or(|s| s.is_empty()), "the entity list saw the thumbnail: {delta:#}");
        }
    });
    assert!(pictured > 8, "the picture holds the level's enemies, rooms and their meshes ({pictured})");
    assert_eq!((img.width(), img.height()), (128, 128));
    std::fs::write(Path::new(env!("CARGO_TARGET_TMPDIR")).join("arena-thumbnail.png"), bevy_ts_viewport::assets::png(128, 128, img.data.as_ref().unwrap())).unwrap();
    let red = img.data.as_ref().unwrap().chunks(4).filter(|p| p[3] > 0 && p[0] as u32 > 2 * p[1].max(p[2]) as u32).count();
    assert!(red > 20, "the level's red enemies are drawn ({red} pixels)");

    for _ in 0..4 {
        app.update();
    }
    assert_eq!(foes_seen(&app), 4, "no TS system saw a new Foe");
    assert_eq!(call(&mut app, "bevy-ts/tsx_tree", Value::Null).to_string(), tree, "tsx_tree is unchanged");
    let mut after = editor_only(&mut app);
    let mut before = editor;
    after.sort();
    before.sort();
    assert_eq!(after, before, "every thumbnail entity is gone");
}

#[test]
fn a_level_that_fails_to_compose_has_no_thumbnail() {
    let dir = arena("viewport-thumbnail-broken", &[("assets/levels/broken.bsn", "#A\n@enemy::Nope\n".to_string())]);
    let mut app = app(&dir);
    let errors = app.world().resource::<bevy_ts::log::ScriptLog>().next_seq();
    call(&mut app, "bevy-ts/asset_thumbnail", json!({ "path": "levels/broken.bsn", "size": 64 }));
    let mut r = Value::Null;
    for _ in 0..200 {
        app.update();
        r = call(&mut app, "bevy-ts/asset_thumbnail", json!({ "path": "levels/broken.bsn", "size": 64 }));
        if r["pending"] == false {
            break;
        }
    }
    assert_eq!(r["pending"], false, "{r}");
    assert!(r["png"].is_null(), "{r}");
    let logged: Vec<String> = app.world().resource::<bevy_ts::log::ScriptLog>().since(errors).filter(|e| e.level != bevy_ts::log::LogLevel::Info).map(|e| e.message.clone()).collect();
    assert!(logged.is_empty(), "nothing logged to the game: {logged:?}");
}

/// `n` enemies in a grid: `2n` entities once expanded.
fn grid(n: usize) -> String {
    (0..n)
        .map(|i| format!("#E{i}\n@enemy::Enemy\nTransform {{ translation: Vec3 {{ x: {}.0, z: {}.0 }} }}\n", (i % 10) * 2, (i / 10) * 2))
        .collect::<Vec<_>>()
        .join("--\n")
}

#[test]
#[ignore]
fn measure_level_thumbnails() {
    let dir = arena("viewport-thumbnail-bench", &[("assets/levels/grid.bsn", grid(100))]);
    let mut app = app(&dir);
    // Idle frames, for the frame time the game has without a thumbnail.
    let mut idle = vec![];
    for _ in 0..60 {
        let t = Instant::now();
        app.update();
        idle.push(t.elapsed().as_secs_f64() * 1e3);
    }
    let median = |v: &mut Vec<f64>| {
        v.sort_by(f64::total_cmp);
        v[v.len() / 2]
    };
    println!("| level | entities | compose ms | frames to PNG | frame ms median / max while rendering | idle frame ms |");
    println!("|---|---|---|---|---|---|");
    let idle = median(&mut idle);
    // Both levels loaded first, so the timed compose reads loaded files as a job does.
    let handles: Vec<Handle<bevy_ts_levels::engine::BsnLevel>> = ["levels/arena.bsn", "levels/grid.bsn"].iter().map(|p| app.world().resource::<AssetServer>().load(*p)).collect();
    while !handles.iter().all(|h| app.world().resource::<AssetServer>().is_loaded_with_dependencies(h)) {
        app.update();
    }
    // Three rounds per level; each touches the file so the cache, keyed by mtime, misses.
    for (path, size) in [("levels/arena.bsn", 128), ("levels/grid.bsn", 128)].repeat(3) {
        let file = dir.join("assets").join(path);
        std::thread::sleep(Duration::from_millis(5));
        std::fs::write(&file, std::fs::read(&file).unwrap()).unwrap();
        let t = Instant::now();
        let picture = bevy_ts_levels::picture::spawn_level_picture(app.world_mut(), path, EditorOnly).unwrap();
        let compose = t.elapsed().as_secs_f64() * 1e3;
        let entities = {
            let world = app.world_mut();
            let mut n = 0;
            let mut stack = vec![picture];
            while let Some(e) = stack.pop() {
                n += 1;
                if let Some(c) = world.get::<Children>(e) {
                    stack.extend(c.iter());
                }
            }
            n - 1
        };
        app.world_mut().entity_mut(picture).despawn();
        app.update();
        let (_, frames, mut times) = thumbnail(&mut app, path, size, |_| {});
        let max = times.iter().copied().fold(0.0, f64::max);
        println!("| {path} | {entities} | {compose:.2} | {frames} | {:.2} / {max:.2} | {idle:.2} |", median(&mut times));
    }
}
