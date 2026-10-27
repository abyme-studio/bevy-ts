//! Change `asset-preview`: several viewports per engine, and previews of meshes and glTF scenes
//! in scenes of their own, against the in-process test viewer.
#![cfg(target_os = "macos")]

use crate::common;

use bevy::camera::visibility::RenderLayers;
use bevy::prelude::*;
use bevy_ts_viewport::mach::Viewer;
use bevy_ts_viewport::preview::{PREVIEW_LAYERS, PreviewCamera, PreviewKeepAlive, PreviewOnly};
use common::*;
use serde_json::{Value, json};
use std::time::Duration;

/// Updates until both viewers have `n` frames.
fn run_both(app: &mut App, a: (&Viewer, &mut Seen), b: (&Viewer, &mut Seen), n: usize) {
    let t0 = std::time::Instant::now();
    while a.1.frames.len() < n || b.1.frames.len() < n {
        assert!(t0.elapsed() < LONG, "timed out: {} and {} frames", a.1.frames.len(), b.1.frames.len());
        app.update();
        pump(a.0, a.1, Duration::from_millis(1));
        pump(b.0, b.1, Duration::from_millis(1));
    }
}

/// Updates until `n` previews' glTF scenes have spawned: each test model's mesh node is `cube`.
fn until_spawned(app: &mut App, n: usize) {
    let t0 = std::time::Instant::now();
    loop {
        let cubes = app.world_mut().query_filtered::<&Name, With<PreviewOnly>>().iter(app.world()).filter(|n| n.as_str() == "cube").count();
        if cubes >= n {
            break;
        }
        assert!(t0.elapsed() < LONG, "{cubes} of {n} preview scenes spawned");
        app.update();
    }
}

fn preview_camera(app: &mut App) -> Transform {
    *app.world_mut().query_filtered::<&Transform, With<PreviewCamera>>().single(app.world()).unwrap()
}

#[test]
fn a_preview_viewport_runs_beside_the_game_and_input_moves_only_its_camera() {
    let dir = asset_root("beside");
    write_gltf(&dir.join("models/grunt.gltf"), &[]);
    let mut app = app_at(&dir);
    let (game_id, game) = attach_view(&mut app, 320.0, 240.0, json!("game"));
    assert_eq!(game_id, 0);
    let p = open_preview(&mut app, "models/grunt.gltf");
    assert_eq!(p["kind"], "mesh");
    let id = p["preview"].as_u64().unwrap();
    let (vid, preview) = attach_view(&mut app, 200.0, 100.0, json!({ "preview": id }));
    assert_ne!(vid, 0);
    let (mut gs, mut ps) = (Seen::default(), Seen::default());
    run_both(&mut app, (&game, &mut gs), (&preview, &mut ps), 3);
    assert_eq!(gs.frames.last().unwrap()["w"], 320);
    assert_eq!((ps.frames.last().unwrap()["w"].as_u64(), ps.frames.last().unwrap()["h"].as_u64()), (Some(200), Some(100)));
    assert_eq!(ps.surfaces.len(), 3);

    // Each viewport counts its own frames and reports its own size.
    let gstats = call(&mut app, "bevy-ts/viewport_stats", Value::Null).unwrap();
    let pstats = call(&mut app, "bevy-ts/viewport_stats", json!({ "viewport": vid })).unwrap();
    assert_eq!(gstats["size"]["pixelW"], 320);
    assert_eq!(pstats["size"]["pixelW"], 200);
    assert!(pstats["frames"].as_u64().unwrap() >= 3);

    // A drag on the preview's channel orbits the preview camera; the game's input sees nothing.
    let cursor_before = app.world_mut().query_filtered::<&Window, With<bevy_ts_viewport::VirtualWindow>>().single(app.world()).unwrap().cursor_position();
    let before = preview_camera(&mut app);
    for (kind, x) in [("down", 10.0), ("move", 60.0), ("up", 60.0)] {
        preview.send("input", json!({ "type": "pointer", "kind": kind, "x": x, "y": 10.0, "button": "left" })).unwrap();
    }
    let n = ps.frames.len() + 2;
    run_both(&mut app, (&game, &mut gs), (&preview, &mut ps), n);
    let after = preview_camera(&mut app);
    assert!(before.rotation.angle_between(after.rotation) > 0.1, "the preview camera orbited");
    let cursor_after = app.world_mut().query_filtered::<&Window, With<bevy_ts_viewport::VirtualWindow>>().single(app.world()).unwrap().cursor_position();
    assert_eq!(cursor_before, cursor_after, "the game window saw no pointer");

    // HTTP input and a camera pose by viewport id; `f` reframes.
    call(&mut app, "bevy-ts/viewport_input", json!({ "viewport": vid, "events": [{ "type": "wheel", "dx": 0, "dy": 3, "unit": "line" }] })).unwrap();
    app.update();
    let pose = call(&mut app, "bevy-ts/viewport_camera", json!({ "viewport": vid })).unwrap();
    let t = pose["translation"].as_array().unwrap().iter().map(|v| v.as_f64().unwrap()).collect::<Vec<_>>();
    let near = (t[0] * t[0] + t[1] * t[1] + t[2] * t[2]).sqrt();
    let framed = call(&mut app, "bevy-ts/viewport_camera", json!({ "viewport": vid, "frame": true })).unwrap();
    let t = framed["translation"].as_array().unwrap().iter().map(|v| v.as_f64().unwrap()).collect::<Vec<_>>();
    assert!(((t[0] * t[0] + t[1] * t[1] + t[2] * t[2]).sqrt() - near).abs() > 0.05, "the wheel zoomed and f reframed");

    // Detaching the preview leaves the game's viewport attached.
    let d = call(&mut app, "bevy-ts/viewport_detach", json!({ "viewport": vid })).unwrap();
    assert_eq!(d["detached"], true);
    assert_eq!(call(&mut app, "bevy-ts/viewport_stats", Value::Null).unwrap()["attached"], true);
}

