//! Edit mode in the hole: modes, the editor camera, picking that selects a prefab instance,
//! selection sync, and gizmo drags becoming routed edits. TSX provenance is stood in for by
//! `TsxNode` components shaped as the reconciler writes them.
#![cfg(target_os = "macos")]

use crate::common;

use bevy::picking::events::PointerClick;
use bevy::prelude::*;
use bevy_ts::backend::SourceLocation;
use bevy_ts_tsx::tree::{FieldState, OwnedComponent, Provenance, TsxNode};
use bevy_ts_viewport::edit::{EditorCamera, EditorOnly, Mode};
use common::*;
use serde_json::{Value, json};
use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};
use std::time::Duration;

#[derive(Component)]
struct Bob;

#[derive(Component)]
struct GameCamera;

/// What the scene's entities are, for the tests.
#[derive(Resource, Clone, Copy)]
struct Scene {
    root: Entity,
    eye: Entity,
    free: Entity,
}

fn at(file: &str, line: u32) -> SourceLocation {
    SourceLocation { file: file.into(), line, column: 3 }
}

fn node(id: u64, source: Vec<SourceLocation>, translation: Provenance) -> TsxNode {
    let mut fields = BTreeMap::new();
    for (f, p) in [("translation", translation), ("rotation", Provenance::Default), ("scale", Provenance::Default)] {
        fields.insert(f.to_string(), FieldState { provenance: p, applied: Value::Null });
    }
    let mut components = BTreeMap::new();
    components.insert("Transform".to_string(), OwnedComponent { fields, source: Some(at("enemy.tsx", 3)), overrides: BTreeMap::new() });
    TsxNode { id, root: "game".into(), key_path: vec!["b".into()], types: vec!["Enemy".into()], source, components }
}

/// A game camera, a shadow-casting light (as the `directionalLight` recipe makes), a bobbing enemy
/// prefab instance `b` (a body and an `eye` child) placed at
/// `game.tsx:5`, and a cube no source describes.
fn edit_scene(mut commands: Commands, mut meshes: ResMut<Assets<Mesh>>, mut materials: ResMut<Assets<StandardMaterial>>) {
    let mesh = meshes.add(Cuboid::from_size(Vec3::ONE));
    let small = meshes.add(Cuboid::from_size(Vec3::splat(0.4)));
    let mat = MeshMaterial3d(materials.add(StandardMaterial::default()));
    commands.spawn((GameCamera, Camera3d::default(), Transform::from_xyz(0.0, 0.0, 10.0).looking_at(Vec3::ZERO, Vec3::Y)));
    commands.spawn((DirectionalLight { shadow_maps_enabled: true, ..default() }, Transform::from_xyz(3.0, 6.0, 2.0).looking_at(Vec3::ZERO, Vec3::Y)));
    let root = commands
        .spawn((
            Name::new("b"),
            Bob,
            Mesh3d(mesh.clone()),
            mat.clone(),
            Transform::from_xyz(3.0, 0.0, 0.0),
            node(0xb, vec![at("enemy.tsx", 2), at("game.tsx", 5)], Provenance::Controlled),
        ))
        .id();
    let eye = commands
        .spawn((
            Name::new("eye"),
            Mesh3d(small),
            mat.clone(),
            Transform::from_xyz(0.0, 0.0, 0.7),
            node(0xe, vec![at("enemy.tsx", 4), at("game.tsx", 5)], Provenance::Default),
            ChildOf(root),
        ))
        .id();
    let free = commands.spawn((Name::new("free"), Mesh3d(mesh), mat, Transform::from_xyz(-3.0, 0.0, 0.0))).id();
    commands.insert_resource(Scene { root, eye, free });
}

fn bob(time: Res<Time>, mut q: Query<&mut Transform, With<Bob>>) {
    for mut t in &mut q {
        t.translation.y = (time.elapsed_secs() * 2.0).sin();
    }
}

fn edit_app() -> App {
    let mut app = app_with(|app| {
        app.add_plugins(bevy::picking::mesh_picking::MeshPickingPlugin).add_systems(Startup, edit_scene).add_systems(Update, bob);
    });
    app.update();
    // One game camera: the shared scene's camera and cube are not this test's.
    let extra: Vec<Entity> = app
        .world_mut()
        .query_filtered::<Entity, (Or<(With<Camera>, With<TheCube>)>, Without<GameCamera>, Without<EditorOnly>)>()
        .iter(app.world())
        .collect();
    for e in extra {
        app.world_mut().despawn(e);
    }
    for _ in 0..2 {
        app.update();
    }
    app
}

fn scene(app: &App) -> Scene {
    *app.world().resource::<Scene>()
}

fn updates(app: &mut App, n: usize) {
    for _ in 0..n {
        std::thread::sleep(Duration::from_millis(5));
        app.update();
    }
}

fn input(app: &mut App, events: Value) {
    call(app, "bevy-ts/viewport_input", json!({ "events": events })).unwrap();
}

/// Where a world point is in the hole (points), through the editor camera.
fn screen(app: &mut App, p: Vec3) -> Vec2 {
    let world = app.world_mut();
    let mut q = world.query_filtered::<(&Camera, &GlobalTransform), With<EditorCamera>>();
    let (c, gt) = q.single(world).unwrap();
    c.world_to_viewport(gt, p).unwrap()
}

fn center(app: &App, e: Entity) -> Vec3 {
    app.world().get::<GlobalTransform>(e).unwrap().translation()
}

/// A click: hover first, as a real pointer does, then press and release.
fn click(app: &mut App, at: Vec2, mods: Value) {
    input(app, json!([{ "type": "pointer", "kind": "move", "x": at.x, "y": at.y, "mods": mods }]));
    updates(app, 2);
    input(app, json!([{ "type": "pointer", "kind": "down", "x": at.x, "y": at.y, "button": "left", "clicks": 1, "mods": mods }]));
    updates(app, 1);
    input(app, json!([{ "type": "pointer", "kind": "up", "x": at.x, "y": at.y, "button": "left", "clicks": 1, "mods": mods }]));
    updates(app, 2);
}

fn key(app: &mut App, code: &str, state: &str) {
    input(app, json!([{ "type": "key", "state": state, "code": code }]));
}

fn selection(app: &mut App) -> Value {
    let id = match app.world().resource::<bevy::remote::RemoteMethods>().get("bevy-ts/viewport_selection+watch") {
        Some(bevy::remote::RemoteMethodSystemId::Watching(id)) => *id,
        _ => panic!("no selection watch"),
    };
    // A fresh subscriber each time: the snapshot.
    let params = Some(json!({ "n": rand_key() }));
    let v = app.world_mut().run_system_with(id, params).unwrap().unwrap().unwrap();
    validate("bevy-ts/viewport_selection+watch", &v);
    v
}

fn rand_key() -> u64 {
    use std::sync::atomic::{AtomicU64, Ordering};
    static N: AtomicU64 = AtomicU64::new(0);
    N.fetch_add(1, Ordering::Relaxed)
}

fn edits(app: &mut App) -> Vec<Value> {
    let id = match app.world().resource::<bevy::remote::RemoteMethods>().get("bevy-ts/viewport_edits+watch") {
        Some(bevy::remote::RemoteMethodSystemId::Watching(id)) => *id,
        _ => panic!("no edits watch"),
    };
    let v = app.world_mut().run_system_with(id, Some(json!({ "n": rand_key() }))).unwrap().unwrap().unwrap();
    validate("bevy-ts/viewport_edits+watch", &v);
    v["edits"].as_array().unwrap().clone()
}

#[test]
fn edit_pauses_and_views_from_the_game_cameras_pose_and_play_restores() {
    let mut app = edit_app();
    let s = scene(&app);
    let game_pose = *app.world_mut().query_filtered::<&GlobalTransform, With<GameCamera>>().single(app.world()).unwrap();
    let r = call(&mut app, "bevy-ts/viewport_mode", json!({ "mode": "edit" })).unwrap();
    assert_eq!(r["mode"], "edit");
    updates(&mut app, 1);
    let y0 = app.world().get::<Transform>(s.root).unwrap().translation.y;
    updates(&mut app, 10);
    assert_eq!(app.world().get::<Transform>(s.root).unwrap().translation.y, y0, "the cube stands still");
    let game_active = app.world_mut().query_filtered::<&Camera, With<GameCamera>>().single(app.world()).unwrap().is_active;
    assert!(!game_active);
    let cam = call(&mut app, "bevy-ts/viewport_camera", Value::Null).unwrap();
    let t = Vec3::new(cam["translation"][0].as_f64().unwrap() as f32, cam["translation"][1].as_f64().unwrap() as f32, cam["translation"][2].as_f64().unwrap() as f32);
    assert!(t.distance(game_pose.translation()) < 1e-4, "the editor camera starts at the game camera's pose: {t} vs {}", game_pose.translation());
    assert!(app.world_mut().query_filtered::<(), (With<EditorCamera>, With<EditorOnly>)>().iter(app.world()).count() == 1);

    let r = call(&mut app, "bevy-ts/viewport_mode", json!({ "mode": "play" })).unwrap();
    assert_eq!(r["mode"], "play");
    updates(&mut app, 10);
    assert_ne!(app.world().get::<Transform>(s.root).unwrap().translation.y, y0, "it bobs again");
    let game_active = app.world_mut().query_filtered::<&Camera, With<GameCamera>>().single(app.world()).unwrap().is_active;
    assert!(game_active);
    let editor_active = app.world_mut().query_filtered::<&Camera, With<EditorCamera>>().single(app.world()).unwrap().is_active;
    assert!(!editor_active);
}

#[test]
fn a_click_selects_the_prefab_instance_alt_the_exact_entity() {
    let mut app = edit_app();
    let s = scene(&app);
    call(&mut app, "bevy-ts/viewport_mode", json!({ "mode": "edit" })).unwrap();
    updates(&mut app, 2);
    let p = center(&app, s.eye) + Vec3::Z * 0.2;
    let eye = screen(&mut app, p);
    let pick = call(&mut app, "bevy-ts/viewport_pick", json!({ "x": eye.x, "y": eye.y })).unwrap();
    assert_eq!(pick["hits"][0]["entity"], s.eye.to_bits());
    assert_eq!(pick["hits"][0]["selects"], s.root.to_bits());

    click(&mut app, eye, json!({}));
    let sel = selection(&mut app);
    assert_eq!(sel["origin"], "viewport");
    assert_eq!(sel["items"][0]["entity"], s.root.to_bits());
    assert_eq!(sel["items"][0]["primary"], true);
    assert_eq!(sel["items"][0]["node"], "000000000000000b");
    assert_eq!(sel["items"][0]["source"][1]["file"], "game.tsx");
    assert_eq!(sel["items"][0]["routes"]["translation"]["kind"], "tsx");

    // With b selected its gizmo sits over the eye and a press there grabs the gizmo: clear first.
    click(&mut app, Vec2::new(5.0, 5.0), json!({}));
    click(&mut app, eye, json!({ "alt": true }));
    assert_eq!(selection(&mut app)["items"][0]["entity"], s.eye.to_bits());

    let p = center(&app, s.free);
    let free = screen(&mut app, p);
    click(&mut app, free, json!({ "shift": true }));
    let sel = selection(&mut app);
    assert_eq!(sel["items"].as_array().unwrap().len(), 2);
    assert_eq!(sel["items"][1]["entity"], s.free.to_bits());
    assert_eq!(sel["items"][1]["routes"], Value::Null, "routes ride on the primary only");

    click(&mut app, Vec2::new(5.0, 5.0), json!({}));
    assert_eq!(selection(&mut app)["items"].as_array().unwrap().len(), 0, "a click on nothing clears");
}

