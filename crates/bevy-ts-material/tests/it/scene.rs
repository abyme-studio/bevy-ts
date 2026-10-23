//! A script material on a glTF scene root (change `scene-material`), headless: the engine's
//! plugins with no window and no GPU. `ship.gltf` has three mesh entities: two on material
//! `hull`, one on `trim`.

use bevy::gltf::GltfMaterialName;
use bevy::prelude::*;
use bevy::render::RenderPlugin;
use bevy::render::settings::{RenderCreation, WgpuSettings};
use bevy::window::ExitCondition;
use bevy::winit::WinitPlugin;
use bevy::world_serialization::{WorldAssetRoot, WorldInstance, WorldInstanceReady};
use bevy_ts_material::layout::{Base, Bindings, ParamType};
use bevy_ts_material::{GltfMaterialFilter, Kind, SceneMaterialTargets, ScriptMaterial, ScriptMaterial3d, ScriptMaterialRenderPlugin};
use std::path::PathBuf;
use std::sync::Arc;

const SHIP: &str = "models/ship.gltf#Scene0";

fn app() -> App {
    let mut app = App::new();
    let render = RenderPlugin { render_creation: RenderCreation::Automatic(Box::new(WgpuSettings { backends: None, ..default() })), ..default() };
    let assets = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/assets");
    app.add_plugins(
        DefaultPlugins
            .build()
            .disable::<WinitPlugin>()
            .disable::<bevy::log::LogPlugin>()
            .set(WindowPlugin { primary_window: None, exit_condition: ExitCondition::DontExit, ..default() })
            .set(render)
            .set(AssetPlugin { file_path: assets.to_string_lossy().into_owned(), watch_for_changes_override: Some(false), ..default() }),
    )
    .add_plugins(ScriptMaterialRenderPlugin);
    // The glTF loader registers in `finish`, which `update` alone never runs.
    app.finish();
    app.cleanup();
    app
}

fn material(app: &mut App, tint: f32) -> Handle<ScriptMaterial> {
    let bindings = Bindings::new("test::Tint", [("tint", &ParamType::Base(Base::F32))], &[]).unwrap();
    let kind = Arc::new(Kind::new("test::Tint", bindings, None, None, AlphaMode::Opaque));
    let mut data = kind.bindings.params().defaults();
    kind.bindings.params().encode("tint", &serde_json::json!(tint), &mut data).unwrap();
    app.world_mut().resource_mut::<Assets<ScriptMaterial>>().add(ScriptMaterial { kind, data, textures: vec![] })
}

/// Frames until `done` holds. Shaders and assets finish on other threads, which a loaded machine
/// slows by any amount, so the bound is a long wall-clock one that names what never came.
fn until(app: &mut App, what: &str, done: impl Fn(&World) -> bool) {
    let bound = std::time::Duration::from_secs(120);
    let start = std::time::Instant::now();
    loop {
        app.update();
        if done(app.world()) {
            return;
        }
        assert!(start.elapsed() < bound, "never within {bound:?}: {what}");
        std::thread::sleep(std::time::Duration::from_millis(1));
    }
}

/// A ship root, its scene ready.
fn ship(app: &mut App, extra: impl Bundle) -> Entity {
    let scene = app.world().resource::<AssetServer>().load(SHIP);
    let root = app.world_mut().spawn((WorldAssetRoot(scene), Transform::default(), Visibility::default(), extra)).id();
    until(app, "the ship's meshes", |w| meshes(w, root).len() == 3);
    app.update();
    root
}

/// The root's mesh descendants, each with its glTF material name.
fn meshes(world: &World, root: Entity) -> Vec<(Entity, String)> {
    let mut out = vec![];
    let mut stack = vec![root];
    while let Some(e) = stack.pop() {
        if world.get::<Mesh3d>(e).is_some() {
            out.push((e, world.get::<GltfMaterialName>(e).map(|n| n.0.clone()).unwrap_or_default()));
        }
        if let Some(c) = world.get::<Children>(e) {
            stack.extend(c.iter());
        }
    }
    out
}

/// How many of the root's meshes named `name` hold `m`, and how many keep a `StandardMaterial`.
fn count(world: &World, root: Entity, name: &str, m: &Handle<ScriptMaterial>) -> (usize, usize) {
    let of: Vec<Entity> = meshes(world, root).into_iter().filter(|(_, n)| n == name).map(|(e, _)| e).collect();
    let script = of.iter().filter(|e| world.get::<ScriptMaterial3d>(**e).is_some_and(|s| s.0 == *m)).count();
    let standard = of.iter().filter(|e| world.get::<MeshMaterial3d<StandardMaterial>>(**e).is_some()).count();
    (script, standard)
}