#[test]
fn a_client_without_views_gets_the_game_viewport() {
    let mut app = app();
    let r = call(&mut app, "bevy-ts/viewport_attach", json!({ "w": 300, "h": 200, "scale": 1 })).unwrap();
    assert_eq!(r["viewport"], 0);
    let viewer = Viewer::connect(r["service"].as_str().unwrap()).unwrap();
    call(&mut app, "bevy-ts/viewport_resize", json!({ "w": 400, "h": 200, "scale": 1 })).unwrap();
    let mut seen = Seen::default();
    run_until(&mut app, &viewer, &mut seen, LONG, |s| s.frames.last().is_some_and(|f| f["w"] == 400));
    assert!(call(&mut app, "bevy-ts/viewport_stats", json!({ "viewport": 7 })).is_err(), "an unknown viewport is an error");
    assert_eq!(call(&mut app, "bevy-ts/viewport_detach", Value::Null).unwrap()["detached"], true);
}

#[test]
fn the_games_frames_are_byte_identical_with_a_preview_open() {
    let dir = asset_root("identical");
    write_gltf(&dir.join("models/grunt.gltf"), &[]);
    let mut app = app_at(&dir);
    let (_, game) = attach_view(&mut app, 160.0, 120.0, json!("game"));
    let grid: Vec<(u32, u32)> = (0..120).step_by(3).flat_map(|y| (0..160).step_by(3).map(move |x| (x, y))).collect();
    let mut seen = Seen::default();
    run_until(&mut app, &game, &mut seen, LONG, |s| s.frames.len() >= 4);
    let read = |seen: &Seen| {
        let f = seen.frames.last().unwrap();
        seen.surfaces[&(f["ring"].as_u64().unwrap(), f["slot"].as_u64().unwrap())].read(&grid)
    };
    let alone = read(&seen);
    let id = open_preview(&mut app, "models/grunt.gltf")["preview"].as_u64().unwrap();
    let (_, preview) = attach_view(&mut app, 160.0, 120.0, json!({ "preview": id }));
    let mut ps = Seen::default();
    let n = seen.frames.len() + 6;
    run_both(&mut app, (&game, &mut seen), (&preview, &mut ps), n);
    assert_eq!(alone, read(&seen), "the game's pixels are unchanged with a preview open");
    // And the preview shows something: the model, lit, over the clear colour.
    let f = ps.frames.last().unwrap();
    let px = ps.surfaces[&(f["ring"].as_u64().unwrap(), f["slot"].as_u64().unwrap())].read(&[(80, 60), (2, 2)]);
    assert_ne!(px[0], px[1], "the model is in the middle of the preview");
}