#[test]
fn a_pick_answers_world_points_and_the_ground() {
    let mut app = edit_app();
    let s = scene(&app);
    call(&mut app, "bevy-ts/viewport_mode", json!({ "mode": "edit" })).unwrap();
    updates(&mut app, 2);
    // Level, a metre above the floor: the upper half of the hole is sky.
    {
        let world = app.world_mut();
        let mut q = world.query_filtered::<&mut Transform, With<EditorCamera>>();
        *q.single_mut(world).unwrap() = Transform::from_xyz(0.0, 1.0, 10.0).looking_at(Vec3::new(0.0, 1.0, 0.0), Vec3::Y);
    }
    updates(&mut app, 2);

    let c = center(&app, s.free);
    let at = screen(&mut app, c);
    let pick = call(&mut app, "bevy-ts/viewport_pick", json!({ "x": at.x, "y": at.y })).unwrap();
    assert_eq!(pick["hits"][0]["entity"], s.free.to_bits());
    let p: Vec<f32> = serde_json::from_value(pick["hits"][0]["point"].clone()).unwrap();
    let local = (Vec3::from_slice(&p) - c).abs();
    assert!((local.max_element() - 0.5).abs() < 1e-3, "the point is on the cube's surface: {local}");

    let at = screen(&mut app, Vec3::new(0.0, 0.0, 4.0));
    let pick = call(&mut app, "bevy-ts/viewport_pick", json!({ "x": at.x, "y": at.y })).unwrap();
    assert_eq!(pick["hits"].as_array().unwrap().len(), 0, "{pick}");
    let g: Vec<f32> = serde_json::from_value(pick["ground"].clone()).unwrap();
    assert!(g[1].abs() < 1e-4 && (g[0]).abs() < 1e-3 && (g[2] - 4.0).abs() < 1e-3, "{g:?}");

    let pick = call(&mut app, "bevy-ts/viewport_pick", json!({ "x": at.x, "y": 2.0 })).unwrap();
    assert_eq!(pick["hits"].as_array().unwrap().len(), 0, "{pick}");
    assert_eq!(pick["ground"], Value::Null, "the sky has no ground");
}

#[derive(Resource, Default, Clone)]
struct Clicks(Arc<Mutex<u32>>);

#[test]
fn game_pointer_observers_stay_silent_in_edit_mode() {
    let mut app = edit_app();
    let s = scene(&app);
    let clicks = Clicks::default();
    app.insert_resource(clicks.clone());
    app.world_mut().entity_mut(s.free).observe(|_: On<PointerClick>, c: Res<Clicks>| *c.0.lock().unwrap() += 1);
    call(&mut app, "bevy-ts/viewport_mode", json!({ "mode": "edit" })).unwrap();
    updates(&mut app, 2);
    let p = center(&app, s.free);
    let free = screen(&mut app, p);
    click(&mut app, free, json!({}));
    assert_eq!(*clicks.0.lock().unwrap(), 0, "no game click in edit mode");
    assert_eq!(selection(&mut app)["items"][0]["entity"], s.free.to_bits(), "the editor selected it instead");
    call(&mut app, "bevy-ts/viewport_mode", json!({ "mode": "play" })).unwrap();
    updates(&mut app, 2);
    // The game camera looks from the same pose, so the same point hits the cube.
    click(&mut app, free, json!({}));
    assert_eq!(*clicks.0.lock().unwrap(), 1, "the game's click fires in play");
}

#[test]
fn selecting_from_the_editor_then_framing_it() {
    let mut app = edit_app();
    let s = scene(&app);
    call(&mut app, "bevy-ts/viewport_mode", json!({ "mode": "edit" })).unwrap();
    updates(&mut app, 2);
    let r = call(&mut app, "bevy-ts/viewport_select", json!({ "entities": [s.root.to_bits()] })).unwrap();
    assert_eq!(r["origin"], "editor");
    assert_eq!(r["items"][0]["entity"], s.root.to_bits());
    let before = *app.world_mut().query_filtered::<&Transform, With<EditorCamera>>().single(app.world()).unwrap();
    key(&mut app, "KeyF", "down");
    updates(&mut app, 2);
    key(&mut app, "KeyF", "up");
    updates(&mut app, 1);
    let after = *app.world_mut().query_filtered::<&Transform, With<EditorCamera>>().single(app.world()).unwrap();
    assert_ne!(before.translation, after.translation);
    assert!(before.rotation.angle_between(after.rotation) < 1e-4, "framing keeps the direction");
    let to = (center(&app, s.root) - after.translation).normalize();
    assert!(to.dot(after.forward().as_vec3()) > 0.999, "b is in the middle of the view");
    // Escape clears.
    key(&mut app, "Escape", "down");
    updates(&mut app, 1);
    key(&mut app, "Escape", "up");
    updates(&mut app, 1);
    assert_eq!(selection(&mut app)["items"].as_array().unwrap().len(), 0);
}

/// Drags the translate X handle of the primary selection by `dx` points; `escape` cancels mid-way.
fn drag_x(app: &mut App, entity: Entity, dx: f32, escape: bool) {
    let pos = center(app, entity);
    let cam = app.world_mut().query_filtered::<&GlobalTransform, With<EditorCamera>>().single(app.world()).unwrap().translation();
    let scale = (cam - pos).length() * 0.1;
    let handle = screen(app, pos + Vec3::X * 0.6 * scale);
    input(app, json!([{ "type": "pointer", "kind": "move", "x": handle.x, "y": handle.y }]));
    updates(app, 2);
    input(app, json!([{ "type": "pointer", "kind": "down", "x": handle.x, "y": handle.y, "button": "left" }]));
    updates(app, 1);
    for i in 1..=5 {
        let x = handle.x + dx * i as f32 / 5.0;
        input(app, json!([{ "type": "pointer", "kind": "move", "x": x, "y": handle.y }]));
        updates(app, 1);
    }
    if escape {
        key(app, "Escape", "down");
        updates(app, 1);
    }
    input(app, json!([{ "type": "pointer", "kind": "up", "x": handle.x + dx, "y": handle.y, "button": "left" }]));
    updates(app, 2);
    if escape {
        key(app, "Escape", "up");
        updates(app, 1);
    }
}

#[test]
fn a_drag_becomes_one_tsx_edit_and_a_rejection_restores_and_locks() {
    let mut app = edit_app();
    let s = scene(&app);
    call(&mut app, "bevy-ts/viewport_mode", json!({ "mode": "edit" })).unwrap();
    updates(&mut app, 2);
    call(&mut app, "bevy-ts/viewport_select", json!({ "entities": [s.root.to_bits()] })).unwrap();
    updates(&mut app, 2);
    let x0 = app.world().get::<Transform>(s.root).unwrap().translation.x;
    drag_x(&mut app, s.root, 80.0, false);
    let x1 = app.world().get::<Transform>(s.root).unwrap().translation.x;
    assert!(x1 > x0 + 0.2, "the cube followed the pointer: {x0} -> {x1}");
    let e = edits(&mut app);
    assert_eq!(e.len(), 1, "one edit per drag: {e:?}");
    assert_eq!(e[0]["field"], "translation");
    assert_eq!(e[0]["before"]["translation"][0], 3.0);
    assert!(e[0]["after"]["translation"][0].as_f64().unwrap() > 3.2);
    assert_eq!(e[0]["route"]["kind"], "tsx");
    assert_eq!(e[0]["route"]["source"][1]["file"], "game.tsx");
    assert_eq!(e[0]["status"], "pending");

    let r = call(&mut app, "bevy-ts/viewport_edit_result", json!({ "id": e[0]["id"], "status": "rejected", "reason": "computed" })).unwrap();
    assert_eq!(r["status"], "rejected");
    updates(&mut app, 1);
    assert_eq!(app.world().get::<Transform>(s.root).unwrap().translation.x, x0, "restored within a frame");
    let sel = selection(&mut app);
    assert_eq!(sel["items"][0]["routes"]["translation"]["kind"], "locked");
    assert_eq!(sel["items"][0]["routes"]["translation"]["reason"], "computed");
    // A locked field does not drag.
    drag_x(&mut app, s.root, 80.0, false);
    assert_eq!(app.world().get::<Transform>(s.root).unwrap().translation.x, x0);
    assert_eq!(edits(&mut app).len(), 1);
}

#[test]
fn escape_mid_drag_restores_and_emits_nothing() {
    let mut app = edit_app();
    let s = scene(&app);
    call(&mut app, "bevy-ts/viewport_mode", json!({ "mode": "edit" })).unwrap();
    updates(&mut app, 2);
    call(&mut app, "bevy-ts/viewport_select", json!({ "entities": [s.free.to_bits()] })).unwrap();
    updates(&mut app, 2);
    let x0 = app.world().get::<Transform>(s.free).unwrap().translation.x;
    drag_x(&mut app, s.free, 80.0, true);
    assert_eq!(app.world().get::<Transform>(s.free).unwrap().translation.x, x0);
    assert!(edits(&mut app).is_empty());
    drag_x(&mut app, s.free, 80.0, false);
    let e = edits(&mut app);
    assert_eq!(e.len(), 1);
    assert_eq!(e[0]["route"]["kind"], "transient");
    assert_eq!(e[0]["status"], "transient");
}

#[test]
fn an_unanswered_edit_becomes_unconfirmed_and_revert_moves_it_back() {
    let mut app = edit_app();
    let s = scene(&app);
    call(&mut app, "bevy-ts/viewport_mode", json!({ "mode": "edit" })).unwrap();
    updates(&mut app, 1);
    app.world_mut().resource_mut::<bevy_ts_viewport::edits::EditLog>().timeout = Some(Duration::from_millis(50));
    let before = *app.world().get::<Transform>(s.root).unwrap();
    let mut after = before;
    after.translation.x = 5.0;
    *app.world_mut().get_mut::<Transform>(s.root).unwrap() = after;
    let id = bevy_ts_viewport::edits::record(app.world_mut(), s.root, "translation", before, after);
    std::thread::sleep(Duration::from_millis(80));
    updates(&mut app, 1);
    let e = edits(&mut app);
    assert_eq!(e[0]["status"], "unconfirmed");
    assert_eq!(app.world().get::<Transform>(s.root).unwrap().translation.x, 5.0, "it keeps its new position");
    let r = call(&mut app, "bevy-ts/viewport_revert", json!({ "id": id })).unwrap();
    assert_eq!(r["status"], "reverted");
    assert_eq!(app.world().get::<Transform>(s.root).unwrap().translation.x, before.translation.x);
}

