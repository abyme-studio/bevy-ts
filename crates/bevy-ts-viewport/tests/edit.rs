//! Edit mode in the hole: modes, the editor camera, picking that selects a prefab instance,
//! selection sync, and gizmo drags becoming routed edits. TSX provenance is stood in for by
//! `TsxNode` components shaped as the reconciler writes them.
#![cfg(target_os = "macos")]

mod common;

use bevy::picking::events::PointerClick;
use bevy::prelude::*;
use bevy_ts::backend::SourceLocation;
use bevy_ts_tsx::tree::{FieldState, OwnedComponent, Provenance, TsxNode};
use bevy_ts_viewport::edit::{EditorCamera, EditorOnly};
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
    components.insert("Transform".to_string(), OwnedComponent { fields, source: Some(at("enemy.tsx", 3)) });
    TsxNode { id, root: "game".into(), key_path: vec!["b".into()], types: vec!["Enemy".into()], source, components }
}

/// A game camera, a bobbing enemy prefab instance `b` (a body and an `eye` child) placed at
/// `game.tsx:5`, and a cube no source describes.
fn edit_scene(mut commands: Commands, mut meshes: ResMut<Assets<Mesh>>, mut materials: ResMut<Assets<StandardMaterial>>) {
    let mesh = meshes.add(Cuboid::from_size(Vec3::ONE));
    let small = meshes.add(Cuboid::from_size(Vec3::splat(0.4)));
    let mat = MeshMaterial3d(materials.add(StandardMaterial::default()));
    commands.spawn((GameCamera, Camera3d::default(), Transform::from_xyz(0.0, 0.0, 10.0).looking_at(Vec3::ZERO, Vec3::Y)));
    commands.spawn((DirectionalLight::default(), Transform::from_xyz(3.0, 6.0, 2.0).looking_at(Vec3::ZERO, Vec3::Y)));
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
    for _ in 0..3 {
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
    assert!(t.distance(game_pose.translation()) < 1e-4, "the editor camera starts at the game camera's pose");
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
