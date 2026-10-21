//! The asset viewer's methods: the list, its watch, and thumbnails that leave the game alone.
#![cfg(target_os = "macos")]

mod common;

use bevy::prelude::*;
use bevy::remote::{RemoteMethodSystemId, RemoteMethods, RemotePlugin};
use bevy::window::ExitCondition;
use bevy::winit::WinitPlugin;
use bevy_ts_viewport::ViewportPlugin;
use bevy_ts_viewport::assets::{AssetRoot, png};
use common::{scene, validate};
use serde_json::{Value, json};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

fn root(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join("bevy-ts-viewport-assets").join(name);
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join("textures")).unwrap();
    std::fs::create_dir_all(dir.join("models")).unwrap();
    dir
}

fn write_png(path: &Path, w: u32, h: u32, rgba: [u8; 4]) {
    let data: Vec<u8> = (0..w * h).flat_map(|_| rgba).collect();
    std::fs::write(path, png(w, h, &data)).unwrap();
}

/// A one-mesh glTF: a unit cube's eight corners as a triangle list, buffer inline.
fn write_gltf(path: &Path) {
    let p: [[f32; 3]; 8] = [[-0.5, -0.5, -0.5], [0.5, -0.5, -0.5], [0.5, 0.5, -0.5], [-0.5, 0.5, -0.5], [-0.5, -0.5, 0.5], [0.5, -0.5, 0.5], [0.5, 0.5, 0.5], [-0.5, 0.5, 0.5]];
    let idx: [u16; 36] = [0, 2, 1, 0, 3, 2, 4, 5, 6, 4, 6, 7, 0, 1, 5, 0, 5, 4, 3, 7, 6, 3, 6, 2, 0, 4, 7, 0, 7, 3, 1, 2, 6, 1, 6, 5];
    let mut buf = vec![];
    for v in p {
        for c in v {
            buf.extend_from_slice(&c.to_le_bytes());
        }
    }
    for i in idx {
        buf.extend_from_slice(&i.to_le_bytes());
    }
    let uri = format!("data:application/octet-stream;base64,{}", bevy_ts_viewport::assets::base64(&buf));
    let doc = json!({
        "asset": { "version": "2.0" },
        "scene": 0, "scenes": [{ "nodes": [0] }], "nodes": [{ "mesh": 0 }],
        "meshes": [{ "primitives": [{ "attributes": { "POSITION": 0 }, "indices": 1 }] }],
        "buffers": [{ "byteLength": buf.len(), "uri": uri }],
        "bufferViews": [{ "buffer": 0, "byteOffset": 0, "byteLength": 96 }, { "buffer": 0, "byteOffset": 96, "byteLength": 72 }],
        "accessors": [
            { "bufferView": 0, "componentType": 5126, "count": 8, "type": "VEC3", "min": [-0.5, -0.5, -0.5], "max": [0.5, 0.5, 0.5] },
            { "bufferView": 1, "componentType": 5123, "count": 36, "type": "SCALAR" }
        ]
    });
    std::fs::write(path, doc.to_string()).unwrap();
}

fn app(dir: &Path) -> App {
    let mut app = App::new();
    app.add_plugins(
        DefaultPlugins
            .build()
            .disable::<WinitPlugin>()
            .set(WindowPlugin { primary_window: None, exit_condition: ExitCondition::DontExit, ..default() })
            .set(AssetPlugin { file_path: dir.display().to_string(), ..default() }),
    )
    .add_plugins(RemotePlugin::default())
    .add_plugins(ViewportPlugin::default())
    .insert_resource(AssetRoot(dir.to_path_buf()))
    .add_systems(Startup, scene);
    app.finish();
    app.cleanup();
    app.update();
    app
}

fn call(app: &mut App, method: &str, params: Value) -> Value {
    let id = match app.world().resource::<RemoteMethods>().get(method) {
        Some(RemoteMethodSystemId::Instant(id)) => *id,
        _ => panic!("{method} not served"),
    };
    let r = app.world_mut().run_system_with(id, (!params.is_null()).then_some(params)).unwrap().unwrap_or_else(|e| panic!("{method}: {}", e.message));
    validate(method, &r);
    r
}

fn watch(app: &mut App) -> Option<Value> {
    let id = match app.world().resource::<RemoteMethods>().get("bevy-ts/assets+watch") {
        Some(RemoteMethodSystemId::Watching(id)) => *id,
        _ => panic!("no assets watch"),
    };
    let r = app.world_mut().run_system_with(id, None).unwrap().unwrap();
    if let Some(v) = &r {
        validate("bevy-ts/assets+watch", v);
    }
    r
}