/// The free cube's middle pixel and a corner of clear colour in the game view, as `[b, g, r, a]`,
/// after `n` more frames.
fn free_pixel(app: &mut App, viewer: &bevy_ts_viewport::mach::Viewer, seen: &mut Seen, n: usize) -> ([u8; 4], [u8; 4]) {
    let n = seen.frames.len() + n;
    run_until(app, viewer, seen, LONG, |s| s.frames.len() >= n);
    let p = center(app, scene(app).free);
    let world = app.world_mut();
    let mut q = world.query_filtered::<(&Camera, &GlobalTransform), With<GameCamera>>();
    let (c, gt) = q.single(world).unwrap();
    let at = c.world_to_viewport(gt, p).unwrap();
    let f = seen.frames.last().unwrap();
    let px = seen.surfaces[&(f["ring"].as_u64().unwrap(), f["slot"].as_u64().unwrap())].read(&[(at.x as u32, at.y as u32), (2, 2)]);
    (px[0], px[1])
}

/// A mode switch to make in `Last`, after `PostUpdate`, where an HTTP request lands.
#[derive(Resource, Default)]
struct SwitchTo(Option<Mode>);

fn switch_late(world: &mut World) {
    if let Some(mode) = world.resource_mut::<SwitchTo>().0.take() {
        bevy_ts_viewport::edit::set_mode(world, mode, None, None, None).unwrap();
    }
}

fn switch(app: &mut App, mode: Mode) {
    app.world_mut().resource_mut::<SwitchTo>().0 = Some(mode);
}

#[test]
fn meshes_stay_lit_after_edit_and_back_to_play() {
    let mut app = edit_app();
    app.init_resource::<SwitchTo>().add_systems(Last, switch_late);
    let (_, viewer) = attach(&mut app, 160.0, 120.0, 1.0);
    let mut seen = Seen::default();
    // The first lit frame comes once the mesh's pipelines compile, on other threads that a loaded
    // machine slows by any amount: frames until it is lit, or `LONG`.
    let brighter = |l: [u8; 4], c: [u8; 4]| l[..3].iter().zip(&c[..3]).all(|(l, c)| l > c);
    let t0 = std::time::Instant::now();
    let (lit, clear) = loop {
        let (lit, clear) = free_pixel(&mut app, &viewer, &mut seen, 1);
        if brighter(lit, clear) || t0.elapsed() > LONG {
            break (lit, clear);
        }
    };
    // Settled: the frames after it draw the same.
    let (lit, clear) = if brighter(lit, clear) { free_pixel(&mut app, &viewer, &mut seen, 6) } else { (lit, clear) };
    println!("play {lit:?} clear {clear:?}");
    assert!(brighter(lit, clear), "the cube is lit, brighter than the clear colour, in play within {LONG:?}: {lit:?} vs {clear:?}");
    switch(&mut app, Mode::Edit);
    free_pixel(&mut app, &viewer, &mut seen, 6);
    // The walk's edit mode resized the hole and changed its scale.
    call(&mut app, "bevy-ts/viewport_resize", json!({ "w": 200, "h": 100, "scale": 2 })).unwrap();
    free_pixel(&mut app, &viewer, &mut seen, 6);
    call(&mut app, "bevy-ts/viewport_resize", json!({ "w": 160, "h": 120, "scale": 1 })).unwrap();
    free_pixel(&mut app, &viewer, &mut seen, 6);
    switch(&mut app, Mode::Play);
    let (again, _) = free_pixel(&mut app, &viewer, &mut seen, 6);
    println!("play again {again:?}");
    assert_eq!(lit, again, "the cube is lit as before after edit then play");
}

// ---- tools ----

fn tools_watch(app: &mut App, key: u64) -> Option<Value> {
    let id = match app.world().resource::<bevy::remote::RemoteMethods>().get("bevy-ts/viewport_tools+watch") {
        Some(bevy::remote::RemoteMethodSystemId::Watching(id)) => *id,
        _ => panic!("no tools watch"),
    };
    let v = app.world_mut().run_system_with(id, Some(json!({ "k": key }))).unwrap().unwrap();
    if let Some(v) = &v {
        validate("bevy-ts/viewport_tools+watch", v);
    }
    v
}

fn gizmo(app: &App) -> &bevy::gizmos::transform_gizmo::TransformGizmoSettings {
    app.world().resource::<bevy::gizmos::transform_gizmo::TransformGizmoSettings>()
}

#[test]
fn the_toolbar_sets_the_gizmo_and_the_stream_follows_keys() {
    use bevy::gizmos::transform_gizmo::{TransformGizmoMode, TransformGizmoSpace};
    let mut app = edit_app();
    let s = scene(&app);
    let r = call(&mut app, "bevy-ts/viewport_tools", Value::Null).unwrap();
    assert_eq!(r, json!({ "mode": "translate", "space": "world", "snap": false, "steps": { "translate": 0.5, "rotate": 15.0, "scale": 0.1 }, "origin": "engine" }));
    call(&mut app, "bevy-ts/viewport_mode", json!({ "mode": "edit" })).unwrap();
    updates(&mut app, 2);
    call(&mut app, "bevy-ts/viewport_select", json!({ "entities": [s.root.to_bits()] })).unwrap();
    let key = rand_key();
    assert_eq!(tools_watch(&mut app, key).unwrap()["origin"], "engine", "sent on open");
    updates(&mut app, 1);
    assert_eq!(tools_watch(&mut app, key), None, "nothing new");

    let r = call(&mut app, "bevy-ts/viewport_tools", json!({ "mode": "rotate" })).unwrap();
    assert_eq!(r["mode"], "rotate");
    assert_eq!(r["origin"], "editor");
    updates(&mut app, 1);
    assert_eq!(gizmo(&app).mode, TransformGizmoMode::Rotate, "rotation rings");
    let m = tools_watch(&mut app, key).unwrap();
    assert_eq!((m["mode"].as_str(), m["origin"].as_str()), (Some("rotate"), Some("editor")));
    assert_eq!(call(&mut app, "bevy-ts/viewport_tools", Value::Null).unwrap()["mode"], "rotate", "no params: unchanged");

    key_press(&mut app, "KeyX");
    let m = tools_watch(&mut app, key).unwrap();
    assert_eq!((m["space"].as_str(), m["origin"].as_str()), (Some("local"), Some("viewport")));
    assert_eq!(gizmo(&app).space, TransformGizmoSpace::Local);
    key_press(&mut app, "KeyW");
    assert_eq!(tools_watch(&mut app, key).unwrap()["mode"], "translate");

    call(&mut app, "bevy-ts/viewport_tools", json!({ "snap": true })).unwrap();
    updates(&mut app, 1);
    let g = gizmo(&app);
    assert_eq!((g.snap_translate, g.snap_scale), (Some(0.5), Some(0.1)));
    assert!((g.snap_rotate.unwrap() - 15f32.to_radians()).abs() < 1e-6);

    // Kept across a mode switch.
    call(&mut app, "bevy-ts/viewport_mode", json!({ "mode": "play" })).unwrap();
    updates(&mut app, 2);
    call(&mut app, "bevy-ts/viewport_mode", json!({ "mode": "edit" })).unwrap();
    updates(&mut app, 2);
    let r = call(&mut app, "bevy-ts/viewport_tools", Value::Null).unwrap();
    assert_eq!((r["mode"].as_str(), r["space"].as_str(), r["snap"].as_bool()), (Some("translate"), Some("local"), Some(true)));
    assert_eq!(gizmo(&app).space, TransformGizmoSpace::Local);
}

fn key_press(app: &mut App, code: &str) {
    key(app, code, "down");
    updates(app, 1);
    key(app, code, "up");
    updates(app, 1);
}

#[test]
fn a_snapped_move_lands_on_a_whole_step() {
    let mut app = edit_app();
    let s = scene(&app);
    call(&mut app, "bevy-ts/viewport_mode", json!({ "mode": "edit" })).unwrap();
    updates(&mut app, 2);
    call(&mut app, "bevy-ts/viewport_tools", json!({ "snap": true })).unwrap();
    call(&mut app, "bevy-ts/viewport_select", json!({ "entities": [s.root.to_bits()] })).unwrap();
    updates(&mut app, 2);
    let pos = center(&app, s.root);
    assert_eq!(pos.x, 3.0);
    let cam = app.world_mut().query_filtered::<&GlobalTransform, With<EditorCamera>>().single(app.world()).unwrap().translation();
    let grab = pos + Vec3::X * 0.6 * (cam - pos).length() * 0.1;
    let (from, to) = (screen(&mut app, grab), screen(&mut app, grab + Vec3::X * 1.4));
    input(&mut app, json!([{ "type": "pointer", "kind": "move", "x": from.x, "y": from.y }]));
    updates(&mut app, 2);
    input(&mut app, json!([{ "type": "pointer", "kind": "down", "x": from.x, "y": from.y, "button": "left" }]));
    updates(&mut app, 1);
    for i in 1..=5 {
        let p = from.lerp(to, i as f32 / 5.0);
        input(&mut app, json!([{ "type": "pointer", "kind": "move", "x": p.x, "y": p.y }]));
        updates(&mut app, 1);
    }
    assert_eq!(app.world().get::<Transform>(s.root).unwrap().translation.x, 4.5, "during the drag");
    input(&mut app, json!([{ "type": "pointer", "kind": "up", "x": to.x, "y": to.y, "button": "left" }]));
    updates(&mut app, 2);
    let e = edits(&mut app);
    assert_eq!(e.len(), 1);
    assert_eq!(e[0]["after"]["translation"][0], 4.5, "in the edit");
}

#[test]
fn a_tool_change_mid_drag_waits_for_the_release() {
    use bevy::gizmos::transform_gizmo::TransformGizmoMode;
    let mut app = edit_app();
    let s = scene(&app);
    call(&mut app, "bevy-ts/viewport_mode", json!({ "mode": "edit" })).unwrap();
    updates(&mut app, 2);
    call(&mut app, "bevy-ts/viewport_select", json!({ "entities": [s.free.to_bits()] })).unwrap();
    updates(&mut app, 2);
    let pos = center(&app, s.free);
    let cam = app.world_mut().query_filtered::<&GlobalTransform, With<EditorCamera>>().single(app.world()).unwrap().translation();
    let from = screen(&mut app, pos + Vec3::X * 0.6 * (cam - pos).length() * 0.1);
    input(&mut app, json!([{ "type": "pointer", "kind": "move", "x": from.x, "y": from.y }]));
    updates(&mut app, 2);
    input(&mut app, json!([{ "type": "pointer", "kind": "down", "x": from.x, "y": from.y, "button": "left" }]));
    updates(&mut app, 1);
    for i in 1..=5 {
        if i == 2 {
            assert_eq!(call(&mut app, "bevy-ts/viewport_tools", json!({ "mode": "rotate" })).unwrap()["mode"], "rotate");
        }
        input(&mut app, json!([{ "type": "pointer", "kind": "move", "x": from.x + 16.0 * i as f32, "y": from.y }]));
        updates(&mut app, 1);
        assert_eq!(gizmo(&app).mode, TransformGizmoMode::Translate, "the drag keeps its mode");
    }
    input(&mut app, json!([{ "type": "pointer", "kind": "up", "x": from.x + 80.0, "y": from.y, "button": "left" }]));
    updates(&mut app, 2);
    assert!(center(&app, s.free).x > pos.x + 0.2, "it moved all the way");
    let e = edits(&mut app);
    assert_eq!((e.len(), e[0]["field"].as_str()), (1, Some("translation")));
    assert_eq!(gizmo(&app).mode, TransformGizmoMode::Rotate, "the new mode after the release");
}