#[test]
fn preview_entities_are_hidden_on_their_layer_and_an_eleventh_evicts_the_oldest() {
    let dir = asset_root("evict");
    for i in 0..11 {
        write_gltf(&dir.join(format!("models/m{i}.gltf")), &[]);
    }
    let mut app = app_at(&dir);
    // Eviction, not the keep-alive: on a loaded machine opening ten outlasts it.
    app.insert_resource(PreviewKeepAlive(Duration::MAX));
    let mut ids = vec![];
    for i in 0..10 {
        ids.push(open_preview(&mut app, &format!("models/m{i}.gltf"))["preview"].as_u64().unwrap());
    }
    until_spawned(&mut app, 10);
    // Every entity under a preview is PreviewOnly (so ScriptHidden) and only on a preview layer.
    let (mut members, mut off_layer) = (0, 0);
    for layers in app.world_mut().query_filtered::<Option<&RenderLayers>, With<PreviewOnly>>().iter(app.world()) {
        members += 1;
        if layers.is_some_and(|l| PREVIEW_LAYERS.clone().all(|p| !l.intersects(&RenderLayers::layer(p)))) {
            off_layer += 1;
        }
    }
    assert!(members >= 10 * 6, "roots, scenes, lights, cameras and windows: {members}");
    assert_eq!(off_layer, 0);
    let hidden = app.world_mut().query_filtered::<(), (With<PreviewOnly>, Without<bevy_ts::ScriptHidden>)>().iter(app.world()).count();
    assert_eq!(hidden, 0, "every preview entity is ScriptHidden");
    let on_preview_layers = app
        .world_mut()
        .query_filtered::<&RenderLayers, Without<PreviewOnly>>()
        .iter(app.world())
        .filter(|l| PREVIEW_LAYERS.clone().any(|p| l.intersects(&RenderLayers::layer(p))))
        .count();
    assert_eq!(on_preview_layers, 0, "nothing else is on a preview layer");

    // Attach to all but the first, so the first is the least recently attached.
    let mut viewers = vec![];
    for id in &ids[1..] {
        viewers.push(attach_view(&mut app, 64.0, 64.0, json!({ "preview": id })));
    }
    let eleventh = open_preview(&mut app, "models/m10.gltf")["preview"].as_u64().unwrap();
    assert!(!ids.contains(&eleventh));
    assert!(call(&mut app, "bevy-ts/asset_preview_control", json!({ "preview": ids[0] })).is_err(), "the oldest was closed");
    assert!(call(&mut app, "bevy-ts/asset_preview_control", json!({ "preview": ids[1] })).is_ok());
    assert_eq!(call(&mut app, "bevy-ts/asset_preview_close", json!({ "preview": eleventh })).unwrap()["closed"], true);
    assert_eq!(call(&mut app, "bevy-ts/asset_preview_close", json!({ "preview": eleventh })).unwrap()["closed"], false);
}

#[test]
fn a_preview_plays_its_gltf_animations() {
    let dir = asset_root("animate");
    write_gltf(&dir.join("models/grunt.gltf"), &["idle", "walk"]);
    let mut app = app_at(&dir);
    let p = open_preview(&mut app, "models/grunt.gltf");
    assert_eq!(p["animations"], json!(["idle", "walk"]));
    let id = p["preview"].as_u64().unwrap();
    until_spawned(&mut app, 1);
    let cube_y = |app: &mut App| {
        app.world_mut()
            .query_filtered::<(&Name, &Transform), With<PreviewOnly>>()
            .iter(app.world())
            .find(|(n, _)| n.as_str() == "cube")
            .map(|(_, t)| t.translation.y)
            .unwrap()
    };
    let y0 = cube_y(&mut app);
    let r = call(&mut app, "bevy-ts/asset_preview_control", json!({ "preview": id, "animation": "walk", "play": true })).unwrap();
    assert_eq!((r["animation"].as_str(), r["playing"].as_bool()), (Some("walk"), Some(true)));
    for _ in 0..20 {
        std::thread::sleep(Duration::from_millis(10));
        app.update();
    }
    assert!(cube_y(&mut app) > y0 + 0.05, "the walk cycle moved the cube");
    assert!(call(&mut app, "bevy-ts/asset_preview_control", json!({ "preview": id, "animation": "run" })).is_err());
    let r = call(&mut app, "bevy-ts/asset_preview_control", json!({ "preview": id, "play": false, "speed": 2 })).unwrap();
    assert_eq!((r["playing"].as_bool(), r["speed"].as_f64()), (Some(false), Some(2.0)));
}

#[test]
fn a_path_that_is_not_a_model_is_an_error_naming_its_kind() {
    let dir = asset_root("kinds");
    std::fs::write(dir.join("models/notes.txt"), "hi").unwrap();
    let mut app = app_at(&dir);
    let e = call(&mut app, "bevy-ts/asset_preview", json!({ "path": "models/notes.txt" })).unwrap_err();
    assert!(e.contains("other"), "{e}");
    assert!(call(&mut app, "bevy-ts/viewport_attach", json!({ "w": 10, "h": 10, "scale": 1, "view": { "preview": 99 } })).is_err());
}