#[test]
fn the_list_names_kind_loader_and_load_state_and_loads_nothing() {
    let dir = root("list");
    write_png(&dir.join("textures/floor.png"), 8, 8, [10, 200, 10, 255]);
    write_gltf(&dir.join("models/enemy.gltf"));
    let mut app = app(&dir);
    let r = call(&mut app, "bevy-ts/assets", Value::Null);
    let by: std::collections::BTreeMap<String, Value> = r["assets"].as_array().unwrap().iter().map(|a| (a["path"].as_str().unwrap().to_string(), a.clone())).collect();
    let floor = &by["textures/floor.png"];
    assert_eq!(floor["kind"], "image");
    assert_eq!(floor["loader"], "ImageLoader");
    assert_eq!(floor["loadState"], "not_loaded");
    let enemy = &by["models/enemy.gltf"];
    assert_eq!(enemy["kind"], "mesh");
    assert_eq!(enemy["loader"], "GltfLoader");
    assert_eq!(enemy["loadState"], "not_loaded");
    assert!(app.world().resource::<AssetServer>().get_path_id("models/enemy.gltf").is_none(), "listing loads nothing");
    let meshes = call(&mut app, "bevy-ts/assets", json!({ "kind": "mesh" }));
    assert_eq!(meshes["assets"].as_array().unwrap().len(), 1);
}

#[test]
fn a_dropped_texture_arrives_on_the_watch() {
    let dir = root("watch");
    write_png(&dir.join("textures/floor.png"), 4, 4, [0, 0, 0, 255]);
    let mut app = app(&dir);
    let first = watch(&mut app).unwrap();
    assert_eq!(first["assets"].as_array().unwrap().len(), 1);
    app.update();
    assert!(watch(&mut app).is_none(), "quiet while nothing changes");
    write_png(&dir.join("textures/wall.png"), 4, 4, [255, 0, 0, 255]);
    let t0 = Instant::now();
    let msg = loop {
        std::thread::sleep(Duration::from_millis(30));
        app.update();
        if let Some(m) = watch(&mut app) {
            break m;
        }
        assert!(t0.elapsed() < Duration::from_secs(5), "no change arrived");
    };
    let c = &msg["changes"][0];
    assert_eq!(c["path"], "textures/wall.png");
    assert_eq!(c["change"], "added");
    assert_eq!(c["asset"]["kind"], "image");
    assert_eq!(c["asset"]["loadState"], "not_loaded");
}

fn decode(b64: &str) -> Image {
    // base64 back to bytes, then Bevy's PNG decoder.
    let table = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut bits = 0u32;
    let mut n = 0;
    let mut out = vec![];
    for c in b64.bytes().filter(|c| *c != b'=') {
        bits = bits << 6 | table.iter().position(|t| *t == c).unwrap() as u32;
        n += 6;
        if n >= 8 {
            n -= 8;
            out.push((bits >> n) as u8);
        }
    }
    Image::from_buffer(
        &out,
        bevy::image::ImageType::Extension("png"),
        bevy::image::CompressedImageFormats::NONE,
        true,
        bevy::image::ImageSampler::Default,
        bevy::asset::RenderAssetUsages::all(),
    )
    .unwrap()
}

#[test]
fn thumbnails_of_an_image_and_a_gltf_leave_the_game_alone() {
    let dir = root("thumbs");
    write_png(&dir.join("textures/floor.png"), 32, 16, [10, 200, 10, 255]);
    write_gltf(&dir.join("models/enemy.gltf"));
    let mut app = app(&dir);
    let mut cameras_before: Vec<(Entity, bool, isize)> = app.world_mut().query::<(Entity, &Camera)>().iter(app.world()).map(|(e, c)| (e, c.is_active, c.order)).collect();
    cameras_before.sort();

    let t = call(&mut app, "bevy-ts/asset_thumbnail", json!({ "path": "textures/floor.png", "size": 64 }));
    let img = decode(t["png"].as_str().unwrap());
    assert_eq!((img.width(), img.height()), (64, 64));
    let px = |img: &Image, x: u32, y: u32| img.data.as_ref().unwrap()[((y * img.width() + x) * 4) as usize..][..4].to_vec();
    assert_eq!(px(&img, 32, 32)[1], 200, "the middle is the texture");
    assert_eq!(px(&img, 32, 2)[3], 0, "letterboxed on clear");

    let t0 = Instant::now();
    let png = loop {
        let t = call(&mut app, "bevy-ts/asset_thumbnail", json!({ "path": "models/enemy.gltf", "size": 128 }));
        if let Some(p) = t["png"].as_str() {
            break p.to_string();
        }
        assert_eq!(t["pending"], true);
        assert!(t0.elapsed() < Duration::from_secs(60), "the thumbnail never finished");
        std::thread::sleep(Duration::from_millis(5));
        app.update();
    };
    let img = decode(&png);
    assert_eq!((img.width(), img.height()), (128, 128));
    assert!(px(&img, 64, 64)[3] > 0, "the model is in the middle");
    assert_eq!(px(&img, 1, 1)[3], 0, "clear around it");
    for _ in 0..4 {
        app.update();
    }
    let mut cameras_after: Vec<(Entity, bool, isize)> = app.world_mut().query::<(Entity, &Camera)>().iter(app.world()).map(|(e, c)| (e, c.is_active, c.order)).collect();
    cameras_after.sort();
    assert_eq!(cameras_before, cameras_after, "the game's cameras are untouched and the thumbnail camera is gone");
    // Cached by path and mtime.
    let again = call(&mut app, "bevy-ts/asset_thumbnail", json!({ "path": "models/enemy.gltf", "size": 128 }));
    assert_eq!(again["png"].as_str(), Some(png.as_str()));
}