#[test]
fn tool_keys_with_a_modifier_are_shortcuts() {
    let mut app = edit_app();
    call(&mut app, "bevy-ts/viewport_mode", json!({ "mode": "edit" })).unwrap();
    updates(&mut app, 2);
    input(&mut app, json!([{ "type": "key", "state": "down", "code": "KeyE", "mods": { "meta": true } }]));
    updates(&mut app, 1);
    input(&mut app, json!([{ "type": "key", "state": "up", "code": "KeyE", "mods": { "meta": true } }]));
    updates(&mut app, 1);
    assert_eq!(call(&mut app, "bevy-ts/viewport_tools", Value::Null).unwrap()["mode"], "translate");
}

// ---- orientation gizmo ----

use bevy_ts_viewport::orient::{OrientCamera, end_position};

fn orient_rect() -> Rect {
    Rect::new(8.0, 8.0, 104.0, 104.0)
}

fn editor_pose(app: &mut App) -> (Transform, EditorCamera) {
    let world = app.world_mut();
    let (t, c) = world.query::<(&Transform, &EditorCamera)>().single(world).unwrap();
    (*t, *c)
}

fn orient_camera(app: &mut App) -> Camera {
    let world = app.world_mut();
    world.query_filtered::<&Camera, With<OrientCamera>>().single(world).unwrap().clone()
}

fn orient_size(app: &mut App) -> UVec2 {
    let h = app.world().resource::<bevy_ts_viewport::orient::OrientImage>().0.clone();
    app.world().resource::<Assets<Image>>().get(&h).unwrap().size()
}

fn orient_quad_shown(app: &mut App) -> bool {
    let world = app.world_mut();
    let v = world.query_filtered::<&Visibility, With<bevy_ts_viewport::orient::OrientQuad>>().single(world).unwrap();
    *v != Visibility::Hidden
}

#[test]
fn clicking_plus_y_looks_down_and_keeps_the_selection() {
    let mut app = edit_app();
    let s = scene(&app);
    assert!(!orient_camera(&mut app).is_active, "none in play");
    call(&mut app, "bevy-ts/viewport_mode", json!({ "mode": "edit" })).unwrap();
    call(&mut app, "bevy-ts/viewport_select", json!({ "entities": [s.root.to_bits()] })).unwrap();
    // Drawn, then two frames to settle.
    updates(&mut app, 4);
    assert_eq!(orient_size(&mut app), UVec2::splat(288), "one texture, at 3x");
    assert!(orient_quad_shown(&mut app));
    let layers = app.world_mut().query_filtered::<&bevy::camera::visibility::RenderLayers, With<OrientCamera>>().single(app.world()).unwrap().clone();
    assert_eq!(layers, bevy::camera::visibility::RenderLayers::layer(bevy_ts_viewport::orient::ORIENT_LAYER));
    // On through its warm-up, then off while the view is still.
    let t0 = std::time::Instant::now();
    while orient_camera(&mut app).is_active {
        assert!(t0.elapsed() < LONG, "a still view does not redraw it");
        updates(&mut app, 1);
    }
    updates(&mut app, 3);
    assert!(!orient_camera(&mut app).is_active, "a still view does not redraw it");

    let (before, c0) = editor_pose(&mut app);
    let distance = before.translation.distance(c0.focus);
    let up = end_position(before.rotation, orient_rect(), Vec3::Y);
    click(&mut app, up, json!({}));
    let (after, c1) = editor_pose(&mut app);
    let limit = bevy_ts_viewport::edit::PITCH_LIMIT;
    assert!((after.forward().as_vec3().dot(Vec3::NEG_Y) - limit.sin()).abs() < 1e-4, "down, to the orbit's pitch limit: {}", after.forward().as_vec3());
    assert!(c1.focus.distance(c0.focus) < 1e-4, "same focus");
    assert!((after.translation.distance(c1.focus) - distance).abs() < 1e-3, "same distance");
    let sel = selection(&mut app);
    assert_eq!(sel["items"][0]["entity"], s.root.to_bits(), "the selection is unchanged");
    assert!(edits(&mut app).is_empty(), "no gizmo drag");

    // Kept on resize, at the new scale.
    call(&mut app, "bevy-ts/viewport_resize", json!({ "w": 800, "h": 600, "scale": 1 })).unwrap();
    updates(&mut app, 2);
    assert_eq!(orient_size(&mut app), UVec2::splat(288), "the same texture at any scale");

    // Redrawn on the two frames after the view turns (gizmo lines land a frame late), and not after.
    call(&mut app, "bevy-ts/viewport_camera", json!({ "rotation": Quat::from_rotation_y(0.3).to_array() })).unwrap();
    for _ in 0..2 {
        updates(&mut app, 1);
        assert!(orient_camera(&mut app).is_active, "the view turned");
    }
    updates(&mut app, 1);
    assert!(!orient_camera(&mut app).is_active, "and is still again");

    call(&mut app, "bevy-ts/viewport_mode", json!({ "mode": "play" })).unwrap();
    updates(&mut app, 2);
    assert!(!orient_camera(&mut app).is_active, "none in play");
    assert!(!orient_quad_shown(&mut app), "none in play");
    let ends_shown = app.world_mut().query_filtered::<&Visibility, With<bevy_ts_viewport::orient::OrientEnd>>().iter(app.world()).filter(|v| **v != Visibility::Hidden).count();
    assert_eq!(ends_shown, 0, "no end shows in play");
    let previews = app.world_mut().query_filtered::<(), (With<OrientCamera>, With<bevy_ts_viewport::preview::PreviewOnly>)>().iter(app.world()).count();
    assert_eq!(previews, 0, "none in a preview");
}

#[test]
fn a_press_in_the_square_never_picks_and_in_play_reaches_the_game() {
    let mut app = edit_app();
    // A cube under the square's corner, where no axis end is.
    let at = Vec2::new(14.0, 14.0);
    let ray = {
        let world = app.world_mut();
        let (c, gt) = world.query_filtered::<(&Camera, &GlobalTransform), With<GameCamera>>().single(world).unwrap();
        c.viewport_to_world(gt, at).unwrap()
    };
    let mesh = app.world_mut().resource_mut::<Assets<Mesh>>().add(Cuboid::from_size(Vec3::ONE));
    let cube = app.world_mut().spawn((Mesh3d(mesh), Transform::from_translation(ray.get_point(6.0)))).id();
    let clicks = Clicks::default();
    app.insert_resource(clicks.clone());
    app.world_mut().entity_mut(cube).observe(|_: On<PointerClick>, c: Res<Clicks>| *c.0.lock().unwrap() += 1);
    call(&mut app, "bevy-ts/viewport_mode", json!({ "mode": "edit" })).unwrap();
    updates(&mut app, 2);
    let pick = call(&mut app, "bevy-ts/viewport_pick", json!({ "x": at.x, "y": at.y })).unwrap();
    assert_eq!(pick["hits"][0]["entity"], cube.to_bits(), "the cube is under the square");
    let (before, _) = editor_pose(&mut app);
    click(&mut app, at, json!({}));
    assert_eq!(selection(&mut app)["items"].as_array().unwrap().len(), 0, "no pick in the square");
    assert_eq!(editor_pose(&mut app).0, before, "no end there: the camera stays");

    call(&mut app, "bevy-ts/viewport_mode", json!({ "mode": "play" })).unwrap();
    updates(&mut app, 2);
    click(&mut app, at, json!({}));
    assert_eq!(*clicks.0.lock().unwrap(), 1, "the game's click fires in play");
}

/// The frames show the gizmo: the `+Y` end's pixel is green. Writes the corner to
/// `target/orient-gizmo.bmp` to look at.
#[test]
fn the_frames_show_the_orientation_gizmo() {
    let mut app = edit_app();
    let (_, viewer) = attach(&mut app, 480.0, 320.0, 2.0);
    call(&mut app, "bevy-ts/viewport_mode", json!({ "mode": "edit" })).unwrap();
    let view = Quat::from_euler(EulerRot::YXZ, -0.6, -0.4, 0.0);
    call(&mut app, "bevy-ts/viewport_camera", json!({ "rotation": view.to_array() })).unwrap();
    let mut seen = Seen::default();
    let green = |[b, g, r, _]: [u8; 4]| g > 150 && r < 160 && b < 120;
    let up = end_position(view, orient_rect(), Vec3::Y) * 2.0;
    let px = pixel_shown(&mut app, &viewer, &mut seen, up, green);
    assert!(green(px), "+Y end at {up} is green: bgra {px:?}");
    let f = seen.frames.last().unwrap().clone();
    let side = 224u32;
    let points: Vec<(u32, u32)> = (0..side).flat_map(|y| (0..side).map(move |x| (x, y))).collect();
    let corner = seen.surfaces[&(f["ring"].as_u64().unwrap(), f["slot"].as_u64().unwrap())].read(&points);
    write_bmp(concat!(env!("CARGO_MANIFEST_DIR"), "/../../target/orient-gizmo.bmp"), side, &corner);

    // After a turn, the lines too: the middle of the +X line is red. Once the warm-up is over, so
    // a turn redraws alone.
    let t0 = std::time::Instant::now();
    while orient_camera(&mut app).is_active {
        assert!(t0.elapsed() < LONG, "the warm-up never ends");
        updates(&mut app, 1);
    }
    let view = Quat::from_euler(EulerRot::YXZ, 0.5, -0.3, 0.0);
    call(&mut app, "bevy-ts/viewport_camera", json!({ "rotation": view.to_array() })).unwrap();
    let red = |[b, g, r, _]: [u8; 4]| r > 150 && g < 120 && b < 140;
    let mid = (orient_rect().center() + end_position(view, orient_rect(), Vec3::X)) / 2.0 * 2.0;
    let px = pixel_shown(&mut app, &viewer, &mut seen, mid, red);
    assert!(red(px), "after a turn, the +X line at {mid} is red: bgra {px:?}");

    // Redrawn at a new scale.
    call(&mut app, "bevy-ts/viewport_resize", json!({ "w": 480, "h": 320, "scale": 1 })).unwrap();
    let up = end_position(view, orient_rect(), Vec3::Y);
    let px = pixel_shown(&mut app, &viewer, &mut seen, up, green);
    assert!(green(px), "after the resize, +Y end at {up} is green: bgra {px:?}");
}

