//! `MaterialAlpha` (change `material-alpha`), headless: the engine's plugins with no window and no
//! GPU. `ship.gltf` (scene-material's) has three mesh entities: two on material `hull`, one on
//! `trim`.

use bevy::prelude::*;
use bevy::render::RenderPlugin;
use bevy::render::settings::{RenderCreation, WgpuSettings};
use bevy::window::ExitCondition;
use bevy::winit::WinitPlugin;
use bevy::gltf::GltfMaterialName;
use bevy::world_serialization::WorldAssetRoot;
use bevy_ts_material::layout::{Base, Bindings, ParamType};
use bevy_ts_material::{Kind, MaterialAlpha, MaterialAlphaTargets, SceneMaterialTargets, ScriptMaterial, ScriptMaterial3d, ScriptMaterialRenderPlugin};
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
    app.finish();
    app.cleanup();
    app
}

fn red(app: &mut App) -> Handle<StandardMaterial> {
    app.world_mut().resource_mut::<Assets<StandardMaterial>>().add(StandardMaterial::from(Color::srgb(1.0, 0.0, 0.0)))
}

fn cube(app: &mut App, material: &Handle<StandardMaterial>, extra: impl Bundle) -> Entity {
    let mesh = app.world_mut().resource_mut::<Assets<Mesh>>().add(Cuboid::default());
    app.world_mut().spawn((Mesh3d(mesh), MeshMaterial3d(material.clone()), extra)).id()
}

fn drawn(app: &App, e: Entity) -> Handle<StandardMaterial> {
    app.world().get::<MeshMaterial3d<StandardMaterial>>(e).unwrap().0.clone()
}

fn get(app: &App, h: &Handle<StandardMaterial>) -> StandardMaterial {
    app.world().resource::<Assets<StandardMaterial>>().get(h).unwrap().clone()
}

fn count(app: &App) -> usize {
    app.world().resource::<Assets<StandardMaterial>>().len()
}

#[test]
fn a_shape_fades_with_a_copy_and_its_twin_does_not() {
    let mut app = app();
    let m = red(&mut app);
    let faded = cube(&mut app, &m, MaterialAlpha { alpha: 0.5 });
    let twin = cube(&mut app, &m, ());
    app.update();

    let copy = drawn(&app, faded);
    assert_ne!(copy, m, "the faded cube draws a copy");
    assert_eq!(drawn(&app, twin), m, "its twin keeps the shared material");
    let c = get(&app, &copy);
    assert_eq!(c.base_color.alpha(), 0.5);
    assert_eq!(c.alpha_mode, AlphaMode::Blend);
    let o = get(&app, &m);
    assert_eq!((o.base_color.alpha(), o.alpha_mode), (1.0, AlphaMode::Opaque), "the original is untouched");
    assert_eq!(app.world().get::<MaterialAlphaTargets>(faded).unwrap().targets, vec![faded]);

    // A changed alpha edits the copy in place: no asset added, the same handle drawn.
    let before = count(&app);
    for a in [0.4, 0.3, 1.0, 0.0] {
        app.world_mut().get_mut::<MaterialAlpha>(faded).unwrap().alpha = a;
        app.update();
        assert_eq!(drawn(&app, faded), copy);
        assert_eq!(count(&app), before);
        let c = get(&app, &copy);
        assert_eq!(c.base_color.alpha(), a);
        assert_eq!(c.alpha_mode, if a < 1.0 { AlphaMode::Blend } else { AlphaMode::Opaque }, "at 1 the copy equals the original");
    }

    // Clamped, not refused.
    app.world_mut().get_mut::<MaterialAlpha>(faded).unwrap().alpha = 3.0;
    app.update();
    assert_eq!(get(&app, &copy).base_color.alpha(), 1.0);

    // Removal restores the original and drops the copy.
    let id = copy.id();
    drop(copy);
    app.world_mut().entity_mut(faded).remove::<MaterialAlpha>();
    app.update();
    assert_eq!(drawn(&app, faded), m);
    assert!(app.world().get::<MaterialAlphaTargets>(faded).is_none());
    app.update();
    app.update();
    assert!(app.world().resource::<Assets<StandardMaterial>>().get(id).is_none(), "the copy is freed");
}

#[test]
fn one_copy_per_original_and_alpha_multiplies() {
    let mut app = app();
    let half = app.world_mut().resource_mut::<Assets<StandardMaterial>>().add(StandardMaterial {
        base_color: Color::srgba(1.0, 0.0, 0.0, 0.5),
        alpha_mode: AlphaMode::Add,
        ..default()
    });
    let e = cube(&mut app, &half, MaterialAlpha { alpha: 0.5 });
    app.update();
    let c = get(&app, &drawn(&app, e));
    assert_eq!(c.base_color.alpha(), 0.25, "the original's alpha times alpha");
    assert_eq!(c.alpha_mode, AlphaMode::Add, "a mode that blends already is kept");
    assert_eq!(app.world().get::<MaterialAlphaTargets>(e).unwrap().copies().count(), 1);
}