#[test]
fn every_mesh_of_the_scene_takes_it() {
    let mut app = app();
    let m = material(&mut app, 1.0);
    let root = ship(&mut app, ScriptMaterial3d(m.clone()));
    assert_eq!(count(app.world(), root, "hull", &m), (2, 0));
    assert_eq!(count(app.world(), root, "trim", &m), (1, 0));
    assert_eq!(app.world().get::<SceneMaterialTargets>(root).unwrap().0.len(), 3);
}

#[test]
fn a_filter_takes_only_the_named_material() {
    let mut app = app();
    let m = material(&mut app, 1.0);
    let root = ship(&mut app, (ScriptMaterial3d(m.clone()), GltfMaterialFilter("hull".into())));
    assert_eq!(count(app.world(), root, "hull", &m), (2, 0));
    assert_eq!(count(app.world(), root, "trim", &m), (0, 1));
}

#[test]
fn new_props_reach_the_meshes_and_removing_restores() {
    let mut app = app();
    let root = ship(&mut app, ());
    // Each mesh's own material, before any swap.
    let originals: Vec<(Entity, AssetId<StandardMaterial>)> =
        meshes(app.world(), root).into_iter().map(|(e, _)| (e, app.world().get::<MeshMaterial3d<StandardMaterial>>(e).unwrap().0.id())).collect();
    let a = material(&mut app, 1.0);
    app.world_mut().entity_mut(root).insert((ScriptMaterial3d(a.clone()), GltfMaterialFilter("hull".into())));
    app.update();
    assert_eq!(count(app.world(), root, "hull", &a), (2, 0));
    let b = material(&mut app, 2.0);
    app.world_mut().entity_mut(root).insert(ScriptMaterial3d(b.clone()));
    app.update();
    assert_eq!(count(app.world(), root, "hull", &b), (2, 0), "the new instance, the same frame");
    // The filter moves to trim: hull goes back, trim takes it.
    app.world_mut().entity_mut(root).insert(GltfMaterialFilter("trim".into()));
    app.update();
    assert_eq!(count(app.world(), root, "hull", &b), (0, 2));
    assert_eq!(count(app.world(), root, "trim", &b), (1, 0));
    // No filter: every mesh.
    app.world_mut().entity_mut(root).remove::<GltfMaterialFilter>();
    app.update();
    assert_eq!(count(app.world(), root, "hull", &b), (2, 0));
    app.world_mut().entity_mut(root).remove::<ScriptMaterial3d>();
    app.update();
    assert_eq!(count(app.world(), root, "hull", &b), (0, 2));
    assert_eq!(count(app.world(), root, "trim", &b), (0, 1));
    assert!(app.world().get::<SceneMaterialTargets>(root).is_none());
    // The very handles the scene gave, back on the same entities.
    for (e, id) in originals {
        assert_eq!(app.world().get::<MeshMaterial3d<StandardMaterial>>(e).unwrap().0.id(), id);
    }
}

#[test]
fn a_material_added_after_the_scene_is_ready_applies() {
    let mut app = app();
    let root = ship(&mut app, ());
    let m = material(&mut app, 1.0);
    app.world_mut().entity_mut(root).insert(ScriptMaterial3d(m.clone()));
    app.update();
    assert_eq!(count(app.world(), root, "hull", &m), (2, 0));
}

#[test]
fn a_scene_ready_again_is_swapped_again_and_tsx_children_are_left_alone() {
    let mut app = app();
    let m = material(&mut app, 1.0);
    let root = ship(&mut app, ScriptMaterial3d(m.clone()));
    // A child the scene did not spawn, as a TSX child with its own mesh and material.
    let cube = app.world_mut().resource_mut::<Assets<Mesh>>().add(Cuboid::default());
    let own = app.world_mut().resource_mut::<Assets<StandardMaterial>>().add(StandardMaterial::default());
    let child = app.world_mut().spawn((Mesh3d(cube), MeshMaterial3d(own.clone()), ChildOf(root))).id();
    // A swapped mesh lost, and the ready event again, as a respawn does.
    let (gone, _) = meshes(app.world(), root).into_iter().find(|(e, _)| *e != child).unwrap();
    app.world_mut().entity_mut(gone).insert(MeshMaterial3d(own.clone())).remove::<ScriptMaterial3d>();
    let i = **app.world().get::<WorldInstance>(root).unwrap();
    app.world_mut().trigger(WorldInstanceReady { entity: root, instance_id: i });
    app.update();
    assert!(app.world().get::<ScriptMaterial3d>(gone).is_some_and(|s| s.0 == m));
    assert_eq!(app.world().get::<MeshMaterial3d<StandardMaterial>>(child).unwrap().0, own);
    assert!(app.world().get::<ScriptMaterial3d>(child).is_none());
}