/// A pixel of the newest frame, once `ok` holds or `LONG` on: how fast the first frames come
/// depends on the machine's load, and a gizmo that is never drawn still fails.
fn pixel_shown(app: &mut App, viewer: &bevy_ts_viewport::mach::Viewer, seen: &mut Seen, at: Vec2, ok: impl Fn([u8; 4]) -> bool) -> [u8; 4] {
    let start = seen.frames.len();
    let t0 = std::time::Instant::now();
    let mut px = [0; 4];
    for n in start + 1.. {
        if t0.elapsed() > LONG {
            break;
        }
        run_until(app, viewer, seen, LONG, |s| s.frames.len() >= n);
        let f = seen.frames.last().unwrap();
        px = seen.surfaces[&(f["ring"].as_u64().unwrap(), f["slot"].as_u64().unwrap())].read(&[(at.x as u32, at.y as u32)])[0];
        if ok(px) {
            break;
        }
    }
    px
}

fn write_bmp(path: &str, side: u32, bgra: &[[u8; 4]]) {
    let row = side * 3;
    let size = 54 + row * side;
    let mut out = vec![];
    out.extend(b"BM");
    for v in [size, 0, 54, 40, side, side.wrapping_neg()] {
        out.extend(v.to_le_bytes());
    }
    out.extend(1u16.to_le_bytes());
    out.extend(24u16.to_le_bytes());
    for v in [0u32, row * side, 2835, 2835, 0, 0] {
        out.extend(v.to_le_bytes());
    }
    for p in bgra {
        out.extend(&p[..3]);
    }
    std::fs::write(path, out).unwrap();
}

// ---- the game view (change `game-camera-edit`) ----

/// `viewport_mode` with only `camera`.
fn view(app: &mut App, camera: &str) -> Value {
    call(app, "bevy-ts/viewport_mode", json!({ "camera": camera })).unwrap()
}

/// A call's error: its code and message.
fn refused_call(app: &mut App, method: &str, params: Value) -> (i16, String) {
    let id = match app.world().resource::<bevy::remote::RemoteMethods>().get(method) {
        Some(bevy::remote::RemoteMethodSystemId::Instant(id)) => *id,
        _ => panic!("no method {method}"),
    };
    let e = app.world_mut().run_system_with(id, Some(params)).unwrap().expect_err("refused");
    (e.code, e.message)
}

fn is_on<F: bevy::ecs::query::QueryFilter>(app: &mut App) -> Vec<bool> {
    let world = app.world_mut();
    world.query_filtered::<&Camera, F>().iter(world).map(|c| c.is_active).collect()
}

/// A row of pixels across the free cube in the newest frame, after `n` more frames.
fn free_row(app: &mut App, viewer: &bevy_ts_viewport::mach::Viewer, seen: &mut Seen, n: usize) -> Vec<[u8; 4]> {
    free_pixel(app, viewer, seen, n);
    let p = center(app, scene(app).free);
    let world = app.world_mut();
    let mut q = world.query_filtered::<(&Camera, &GlobalTransform), With<GameCamera>>();
    let (c, gt) = q.single(world).unwrap();
    let at = c.world_to_viewport(gt, p).unwrap();
    let f = seen.frames.last().unwrap();
    let points: Vec<(u32, u32)> = (0..40).map(|i| ((at.x as u32).saturating_sub(20) + i, at.y as u32)).collect();
    seen.surfaces[&(f["ring"].as_u64().unwrap(), f["slot"].as_u64().unwrap())].read(&points)
}

#[test]
fn a_client_that_never_sends_camera_sees_auto() {
    let mut app = edit_app();
    let r = call(&mut app, "bevy-ts/viewport_mode", json!({})).unwrap();
    assert_eq!((r["camera"].as_str(), r["gameCameras"].as_u64()), (Some("auto"), Some(1)), "{r}");
    let r = call(&mut app, "bevy-ts/viewport_mode", json!({ "mode": "edit" })).unwrap();
    assert_eq!((r["mode"].as_str(), r["camera"].as_str(), r["gameCameras"].as_u64()), (Some("edit"), Some("auto"), Some(1)), "{r}");
    updates(&mut app, 2);
    assert_eq!(is_on::<With<EditorCamera>>(&mut app), vec![true], "auto in edit is the editor camera");
    assert_eq!(is_on::<With<GameCamera>>(&mut app), vec![false]);
    let r = call(&mut app, "bevy-ts/viewport_mode", json!({ "mode": "play" })).unwrap();
    assert_eq!((r["camera"].as_str(), r["gameCameras"].as_u64()), (Some("auto"), Some(1)), "{r}");
}

#[test]
fn the_game_view_shows_what_play_draws_and_the_editor_view_comes_back() {
    let mut app = edit_app();
    let s = scene(&app);
    let (_, viewer) = attach(&mut app, 160.0, 120.0, 1.0);
    let mut seen = Seen::default();
    call(&mut app, "bevy-ts/viewport_mode", json!({ "mode": "edit" })).unwrap();
    call(&mut app, "bevy-ts/viewport_select", json!({ "entities": [s.free.to_bits()] })).unwrap();
    let editor = free_row(&mut app, &viewer, &mut seen, 6);
    let pose = call(&mut app, "bevy-ts/viewport_camera", Value::Null).unwrap();
    let y0 = app.world().get::<Transform>(s.root).unwrap().translation.y;

    let r = view(&mut app, "game");
    // `paused` is the TS clock's; this app has none, and the bobbing cube below says it is paused.
    assert_eq!((r["mode"].as_str(), r["camera"].as_str(), r["gameCameras"].as_u64()), (Some("edit"), Some("game"), Some(1)), "{r}");
    let game = free_row(&mut app, &viewer, &mut seen, 6);
    assert_eq!(is_on::<With<GameCamera>>(&mut app), vec![true]);
    assert!(is_on::<With<EditorOnly>>(&mut app).iter().all(|on| !on), "the editor camera and its overlays are off");
    assert_eq!(app.world_mut().query_filtered::<(), With<bevy::gizmos::transform_gizmo::TransformGizmoFocus>>().iter(app.world()).count(), 0, "no handle");
    assert_eq!(app.world().get::<Transform>(s.root).unwrap().translation.y, y0, "the world stays paused");
    assert_ne!(editor, game, "the editor view draws the outline and handle over the cube");

    // Back to the editor view: same pose, same selection.
    let r = view(&mut app, "editor");
    assert_eq!(r["camera"], "editor");
    updates(&mut app, 2);
    assert_eq!(call(&mut app, "bevy-ts/viewport_camera", Value::Null).unwrap(), pose, "the editor camera kept its pose");
    assert_eq!(selection(&mut app)["items"][0]["entity"], s.free.to_bits(), "the selection is kept");
    assert_eq!(is_on::<With<EditorCamera>>(&mut app), vec![true]);
    assert_eq!(is_on::<With<GameCamera>>(&mut app), vec![false]);
    assert_eq!(free_row(&mut app, &viewer, &mut seen, 6), editor, "and drawn again");

    // Play draws what the game view drew.
    view(&mut app, "game");
    call(&mut app, "bevy-ts/viewport_mode", json!({ "mode": "play" })).unwrap();
    assert_eq!(free_row(&mut app, &viewer, &mut seen, 6), game, "the game view is a Play frame");
}

#[test]
fn the_game_view_never_picks_or_acts_on_keys() {
    let mut app = edit_app();
    let s = scene(&app);
    call(&mut app, "bevy-ts/viewport_mode", json!({ "mode": "edit" })).unwrap();
    call(&mut app, "bevy-ts/viewport_select", json!({ "entities": [s.free.to_bits()] })).unwrap();
    updates(&mut app, 2);
    let p = center(&app, s.root);
    let root = screen(&mut app, p);
    view(&mut app, "game");
    updates(&mut app, 2);
    click(&mut app, root, json!({}));
    assert_eq!(selection(&mut app)["items"][0]["entity"], s.free.to_bits(), "a click selects nothing");
    key_press(&mut app, "KeyE");
    assert_eq!(call(&mut app, "bevy-ts/viewport_tools", json!({})).unwrap()["mode"], "translate", "no tool key acts");
    let (code, message) = refused_call(&mut app, "bevy-ts/viewport_pick", json!({ "x": root.x, "y": root.y }));
    assert_eq!((code, message.as_str()), (-32602, "Picking needs the editor camera."));
}

/// A second game camera, drawing a red square in the corner over the main one.
fn minimap(app: &mut App, on: bool) -> Entity {
    let viewport = bevy::camera::Viewport { physical_position: UVec2::new(0, 0), physical_size: UVec2::new(40, 30), ..default() };
    let clear = ClearColorConfig::Custom(Color::srgb(1.0, 0.0, 0.0));
    app.world_mut()
        .spawn((Camera3d::default(), Camera { order: 1, is_active: on, viewport: Some(viewport), clear_color: clear, ..default() }, Transform::from_xyz(0.0, 50.0, 0.0).looking_at(Vec3::ZERO, Vec3::Z)))
        .id()
}

fn corner_red(app: &mut App, viewer: &bevy_ts_viewport::mach::Viewer, seen: &mut Seen) -> bool {
    let n = seen.frames.len() + 6;
    run_until(app, viewer, seen, LONG, |s| s.frames.len() >= n);
    let f = seen.frames.last().unwrap();
    let [b, g, r, _] = seen.surfaces[&(f["ring"].as_u64().unwrap(), f["slot"].as_u64().unwrap())].read(&[(5, 5)])[0];
    r > 200 && g < 60 && b < 60
}

#[test]
fn two_game_cameras_draw_in_their_order() {
    let mut app = edit_app();
    let (_, viewer) = attach(&mut app, 160.0, 120.0, 1.0);
    let mut seen = Seen::default();
    minimap(&mut app, true);
    updates(&mut app, 1);
    assert!(corner_red(&mut app, &viewer, &mut seen), "play draws the minimap over the main view");
    call(&mut app, "bevy-ts/viewport_mode", json!({ "mode": "edit" })).unwrap();
    assert!(!corner_red(&mut app, &viewer, &mut seen), "the editor view does not");
    let r = view(&mut app, "game");
    assert_eq!(r["gameCameras"], 2, "{r}");
    assert!(corner_red(&mut app, &viewer, &mut seen), "the game view does, as play");
}

#[test]
fn a_camera_that_starts_off_is_not_counted_and_stays_off() {
    let mut app = edit_app();
    let (_, viewer) = attach(&mut app, 160.0, 120.0, 1.0);
    let mut seen = Seen::default();
    let off = minimap(&mut app, false);
    updates(&mut app, 1);
    call(&mut app, "bevy-ts/viewport_mode", json!({ "mode": "edit" })).unwrap();
    let r = view(&mut app, "game");
    assert_eq!(r["gameCameras"], 1, "{r}");
    assert!(!corner_red(&mut app, &viewer, &mut seen));
    assert!(!app.world().get::<Camera>(off).unwrap().is_active);
    assert_eq!(is_on::<With<GameCamera>>(&mut app), vec![true]);
}

// Booting in edit mode (`bevy-ts dev --edit`, openspec change `dev-edit`), against the fake
// backend: `Startup` (`setup`, `wave`), an `Update` system (`tick`) and an observer on
// `Insert<Transform>` (`moved`) count their runs.