#[test]
fn a_shape_given_another_material_is_faded_again() {
    let mut app = app();
    let m = red(&mut app);
    let e = cube(&mut app, &m, MaterialAlpha { alpha: 0.5 });
    app.update();
    let first = drawn(&app, e);

    // A TS save re-inserts the same original: the same copy comes back.
    app.world_mut().entity_mut(e).insert(MeshMaterial3d(m.clone()));
    app.update();
    assert_eq!(drawn(&app, e), first);

    // A new colour: a copy of the new material, at the same alpha.
    let blue = app.world_mut().resource_mut::<Assets<StandardMaterial>>().add(StandardMaterial::from(Color::srgb(0.0, 0.0, 1.0)));
    app.world_mut().entity_mut(e).insert(MeshMaterial3d(blue.clone()));
    app.update();
    let second = drawn(&app, e);
    assert_ne!(second, blue);
    let c = get(&app, &second);
    assert_eq!((c.base_color.to_srgba().blue, c.base_color.alpha()), (1.0, 0.5));

    app.world_mut().entity_mut(e).remove::<MaterialAlpha>();
    app.update();
    assert_eq!(drawn(&app, e), blue, "removal leaves it the newest material, opaque");
}

#[test]
fn a_tsx_child_is_untouched() {
    let mut app = app();
    let m = red(&mut app);
    let parent = cube(&mut app, &m, MaterialAlpha { alpha: 0.5 });
    let child = cube(&mut app, &m, ChildOf(parent));
    app.update();
    assert_ne!(drawn(&app, parent), m);
    assert_eq!(drawn(&app, child), m);
    // Its material changing asks the parent to sync, which still leaves it alone.
    app.world_mut().entity_mut(child).insert(MeshMaterial3d(m.clone()));
    app.update();
    assert_eq!(drawn(&app, child), m);
}