// ---- the arena's example assets ----

fn arena_assets() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/arena/assets")
}

/// A crate: a unit cube with per-face normals and one PBR material, buffer inline.
fn crate_gltf() -> String {
    let faces: [([f32; 3], [f32; 3], [f32; 3]); 6] = [
        ([1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]),
        ([-1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, -1.0]),
        ([0.0, 1.0, 0.0], [0.0, 0.0, 1.0], [1.0, 0.0, 0.0]),
        ([0.0, -1.0, 0.0], [0.0, 0.0, -1.0], [1.0, 0.0, 0.0]),
        ([0.0, 0.0, 1.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]),
        ([0.0, 0.0, -1.0], [-1.0, 0.0, 0.0], [0.0, 1.0, 0.0]),
    ];
    let (mut pos, mut nrm, mut idx) = (vec![], vec![], vec![]);
    for (n, u, v) in faces {
        let n = Vec3::from(n);
        let (u, v) = (Vec3::from(u), Vec3::from(v));
        // Counter-clockwise seen from outside: u × v is the normal.
        let base = (pos.len()) as u16;
        for (a, b) in [(-1.0, -1.0), (1.0, -1.0), (1.0, 1.0), (-1.0, 1.0)] {
            pos.push((n + u * a + v * b) * 0.5);
            nrm.push(n);
        }
        idx.extend([base, base + 1, base + 2, base, base + 2, base + 3]);
    }
    let mut buf = vec![];
    for p in pos.iter().chain(&nrm) {
        for c in p.to_array() {
            buf.extend_from_slice(&c.to_le_bytes());
        }
    }
    for i in &idx {
        buf.extend_from_slice(&i.to_le_bytes());
    }
    let vbytes = pos.len() * 12;
    let uri = format!("data:application/octet-stream;base64,{}", bevy_ts_viewport::assets::base64(&buf));
    let doc = json!({
        "asset": { "version": "2.0", "generator": "bevy-ts-viewport tests/assets.rs" },
        "scene": 0, "scenes": [{ "nodes": [0] }], "nodes": [{ "name": "crate", "mesh": 0 }],
        "materials": [{ "name": "wood", "pbrMetallicRoughness": { "baseColorFactor": [0.55, 0.36, 0.18, 1.0], "metallicFactor": 0.0, "roughnessFactor": 0.8 } }],
        "meshes": [{ "name": "crate", "primitives": [{ "attributes": { "POSITION": 0, "NORMAL": 1 }, "indices": 2, "material": 0 }] }],
        "buffers": [{ "byteLength": buf.len(), "uri": uri }],
        "bufferViews": [
            { "buffer": 0, "byteOffset": 0, "byteLength": vbytes },
            { "buffer": 0, "byteOffset": vbytes, "byteLength": vbytes },
            { "buffer": 0, "byteOffset": vbytes * 2, "byteLength": idx.len() * 2 }
        ],
        "accessors": [
            { "bufferView": 0, "componentType": 5126, "count": pos.len(), "type": "VEC3", "min": [-0.5, -0.5, -0.5], "max": [0.5, 0.5, 0.5] },
            { "bufferView": 1, "componentType": 5126, "count": nrm.len(), "type": "VEC3" },
            { "bufferView": 2, "componentType": 5123, "count": idx.len(), "type": "SCALAR" }
        ]
    });
    serde_json::to_string_pretty(&doc).unwrap() + "\n"
}