mod boot {
    use super::*;
    use bevy_ts::EngineMode;
    use bevy_ts::backend::ModuleSet;
    use bevy_ts::fake::{FakeFn, FakeHandle, FakeProgram};
    use bevy_ts::generation::{self, ReloadStatus};
    use bevy_ts::manifest::{ComponentRef, ComponentSource, Lifecycle, Manifest, ParamDecl, ScheduleDecl, Stage, SystemDecl};
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicU32, Ordering};

    /// Runs per TS system name.
    #[derive(Resource, Clone, Default)]
    struct Runs(Arc<Mutex<BTreeMap<&'static str, u32>>>);

    impl Runs {
        fn of(&self, name: &str) -> u32 {
            self.0.lock().unwrap().get(name).copied().unwrap_or(0)
        }
    }

    fn decl(name: &str, schedule: ScheduleDecl, params: Vec<ParamDecl>) -> SystemDecl {
        SystemDecl { name: name.into(), schedule, params, after: vec![], before: vec![], sets: vec![], condition: None, groups: vec![], location: None }
    }

    /// `setup` in `Startup`, `tick` in `Update`, `moved` on `Insert<Transform>`, and `wave` in
    /// `Startup` when asked.
    fn program(runs: &Runs, wave: bool) -> FakeProgram {
        let moved = ParamDecl::On { event: None, lifecycle: Some(Lifecycle::Insert), component: Some(ComponentRef { name: "Transform".into(), source: ComponentSource::Rust }) };
        let mut systems = vec![
            ("setup", decl("game.ts#setup", ScheduleDecl::Startup, vec![])),
            ("tick", decl("game.ts#tick", ScheduleDecl::Stage { stage: Stage::Update }, vec![])),
            ("moved", decl("game.ts#moved", ScheduleDecl::Observer, vec![moved])),
        ];
        if wave {
            systems.push(("wave", decl("game.ts#wave", ScheduleDecl::Startup, vec![])));
        }
        let fns = systems
            .iter()
            .map(|(name, _)| {
                let (runs, name) = (runs.clone(), *name);
                Box::new(move |_: &mut bevy_ts::fake::FakeCall| {
                    *runs.0.lock().unwrap().entry(name).or_default() += 1;
                    Ok(())
                }) as FakeFn
            })
            .collect();
        FakeProgram { manifest: Manifest { systems: systems.into_iter().map(|(_, d)| d).collect(), ..Default::default() }, systems: fns, ..Default::default() }
    }

    /// Counts frames of a native system gated on play.
    #[derive(Resource, Clone, Default)]
    struct Spins(Arc<AtomicU32>);

    /// The test app with bevy-ts on the fake backend, booted as `bevy-ts dev --viewport [--edit]`
    /// boots, its first generation applied.
    fn ts_app(edit: bool) -> (App, FakeHandle, Runs, Spins) {
        let (mut app, fake, runs, spins) = ts_app_bare(edit, |_| {});
        reload(&mut app, &fake, &runs, false, false);
        updates(&mut app, 3);
        (app, fake, runs, spins)
    }

    /// The same, with `extra` added and no generation applied yet.
    fn ts_app_bare(edit: bool, extra: impl FnOnce(&mut App)) -> (App, FakeHandle, Runs, Spins) {
        let fake = FakeHandle::default();
        let (h, spins) = (fake.clone(), Spins::default());
        let s = spins.clone();
        let mut app = app_with(|app| {
            app.add_plugins(bevy_ts::BevyTsPlugin::new(move || Box::new(h.backend())).dev(true));
            bevy_ts_remote::add_methods(app);
            app.add_systems(Update, (move || {
                s.0.fetch_add(1, Ordering::Relaxed);
            })
            .run_if(in_state(EngineMode::Play)));
            if edit {
                app.insert_state(EngineMode::Edit);
                app.world_mut().resource_mut::<generation::Generations>().started = false;
                app.world_mut().resource_mut::<bevy_ts::runtime::ScriptClock>().paused = true;
            }
            extra(app);
        });
        let runs = Runs::default();
        app.update();
        (app, fake, runs, spins)
    }

    /// A Stop: its rebuild loads the live set again, so the fake needs a program for it.
    fn stop(app: &mut App, fake: &FakeHandle, runs: &Runs, wave: bool) -> Value {
        fake.push(program(runs, wave));
        call(app, "bevy-ts/viewport_mode", json!({ "mode": "edit" })).unwrap()
    }

    fn reload(app: &mut App, fake: &FakeHandle, runs: &Runs, wave: bool, restart: bool) {
        fake.push(program(runs, wave));
        let set = ModuleSet { entry: PathBuf::from("/game.ts"), modules: vec![] };
        assert_eq!(generation::apply(app.world_mut(), set, restart).status, ReloadStatus::Applied);
    }

    /// A core method (`bevy-ts-remote`), called as a client would.
    fn core(app: &mut App, method: &str, params: Value) -> Result<Value, String> {
        let id = match app.world().resource::<bevy::remote::RemoteMethods>().get(method) {
            Some(bevy::remote::RemoteMethodSystemId::Instant(id)) => *id,
            _ => panic!("no method {method}"),
        };
        app.world_mut().run_system_with(id, Some(params)).unwrap().map_err(|e| e.message)
    }

    fn mode(app: &mut App) -> Value {
        call(app, "bevy-ts/viewport_mode", json!({})).unwrap()
    }

    fn engine_mode(app: &App) -> EngineMode {
        *app.world().resource::<State<EngineMode>>().get()
    }

    #[test]
    fn booting_in_edit_runs_no_game_code_until_the_first_play() {
        let (mut app, fake, runs, spins) = ts_app(true);
        let m = mode(&mut app);
        assert_eq!((m["mode"].as_str(), m["started"].as_bool(), m["paused"].as_bool()), (Some("edit"), Some(false), Some(true)));
        assert_eq!(engine_mode(&app), EngineMode::Edit);
        assert_eq!((runs.of("setup"), runs.of("tick")), (0, 0), "no Startup, no Update");
        assert_eq!(spins.0.load(Ordering::Relaxed), 0, "a native system gated on play stays still");
        assert!(app.world_mut().query_filtered::<&Camera, With<EditorCamera>>().single(app.world()).unwrap().is_active, "the editor camera is on");
        let t = app.world().resource::<Time<Virtual>>().elapsed_secs();
        updates(&mut app, 3);
        assert_eq!(app.world().resource::<Time<Virtual>>().elapsed_secs(), t, "virtual time stands still");

        // The studio's switch to edit after `hello` changes nothing.
        let pose = call(&mut app, "bevy-ts/viewport_camera", Value::Null).unwrap();
        let r = call(&mut app, "bevy-ts/viewport_mode", json!({ "mode": "edit" })).unwrap();
        assert_eq!((r["mode"].as_str(), r["started"].as_bool()), (Some("edit"), Some(false)));
        updates(&mut app, 3);
        assert_eq!((runs.of("setup"), runs.of("tick")), (0, 0));
        assert_eq!(call(&mut app, "bevy-ts/viewport_camera", Value::Null).unwrap(), pose, "the editor camera keeps its pose");

        let r = call(&mut app, "bevy-ts/viewport_mode", json!({ "mode": "play" })).unwrap();
        assert_eq!((r["mode"].as_str(), r["started"].as_bool()), (Some("play"), Some(true)));
        assert_eq!(runs.of("setup"), 1, "the first play runs Startup, at once");
        assert_eq!(engine_mode(&app), EngineMode::Play, "the state follows in the same frame");
        updates(&mut app, 3);
        assert!(runs.of("tick") >= 2, "and the game runs");
        assert!(spins.0.load(Ordering::Relaxed) >= 2);

        // Stop, then play again: the game starts over, and Startup runs once more.
        let r = stop(&mut app, &fake, &runs, false);
        assert_eq!((r["mode"].as_str(), r["started"].as_bool()), (Some("edit"), Some(false)));
        assert_eq!(engine_mode(&app), EngineMode::Edit);
        updates(&mut app, 1);
        let spun = spins.0.load(Ordering::Relaxed);
        updates(&mut app, 3);
        assert_eq!(spins.0.load(Ordering::Relaxed), spun, "the native system stops in edit");
        let r = call(&mut app, "bevy-ts/viewport_mode", json!({ "mode": "play" })).unwrap();
        assert_eq!(r["started"], true);
        updates(&mut app, 2);
        assert_eq!(runs.of("setup"), 2);
    }

    #[test]
    fn a_step_before_the_game_started_runs_startup_and_stays_in_edit() {
        let (mut app, _fake, runs, _) = ts_app(true);
        core(&mut app, "bevy-ts/step", json!({ "frames": 1 })).unwrap();
        assert_eq!(runs.of("setup"), 1);
        updates(&mut app, 3);
        assert_eq!(runs.of("tick"), 1, "one frame");
        let m = mode(&mut app);
        assert_eq!((m["mode"].as_str(), m["started"].as_bool()), (Some("edit"), Some(true)));
        // Play after that resumes without running Startup again.
        call(&mut app, "bevy-ts/viewport_mode", json!({ "mode": "play" })).unwrap();
        updates(&mut app, 2);
        assert_eq!(runs.of("setup"), 1);
        assert!(runs.of("tick") > 1);
    }

    #[test]
    fn resuming_a_game_that_never_started_fails() {
        let (mut app, _fake, runs, _) = ts_app(true);
        let e = core(&mut app, "bevy-ts/pause", json!({ "paused": false })).unwrap_err();
        assert!(e.contains("the game has not started; switch to play"), "{e}");
        updates(&mut app, 3);
        assert_eq!(runs.of("tick"), 0);
        assert!(app.world().resource::<bevy_ts::runtime::ScriptClock>().paused);
    }

    #[test]
    fn edit_time_triggers_do_not_fire_on_play() {
        let (mut app, fake, runs, _) = ts_app(true);
        let e = app.world_mut().spawn(Transform::default()).id();
        updates(&mut app, 1);
        // A drag in edit mode, then play.
        app.world_mut().entity_mut(e).insert(Transform::from_xyz(1.0, 0.0, 0.0));
        updates(&mut app, 1);
        app.world_mut().entity_mut(e).insert(Transform::from_xyz(2.0, 0.0, 0.0));
        call(&mut app, "bevy-ts/viewport_mode", json!({ "mode": "play" })).unwrap();
        updates(&mut app, 3);
        assert_eq!(runs.of("moved"), 0, "the drag fires no game observer, before or after play");
        // In play the observer runs.
        app.world_mut().entity_mut(e).insert(Transform::from_xyz(3.0, 0.0, 0.0));
        updates(&mut app, 1);
        assert_eq!(runs.of("moved"), 1);

        // Edit again: a drag just before play does not fire either.
        stop(&mut app, &fake, &runs, false);
        updates(&mut app, 1);
        app.world_mut().entity_mut(e).insert(Transform::from_xyz(4.0, 0.0, 0.0));
        call(&mut app, "bevy-ts/viewport_mode", json!({ "mode": "play" })).unwrap();
        updates(&mut app, 3);
        assert_eq!(runs.of("moved"), 1);
    }

    #[test]
    fn a_reload_before_the_first_play_adds_a_startup_that_runs_at_play() {
        let (mut app, fake, runs, _) = ts_app(true);
        reload(&mut app, &fake, &runs, true, false);
        updates(&mut app, 2);
        assert_eq!((runs.of("setup"), runs.of("wave")), (0, 0));
        call(&mut app, "bevy-ts/viewport_mode", json!({ "mode": "play" })).unwrap();
        assert_eq!((runs.of("setup"), runs.of("wave")), (1, 1));

        // A restart in edit mode leaves the game not started; the next play starts it again.
        stop(&mut app, &fake, &runs, true);
        reload(&mut app, &fake, &runs, true, true);
        assert_eq!(mode(&mut app)["started"], false);
        assert_eq!(runs.of("setup"), 1);
        call(&mut app, "bevy-ts/viewport_mode", json!({ "mode": "play" })).unwrap();
        assert_eq!((runs.of("setup"), runs.of("wave")), (2, 2));
    }

    #[test]
    fn without_edit_the_game_starts_at_boot_in_play() {
        let (mut app, _fake, runs, spins) = ts_app(false);
        assert_eq!(engine_mode(&app), EngineMode::Play);
        assert_eq!(runs.of("setup"), 1);
        assert!(runs.of("tick") >= 2 && spins.0.load(Ordering::Relaxed) >= 2);
        let m = mode(&mut app);
        assert_eq!((m["mode"].as_str(), m["started"].as_bool()), (Some("play"), Some(true)));
    }

    // Play and Stop (openspec change `play-stop`).

    /// A TS-spawned entity, as a TS system's `commands.spawn` makes one.
    fn spawn_ts(app: &mut App) -> Entity {
        app.world_mut().spawn((bevy_ts::commands::TsSpawned, Transform::default())).id()
    }

    #[test]
    fn stop_throws_the_play_world_away() {
        let (mut app, fake, runs, _) = ts_app(true);
        call(&mut app, "bevy-ts/viewport_mode", json!({ "mode": "play" })).unwrap();
        let bullet = spawn_ts(&mut app);
        updates(&mut app, 3);
        let restarts = fake.restarts();
        let r = stop(&mut app, &fake, &runs, false);
        assert_eq!((r["mode"].as_str(), r["started"].as_bool(), r["paused"].as_bool()), (Some("edit"), Some(false), Some(true)));
        assert!(app.world().get_entity(bullet).is_err(), "what TS spawned is gone");
        assert_eq!(fake.restarts(), restarts + 1, "a fresh isolate");
        updates(&mut app, 3);
        assert_eq!(runs.of("setup"), 1, "no Startup in edit");
    }

    #[test]
    fn stopping_a_paused_game_rebuilds_it_too() {
        let (mut app, fake, runs, _) = ts_app(true);
        call(&mut app, "bevy-ts/viewport_mode", json!({ "mode": "play" })).unwrap();
        let bullet = spawn_ts(&mut app);
        core(&mut app, "bevy-ts/pause", json!({ "paused": true })).unwrap();
        updates(&mut app, 2);
        let r = stop(&mut app, &fake, &runs, false);
        assert_eq!(r["started"], false);
        assert!(app.world().get_entity(bullet).is_err());
    }

    #[test]
    fn time_starts_again_from_zero_and_the_editor_camera_stays() {
        let (mut app, fake, runs, _) = ts_app(true);
        let pose = json!({ "translation": [4.0, 3.0, 8.0] });
        call(&mut app, "bevy-ts/viewport_camera", pose).unwrap();
        updates(&mut app, 2);
        let before = call(&mut app, "bevy-ts/viewport_camera", Value::Null).unwrap();
        call(&mut app, "bevy-ts/viewport_mode", json!({ "mode": "play" })).unwrap();
        updates(&mut app, 10);
        assert!(app.world().resource::<Time<Virtual>>().elapsed_secs() > 0.0);
        stop(&mut app, &fake, &runs, false);
        assert_eq!(app.world().resource::<Time<Virtual>>().elapsed_secs(), 0.0);
        assert_eq!(app.world().resource::<Time>().elapsed_secs(), 0.0, "what a Startup system reads");
        assert_eq!(app.world().resource::<bevy_ts::runtime::ScriptFrame>().0, 0);
        assert!(app.world().resource::<Time<Virtual>>().is_paused());
        updates(&mut app, 3);
        assert_eq!(app.world().resource::<Time<Virtual>>().elapsed_secs(), 0.0, "and stays paused in edit");
        assert_eq!(call(&mut app, "bevy-ts/viewport_camera", Value::Null).unwrap(), before, "the editor camera keeps its pose");
    }

    #[test]
    fn a_selected_bullet_is_dropped_on_stop() {
        let (mut app, fake, runs, _) = ts_app(true);
        call(&mut app, "bevy-ts/viewport_mode", json!({ "mode": "play" })).unwrap();
        let bullet = spawn_ts(&mut app);
        updates(&mut app, 1);
        call(&mut app, "bevy-ts/viewport_select", json!({ "entities": [bullet.to_bits()] })).unwrap();
        assert_eq!(selection(&mut app)["items"].as_array().unwrap().len(), 1);
        stop(&mut app, &fake, &runs, false);
        let sel = app.world().resource::<bevy_ts_viewport::edit::Selection>();
        assert!(sel.items.is_empty());
        assert_eq!(sel.origin, bevy_ts_viewport::edit::Origin::Engine);
    }

    /// `viewport_mode` as a client sees a refusal: the error's message and `data`.
    fn set_mode(app: &mut App, mode: &str) -> Result<Value, (String, Option<Value>, i16)> {
        let id = match app.world().resource::<bevy::remote::RemoteMethods>().get("bevy-ts/viewport_mode") {
            Some(bevy::remote::RemoteMethodSystemId::Instant(id)) => *id,
            _ => panic!("no viewport_mode"),
        };
        app.world_mut().run_system_with(id, Some(json!({ "mode": mode }))).unwrap().map_err(|e| (e.message, e.data, e.code))
    }

    #[test]
    fn a_compile_error_refuses_play_until_fixed() {
        let (mut app, fake, runs, _) = ts_app(true);
        let error = bevy_ts::backend::ScriptError::at("Unexpected token", Some(SourceLocation { file: "src/game.tsx".into(), line: 3, column: 7 }));
        generation::compile_failed(app.world_mut(), vec![error], Duration::ZERO);
        let (message, data, code) = set_mode(&mut app, "play").unwrap_err();
        assert_eq!(message, "src/game.tsx:3: Unexpected token");
        assert_eq!(data, Some(json!({ "reason": "compile-errors" })));
        assert_eq!(code, bevy_ts_viewport::edit::PLAY_REFUSED);
        let m = mode(&mut app);
        assert_eq!((m["mode"].as_str(), m["started"].as_bool()), (Some("edit"), Some(false)));
        updates(&mut app, 3);
        assert_eq!((runs.of("setup"), runs.of("tick")), (0, 0), "no TS system ran");

        // Fixed: the save applies and the game plays.
        reload(&mut app, &fake, &runs, false, false);
        assert_eq!(set_mode(&mut app, "play").unwrap()["mode"], "play");
        assert_eq!(runs.of("setup"), 1);
    }

    #[test]
    fn a_play_before_any_generation_is_refused() {
        let (mut app, _fake, _runs, _) = ts_app_bare(true, |_| {});
        let (message, data, _) = set_mode(&mut app, "play").unwrap_err();
        assert!(!message.is_empty());
        assert_eq!(data, Some(json!({ "reason": "no-generation" })));
        assert_eq!(mode(&mut app)["mode"], "edit");
    }

    #[test]
    fn a_compile_running_does_not_block_play() {
        let (mut app, _fake, runs, _) = ts_app(true);
        let live = app.world().resource::<generation::Generations>().live;
        app.world_mut().resource_mut::<generation::ReloadLog>().push(generation::ReloadReport {
            seq: 0,
            generation: live + 1,
            status: ReloadStatus::Compiling,
            errors: vec![],
            migrations: vec![],
            duration_ms: 0.0,
        });
        assert_eq!(set_mode(&mut app, "play").unwrap()["mode"], "play");
        assert_eq!(runs.of("setup"), 1);
    }

    /// One subscriber of `viewport_mode+watch`, polled once a frame as the server polls it.
    struct ModeStream(Value);

    impl ModeStream {
        fn open() -> Self {
            ModeStream(json!({ "n": rand_key() }))
        }

        fn poll(&self, app: &mut App) -> Option<Value> {
            let id = match app.world().resource::<bevy::remote::RemoteMethods>().get("bevy-ts/viewport_mode+watch") {
                Some(bevy::remote::RemoteMethodSystemId::Watching(id)) => *id,
                _ => panic!("no mode watch"),
            };
            let v = app.world_mut().run_system_with(id, Some(self.0.clone())).unwrap().unwrap();
            if let Some(v) = &v {
                validate("bevy-ts/viewport_mode+watch", v);
            }
            v
        }

        /// The messages over `n` frames.
        fn frames(&self, app: &mut App, n: usize) -> Vec<Value> {
            let mut out = vec![];
            for _ in 0..n {
                app.update();
                out.extend(self.poll(app));
            }
            out
        }
    }

    fn summary(v: &Value) -> (String, bool, bool, String) {
        (v["mode"].as_str().unwrap().into(), v["started"].as_bool().unwrap(), v["paused"].as_bool().unwrap(), v["cause"].as_str().unwrap().into())
    }

    #[test]
    fn the_mode_is_streamed() {
        let (mut app, fake, runs, _) = ts_app(true);
        let s = ModeStream::open();
        let first = s.poll(&mut app).expect("a message on open");
        assert_eq!(summary(&first), ("edit".into(), false, true, "engine".into()));
        assert!(s.frames(&mut app, 3).is_empty(), "nothing while nothing changes");

        call(&mut app, "bevy-ts/viewport_mode", json!({ "mode": "play" })).unwrap();
        let m = s.frames(&mut app, 3);
        assert_eq!(m.iter().map(summary).collect::<Vec<_>>(), vec![("play".into(), true, false, "client".into())]);

        stop(&mut app, &fake, &runs, false);
        let m = s.frames(&mut app, 3);
        assert_eq!(m.iter().map(summary).collect::<Vec<_>>(), vec![("edit".into(), false, true, "client".into())], "one message per Stop");

        // A refused Play: the mode as it was, with `failed`.
        generation::compile_failed(app.world_mut(), vec![bevy_ts::backend::ScriptError::new("boom")], Duration::ZERO);
        assert!(set_mode(&mut app, "play").is_err());
        let m = s.frames(&mut app, 3);
        assert_eq!(m.iter().map(summary).collect::<Vec<_>>(), vec![("edit".into(), false, true, "failed".into())]);

        // A step before the game started.
        reload(&mut app, &fake, &runs, false, false);
        s.frames(&mut app, 1);
        core(&mut app, "bevy-ts/step", json!({ "frames": 1 })).unwrap();
        let m = s.frames(&mut app, 3);
        assert_eq!(m.iter().map(summary).collect::<Vec<_>>(), vec![("edit".into(), true, true, "client".into())]);

        // A restart reload in edit: not started again.
        reload(&mut app, &fake, &runs, false, true);
        let m = s.frames(&mut app, 3);
        assert_eq!(m.iter().map(summary).collect::<Vec<_>>(), vec![("edit".into(), false, true, "restart".into())]);

        // A new subscriber gets the mode with `engine`.
        let again = ModeStream::open().poll(&mut app).unwrap();
        assert_eq!(summary(&again).3, "engine");
    }

    /// A native crate's frame counter, reset in `OnExit(Play)`, and what the TS rebuild saw.
    #[derive(Resource, Default)]
    struct Frames(u32);

    #[derive(Resource, Clone, Default)]
    struct SeenAtRestart(Arc<Mutex<Vec<u32>>>);

    #[derive(Component)]
    struct Leftover;

    #[test]
    fn a_native_crate_resets_itself_on_leaving_play() {
        use bevy_ts::plugin_api::BevyTsAppExt;
        let seen = SeenAtRestart::default();
        let s = seen.clone();
        let (mut app, fake, runs, _) = ts_app_bare(true, move |app| {
            app.init_resource::<Frames>()
                .add_systems(Update, (|mut f: ResMut<Frames>| f.0 += 1).run_if(in_state(EngineMode::Play)))
                .add_systems(OnExit(EngineMode::Play), |mut f: ResMut<Frames>| f.0 = 0)
                .on_ts_restart(move |world| s.0.lock().unwrap().push(world.resource::<Frames>().0));
        });
        reload(&mut app, &fake, &runs, false, false);
        call(&mut app, "bevy-ts/viewport_mode", json!({ "mode": "play" })).unwrap();
        let rust = app.world_mut().spawn(Leftover).id();
        updates(&mut app, 5);
        assert!(app.world().resource::<Frames>().0 >= 4);
        stop(&mut app, &fake, &runs, false);
        assert_eq!(*seen.0.lock().unwrap(), vec![0], "the counter is 0 when the rebuild runs");
        assert!(app.world().get_entity(rust).is_ok(), "bevy-ts leaves what Rust spawned");
    }

    /// The game camera of the shared scene: the one camera that is not the editor's.
    fn game_camera(app: &mut App) -> Entity {
        let world = app.world_mut();
        world.query_filtered::<Entity, (With<Camera>, Without<EditorOnly>)>().single(world).unwrap()
    }

    fn on(app: &App, e: Entity) -> bool {
        app.world().get::<Camera>(e).unwrap().is_active
    }

    fn editor_on(app: &mut App) -> bool {
        let world = app.world_mut();
        world.query_filtered::<&Camera, With<EditorCamera>>().single(world).unwrap().is_active
    }

    #[test]
    fn stepping_in_the_game_view_renders_through_the_game_camera() {
        let (mut app, _fake, runs, _) = ts_app(true);
        let cam = game_camera(&mut app);
        assert_eq!(super::view(&mut app, "game")["camera"], "game");
        core(&mut app, "bevy-ts/step", json!({ "frames": 1 })).unwrap();
        updates(&mut app, 3);
        assert_eq!((runs.of("setup"), runs.of("tick")), (1, 1), "one frame");
        let m = mode(&mut app);
        assert_eq!((m["mode"].as_str(), m["camera"].as_str(), m["paused"].as_bool()), (Some("edit"), Some("game"), Some(true)));
        assert!(on(&app, cam) && !editor_on(&mut app), "seen from the game camera");
    }

    #[test]
    fn play_and_stop_keep_the_camera_choice() {
        let (mut app, fake, runs, _) = ts_app(true);
        let cam = game_camera(&mut app);
        super::view(&mut app, "game");
        let r = call(&mut app, "bevy-ts/viewport_mode", json!({ "mode": "play" })).unwrap();
        assert_eq!((r["mode"].as_str(), r["camera"].as_str()), (Some("play"), Some("game")));
        updates(&mut app, 2);
        assert!(on(&app, cam) && !editor_on(&mut app));
        let r = stop(&mut app, &fake, &runs, false);
        assert_eq!((r["mode"].as_str(), r["camera"].as_str(), r["started"].as_bool()), (Some("edit"), Some("game"), Some(false)));
        updates(&mut app, 2);
        assert!(on(&app, cam) && !editor_on(&mut app), "the game view again, over the rebuilt world");
        // Play with the editor choice in force, and Stop: the editor view.
        super::view(&mut app, "editor");
        updates(&mut app, 2);
        assert!(!on(&app, cam) && editor_on(&mut app));
    }

    #[test]
    fn the_editor_camera_in_play_is_refused() {
        let (mut app, _fake, _runs, _) = ts_app(true);
        let s = ModeStream::open();
        s.poll(&mut app).unwrap();
        // Entering play with it: refused, still edit.
        let (code, message) = super::refused_call(&mut app, "bevy-ts/viewport_mode", json!({ "mode": "play", "camera": "editor" }));
        assert_eq!((code, message.as_str()), (-32602, "The editor camera is not available in Play."));
        let m = s.frames(&mut app, 2);
        assert_eq!(m.iter().map(|v| (v["mode"].as_str().unwrap(), v["camera"].as_str().unwrap(), v["cause"].as_str().unwrap())).collect::<Vec<_>>(), vec![("edit", "auto", "failed")]);
        assert_eq!(engine_mode(&app), EngineMode::Edit);
        // In play: refused, still play, the choice unchanged.
        call(&mut app, "bevy-ts/viewport_mode", json!({ "mode": "play", "camera": "game" })).unwrap();
        s.frames(&mut app, 2);
        let (code, _) = super::refused_call(&mut app, "bevy-ts/viewport_mode", json!({ "camera": "editor" }));
        assert_eq!(code, -32602);
        let m = s.frames(&mut app, 2);
        assert_eq!(m.iter().map(|v| (v["mode"].as_str().unwrap(), v["camera"].as_str().unwrap(), v["cause"].as_str().unwrap())).collect::<Vec<_>>(), vec![("play", "game", "failed")]);
        // `auto` is accepted in play.
        assert_eq!(call(&mut app, "bevy-ts/viewport_mode", json!({ "camera": "auto" })).unwrap()["camera"], "auto");
    }

    /// Stands in for a TS `Startup` system that spawns the game's only camera: once the game has
    /// started.
    fn camera_at_start(world: &mut World, mut done: Local<bool>) {
        if !*done && generation::started(world) {
            *done = true;
            world.spawn((Camera3d::default(), Transform::from_xyz(0.0, 0.0, 5.0).looking_at(Vec3::ZERO, Vec3::Y)));
        }
    }

    #[test]
    fn a_game_with_no_camera_yet_clears_to_its_clear_colour() {
        let (mut app, fake, runs, _) = ts_app_bare(true, |app| {
            app.insert_resource(ClearColor(Color::srgb(1.0, 0.0, 1.0))).add_systems(Update, camera_at_start);
        });
        reload(&mut app, &fake, &runs, false, false);
        let shared = game_camera(&mut app);
        app.world_mut().despawn(shared);
        let (_, viewer) = attach(&mut app, 160.0, 120.0, 1.0);
        let mut seen = Seen::default();
        // The editor view first, so a stale frame would show.
        let n = seen.frames.len() + 6;
        run_until(&mut app, &viewer, &mut seen, LONG, |s| s.frames.len() >= n);
        let r = super::view(&mut app, "game");
        assert_eq!((r["camera"].as_str(), r["gameCameras"].as_u64()), (Some("game"), Some(0)), "{r}");
        let magenta = |[b, g, r, _]: [u8; 4]| r > 240 && g < 15 && b > 240;
        let px = pixel_shown(&mut app, &viewer, &mut seen, Vec2::new(80.0, 60.0), magenta);
        assert!(magenta(px), "the clear colour, no stale editor frame: bgra {px:?}");
        let f = seen.frames.last().unwrap();
        let all = seen.surfaces[&(f["ring"].as_u64().unwrap(), f["slot"].as_u64().unwrap())].read(&[(2, 2), (158, 2), (2, 118), (158, 118), (80, 60), (40, 90)]);
        assert!(all.iter().all(|p| magenta(*p)), "{all:?}");
        core(&mut app, "bevy-ts/step", json!({ "frames": 1 })).unwrap();
        updates(&mut app, 3);
        let m = mode(&mut app);
        assert_eq!((m["gameCameras"].as_u64(), m["started"].as_bool()), (Some(1), Some(true)), "{m}");
        let cam = game_camera(&mut app);
        assert!(on(&app, cam), "the view shows that camera");
    /// Each start notification, with whether a `Startup` had run in the world by then.
    #[derive(Resource, Clone, Default)]
    struct Starts(Arc<Mutex<Vec<bool>>>);

    fn counting_starts() -> (Starts, impl FnOnce(&mut App)) {
        use bevy_ts::plugin_api::BevyTsAppExt;
        let starts = Starts::default();
        let s = starts.clone();
        (starts, move |app: &mut App| {
            app.on_ts_start(move |world| s.0.lock().unwrap().push(!world.resource::<generation::Generations>().startups_run.is_empty()));
        })
    }

    #[test]
    fn a_step_then_play_notifies_the_start_once_before_startup() {
        let (starts, extra) = counting_starts();
        let (mut app, fake, runs, _) = ts_app_bare(true, extra);
        reload(&mut app, &fake, &runs, false, false);
        updates(&mut app, 3);
        assert!(starts.0.lock().unwrap().is_empty(), "no start in edit");
        core(&mut app, "bevy-ts/step", json!({ "frames": 1 })).unwrap();
        call(&mut app, "bevy-ts/viewport_mode", json!({ "mode": "play" })).unwrap();
        updates(&mut app, 2);
        assert_eq!(*starts.0.lock().unwrap(), vec![false], "once, before the step's Startup");
    }

    #[test]
    fn play_stop_play_notifies_the_start_twice() {
        let (starts, extra) = counting_starts();
        let (mut app, fake, runs, _) = ts_app_bare(true, extra);
        reload(&mut app, &fake, &runs, false, false);
        call(&mut app, "bevy-ts/viewport_mode", json!({ "mode": "play" })).unwrap();
        updates(&mut app, 2);
        stop(&mut app, &fake, &runs, false);
        reload(&mut app, &fake, &runs, false, false);
        updates(&mut app, 2);
        call(&mut app, "bevy-ts/viewport_mode", json!({ "mode": "play" })).unwrap();
        updates(&mut app, 2);
        assert_eq!(*starts.0.lock().unwrap(), vec![false, false], "once per Play; a reload in edit starts nothing");
    }

    #[test]
    fn a_game_booted_in_play_notifies_at_its_first_startup() {
        let (starts, extra) = counting_starts();
        let (mut app, fake, runs, _) = ts_app_bare(false, extra);
        reload(&mut app, &fake, &runs, false, false);
        reload(&mut app, &fake, &runs, false, false);
        updates(&mut app, 2);
        assert_eq!(*starts.0.lock().unwrap(), vec![false], "once; a hot reload is no start");
    }
}