#[test]
fn an_original_that_reloads_is_copied_again() {
    let mut app = app();
    let m = red(&mut app);
    let e = cube(&mut app, &m, MaterialAlpha { alpha: 0.5 });
    app.update();
    let copy = drawn(&app, e);
    app.world_mut().resource_mut::<Assets<StandardMaterial>>().get_mut(&m).unwrap().perceptual_roughness = 0.1;
    app.update();
    let c = get(&app, &copy);
    assert_eq!((c.perceptual_roughness, c.base_color.alpha()), (0.1, 0.5));
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

/// The root's mesh descendants, each with its glTF material name.
fn meshes(world: &World, root: Entity) -> Vec<(Entity, String)> {
    let mut out = vec![];
    let mut stack = vec![root];
    while let Some(e) = stack.pop() {
        if world.get::<Mesh3d>(e).is_some() && world.get::<GltfMaterialName>(e).is_some() {
            out.push((e, world.get::<GltfMaterialName>(e).unwrap().0.clone()));
        }
        if let Some(c) = world.get::<Children>(e) {
            stack.extend(c.iter());
        }
    }
    out
}

/// A ship root, its scene ready.
fn ship(app: &mut App, extra: impl Bundle) -> Entity {
    let scene = app.world().resource::<AssetServer>().load(SHIP);
    let root = app.world_mut().spawn((WorldAssetRoot(scene), Transform::default(), Visibility::default(), extra)).id();
    until(app, "the ship's meshes", |w| meshes(w, root).len() == 3);
    app.update();
    root
}

/// Each scene mesh's `StandardMaterial` handle, by glTF material name; none for a script material.
fn drawn_by_name(app: &App, root: Entity) -> Vec<(String, Option<Handle<StandardMaterial>>)> {
    let mut out: Vec<_> = meshes(app.world(), root).into_iter().map(|(e, n)| (n, app.world().get::<MeshMaterial3d<StandardMaterial>>(e).map(|m| m.0.clone()))).collect();
    out.sort_by(|a, b| a.0.cmp(&b.0));
    out
}

fn tint(app: &mut App) -> Handle<ScriptMaterial> {
    let bindings = Bindings::new("test::Tint", [("tint", &ParamType::Base(Base::F32))], &[]).unwrap();
    let kind = Arc::new(Kind::new("test::Tint", bindings, None, None, AlphaMode::Opaque));
    let data = kind.bindings.params().defaults();
    app.world_mut().resource_mut::<Assets<ScriptMaterial>>().add(ScriptMaterial { kind, data, textures: vec![] })
}

#[test]
fn a_scene_root_fades_its_meshes_and_another_instance_does_not() {
    let mut app = app();
    let plain = ship(&mut app, ());
    let originals = drawn_by_name(&app, plain);
    let root = ship(&mut app, MaterialAlpha { alpha: 0.5 });
    assert_eq!(drawn_by_name(&app, plain), originals, "the other instance keeps the model's materials");

    let faded = drawn_by_name(&app, root);
    let t = app.world().get::<MaterialAlphaTargets>(root).unwrap();
    assert_eq!(t.targets.len(), 3);
    assert_eq!(t.copies().count(), 2, "one copy per distinct original: hull and trim");
    for ((n, copy), (_, original)) in faded.iter().zip(&originals) {
        let (copy, original) = (copy.clone().unwrap(), original.clone().unwrap());
        assert_ne!(copy, original, "{n} draws a copy");
        assert_eq!(t.original(copy.id()), Some(&original));
        let (c, o) = (get(&app, &copy), get(&app, &original));
        assert_eq!(c.base_color.alpha(), o.base_color.alpha() * 0.5);
        assert_eq!(c.alpha_mode, AlphaMode::Blend);
        assert_eq!(o.alpha_mode, AlphaMode::Opaque, "the original is untouched");
    }
    assert_eq!(faded[0].1, faded[1].1, "both hull meshes share the root's one hull copy");

    let before = count(&app);
    app.world_mut().get_mut::<MaterialAlpha>(root).unwrap().alpha = 0.2;
    app.update();
    assert_eq!(count(&app), before, "a changed alpha adds no asset");
    assert_eq!(drawn_by_name(&app, root), faded);

    app.world_mut().entity_mut(root).remove::<MaterialAlpha>();
    app.update();
    assert_eq!(drawn_by_name(&app, root), originals, "removal restores");
    assert!(app.world().get::<MaterialAlphaTargets>(root).is_none());
}

#[test]
fn a_respawned_scene_is_faded_again_and_a_tsx_child_is_not() {
    let mut app = app();
    let root = ship(&mut app, MaterialAlpha { alpha: 0.5 });
    let m = red(&mut app);
    let child = cube(&mut app, &m, ChildOf(root));
    app.update();
    assert_eq!(drawn(&app, child), m, "a TSX child is not the scene's");

    let before: Vec<Entity> = meshes(app.world(), root).into_iter().map(|(e, _)| e).collect();
    let scene = app.world().get::<WorldAssetRoot>(root).unwrap().0.clone();
    app.world_mut().entity_mut(root).insert(WorldAssetRoot(scene));
    until(&mut app, "the ship spawned again", |w| {
        let now = meshes(w, root);
        now.len() == 3 && now.iter().all(|(e, _)| !before.contains(e))
    });
    app.update();
    let t = app.world().get::<MaterialAlphaTargets>(root).unwrap();
    for (n, h) in drawn_by_name(&app, root) {
        assert!(t.original(h.unwrap().id()).is_some(), "{n} is faded again");
    }
    assert_eq!(t.copies().count(), 2, "the same originals, the same copies");
    assert_eq!(drawn(&app, child), m);
}

/// Hull takes a script material, trim fades; each removal leaves what the other still asks.
fn script_and_alpha(remove_alpha_first: bool) {
    let mut app = app();
    let plain = ship(&mut app, ());
    let originals = drawn_by_name(&app, plain);
    let s = tint(&mut app);
    // Faded first, so scene-material keeps the hull's copies as the materials to put back.
    let root = ship(&mut app, MaterialAlpha { alpha: 0.5 });
    app.world_mut().entity_mut(root).insert((ScriptMaterial3d(s.clone()), bevy_ts_material::GltfMaterialFilter("hull".into())));
    app.update();
    let script = |app: &App| meshes(app.world(), root).into_iter().filter(|(e, _)| app.world().get::<ScriptMaterial3d>(*e).is_some_and(|m| m.0 == s)).map(|(_, n)| n).collect::<Vec<_>>();
    assert_eq!(script(&app), ["hull", "hull"], "hull keeps the script material, unfaded");
    let trim = drawn_by_name(&app, root).into_iter().find(|(n, _)| n == "trim").unwrap().1.unwrap();
    assert_ne!(Some(trim), originals[2].1.clone(), "trim fades");

    if remove_alpha_first {
        app.world_mut().entity_mut(root).remove::<MaterialAlpha>();
        app.update();
        assert_eq!(script(&app).len(), 2);
        assert_eq!(drawn_by_name(&app, root)[2], originals[2], "trim restored");
        for (_, h) in &app.world().get::<SceneMaterialTargets>(root).unwrap().0 {
            assert!(originals.iter().any(|(_, o)| o.as_ref() == Some(h)), "scene-material keeps an original, not a copy");
        }
        app.world_mut().entity_mut(root).remove::<ScriptMaterial3d>();
        app.update();
        assert_eq!(drawn_by_name(&app, root), originals);
    } else {
        app.world_mut().entity_mut(root).remove::<ScriptMaterial3d>();
        app.update();
        app.update();
        let t = app.world().get::<MaterialAlphaTargets>(root).unwrap();
        for (n, h) in drawn_by_name(&app, root) {
            assert!(t.original(h.unwrap().id()).is_some(), "{n} fades once its script material goes");
        }
        app.world_mut().entity_mut(root).remove::<MaterialAlpha>();
        app.update();
        assert_eq!(drawn_by_name(&app, root), originals);
    }
}

#[test]
fn a_script_material_mesh_is_left_alone_removing_alpha_first() {
    script_and_alpha(true);
}

#[test]
fn a_script_material_mesh_is_left_alone_removing_the_material_first() {
    script_and_alpha(false);
}