/// A 64×64 checker of 8-pixel squares.
fn checker_png() -> Vec<u8> {
    let rgba: Vec<u8> = (0..64 * 64).flat_map(|i| if ((i % 64) / 8 + (i / 64) / 8) % 2 == 0 { [220, 220, 220, 255] } else { [60, 60, 60, 255] }).collect();
    png(64, 64, &rgba)
}

/// A 0.2 s 880 Hz sine, 16-bit mono at 22050 Hz, faded out.
fn hit_wav() -> Vec<u8> {
    let rate = 22050u32;
    let n = rate / 5;
    let samples: Vec<i16> = (0..n).map(|i| {
        let t = i as f32 / rate as f32;
        let fade = 1.0 - i as f32 / n as f32;
        ((t * 880.0 * std::f32::consts::TAU).sin() * fade * 0.5 * i16::MAX as f32) as i16
    }).collect();
    let data = samples.len() as u32 * 2;
    let mut out = vec![];
    out.extend_from_slice(b"RIFF");
    out.extend_from_slice(&(36 + data).to_le_bytes());
    out.extend_from_slice(b"WAVEfmt ");
    out.extend_from_slice(&16u32.to_le_bytes());
    out.extend_from_slice(&1u16.to_le_bytes());
    out.extend_from_slice(&1u16.to_le_bytes());
    out.extend_from_slice(&rate.to_le_bytes());
    out.extend_from_slice(&(rate * 2).to_le_bytes());
    out.extend_from_slice(&2u16.to_le_bytes());
    out.extend_from_slice(&16u16.to_le_bytes());
    out.extend_from_slice(b"data");
    out.extend_from_slice(&data.to_le_bytes());
    for s in samples {
        out.extend_from_slice(&s.to_le_bytes());
    }
    out
}

/// The arena's model, texture and sound are made here. With `BEVY_TS_WRITE_ARENA_ASSETS=1` this
/// writes them; otherwise it checks the committed files are what it makes.
#[test]
fn the_arenas_assets_are_generated_here() {
    let dir = arena_assets();
    let files: [(&str, Vec<u8>); 3] = [("models/crate.gltf", crate_gltf().into_bytes()), ("textures/checker.png", checker_png()), ("sounds/hit.wav", hit_wav())];
    for (path, bytes) in files {
        let full = dir.join(path);
        if std::env::var_os("BEVY_TS_WRITE_ARENA_ASSETS").is_some() {
            std::fs::create_dir_all(full.parent().unwrap()).unwrap();
            std::fs::write(&full, &bytes).unwrap();
        }
        let on_disk = std::fs::read(&full).unwrap_or_default();
        // JSON by value: key order follows serde_json's features, which a workspace build unifies.
        let same = if path.ends_with(".gltf") { serde_json::from_slice::<Value>(&on_disk).ok() == serde_json::from_slice::<Value>(&bytes).ok() } else { on_disk == bytes };
        assert!(same, "{path} is stale: rerun with BEVY_TS_WRITE_ARENA_ASSETS=1");
    }
}

#[test]
fn the_arena_lists_every_kind_and_draws_the_crate() {
    let mut app = app(&arena_assets());
    let r = call(&mut app, "bevy-ts/assets", Value::Null);
    let by: std::collections::BTreeMap<String, Value> = r["assets"].as_array().unwrap().iter().map(|a| (a["path"].as_str().unwrap().to_string(), a.clone())).collect();
    for (path, kind, thumbnail) in [("models/crate.gltf", "mesh", true), ("textures/checker.png", "image", true), ("sounds/hit.wav", "audio", false), ("levels/arena.bsn", "scene", false)] {
        // No LevelsPlugin in this app: a level gets no thumbnail here.
        assert_eq!(by[path]["kind"], kind, "{path}");
        assert_eq!(by[path]["thumbnail"], thumbnail, "{path}");
    }
    let t = call(&mut app, "bevy-ts/asset_thumbnail", json!({ "path": "sounds/hit.wav", "size": 64 }));
    assert!(t["png"].is_null() && t["pending"] == false, "{t}");
    let t0 = Instant::now();
    let png = loop {
        let t = call(&mut app, "bevy-ts/asset_thumbnail", json!({ "path": "models/crate.gltf", "size": 128 }));
        if let Some(p) = t["png"].as_str() {
            break p.to_string();
        }
        assert!(t0.elapsed() < Duration::from_secs(60), "the crate's thumbnail never finished");
        std::thread::sleep(Duration::from_millis(5));
        app.update();
    };
    let img = decode(&png);
    let p = &img.data.as_ref().unwrap()[((64 * 128 + 64) * 4) as usize..][..4];
    assert!(p[3] > 0 && p[0] > p[2], "the crate is drawn, brown: {p:?}");
}
