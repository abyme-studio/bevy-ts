//! `MaterialAlpha` (changes `material-alpha`, `material-fade`), headless: the engine's plugins with no window and no
//! GPU. `ship.gltf` (scene-material's) has three mesh entities: two on material `hull`, one on
//! `trim`.

use bevy::light::NotShadowCaster;
use bevy::prelude::*;
use bevy::render::RenderPlugin;
use bevy::render::settings::{RenderCreation, WgpuSettings};
use bevy::window::ExitCondition;
use bevy::winit::WinitPlugin;
use bevy::gltf::GltfMaterialName;
use bevy::world_serialization::WorldAssetRoot;
use bevy_ts_material::layout::{Base, Bindings, ParamType};
use bevy_ts_material::{FadeMaterial, Kind, MaterialAlpha, MaterialAlphaTargets, SceneMaterialTargets, ScriptMaterial, ScriptMaterial3d, ScriptMaterialRenderPlugin};
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

/// The `StandardMaterial` `e` draws; panics on a faded one.
fn drawn(app: &App, e: Entity) -> Handle<StandardMaterial> {
    assert!(app.world().get::<MeshMaterial3d<FadeMaterial>>(e).is_none(), "{e} holds no fade copy");
    app.world().get::<MeshMaterial3d<StandardMaterial>>(e).unwrap().0.clone()
}

/// The fade copy `e` draws; panics on an unfaded one.
fn faded_copy(app: &App, e: Entity) -> Handle<FadeMaterial> {
    assert!(app.world().get::<MeshMaterial3d<StandardMaterial>>(e).is_none(), "{e} holds no StandardMaterial while faded");
    app.world().get::<MeshMaterial3d<FadeMaterial>>(e).unwrap().0.clone()
}

fn get(app: &App, h: &Handle<StandardMaterial>) -> StandardMaterial {
    app.world().resource::<Assets<StandardMaterial>>().get(h).unwrap().clone()
}

fn copy(app: &App, h: &Handle<FadeMaterial>) -> FadeMaterial {
    app.world().resource::<Assets<FadeMaterial>>().get(h).unwrap().clone()
}

/// Material assets of both kinds.
fn count(app: &App) -> usize {
    app.world().resource::<Assets<StandardMaterial>>().len() + app.world().resource::<Assets<FadeMaterial>>().len()
}

#[test]
fn a_shape_fades_with_a_copy_and_its_twin_does_not() {
    let mut app = app();
    let m = red(&mut app);
    let faded = cube(&mut app, &m, MaterialAlpha { alpha: 0.5 });
    let twin = cube(&mut app, &m, ());
    app.update();

    let c = faded_copy(&app, faded);
    assert_eq!(drawn(&app, twin), m, "its twin keeps the shared material");
    let f = copy(&app, &c);
    assert_eq!(f.extension.alpha, 0.5);
    assert_eq!((f.base.base_color, f.base.alpha_mode), (Color::srgb(1.0, 0.0, 0.0), AlphaMode::Opaque), "an opaque copy keeps its colour and mode");
    let o = get(&app, &m);
    assert_eq!((o.base_color.alpha(), o.alpha_mode), (1.0, AlphaMode::Opaque), "the original is untouched");
    assert_eq!(app.world().get::<MaterialAlphaTargets>(faded).unwrap().targets, vec![faded]);

    // A changed alpha edits the copy in place: no asset added, the same handle drawn.
    let before = count(&app);
    for a in [0.4, 0.3, 1.0, 0.0] {
        app.world_mut().get_mut::<MaterialAlpha>(faded).unwrap().alpha = a;
        app.update();
        assert_eq!(faded_copy(&app, faded), c);
        assert_eq!(count(&app), before);
        let f = copy(&app, &c);
        assert_eq!((f.extension.alpha, f.base.alpha_mode), (a, AlphaMode::Opaque));
    }

    // Clamped, not refused.
    app.world_mut().get_mut::<MaterialAlpha>(faded).unwrap().alpha = 3.0;
    app.update();
    assert_eq!(copy(&app, &c).extension.alpha, 1.0);

    // Removal restores the original and drops the copy.
    let id = c.id();
    drop(c);
    app.world_mut().entity_mut(faded).remove::<MaterialAlpha>();
    app.update();
    assert_eq!(drawn(&app, faded), m, "its StandardMaterial back, no fade copy");
    assert!(app.world().get::<MaterialAlphaTargets>(faded).is_none());
    app.update();
    app.update();
    assert!(app.world().resource::<Assets<FadeMaterial>>().get(id).is_none(), "the copy is freed");
}

#[test]
fn a_blending_original_fades_through_its_colour() {
    for mode in [AlphaMode::Blend, AlphaMode::Add] {
        let mut app = app();
        let half = app.world_mut().resource_mut::<Assets<StandardMaterial>>().add(StandardMaterial {
            base_color: Color::srgba(1.0, 0.0, 0.0, 0.5),
            alpha_mode: mode,
            ..default()
        });
        let e = cube(&mut app, &half, MaterialAlpha { alpha: 0.5 });
        app.update();
        let c = copy(&app, &faded_copy(&app, e));
        assert_eq!(c.base.base_color.alpha(), 0.25, "{mode:?}: the original's alpha times alpha");
        assert_eq!(c.base.alpha_mode, mode, "a mode that blends already is kept");
        assert_eq!(c.extension.alpha, 0.5, "its shadow dithers");
        assert_eq!(app.world().get::<MaterialAlphaTargets>(e).unwrap().copies().count(), 1);
    }
}

#[test]
fn an_alpha_animated_in_animation_systems_reaches_the_copy_that_frame() {
    let mut app = app();
    let m = red(&mut app);
    let e = cube(&mut app, &m, MaterialAlpha { alpha: 1.0 });
    app.update();
    app.add_systems(PostUpdate, (|mut q: Query<&mut MaterialAlpha>| q.iter_mut().for_each(|mut a| a.alpha = 0.25)).in_set(bevy::app::AnimationSystems));
    app.update();
    assert_eq!(copy(&app, &faded_copy(&app, e)).extension.alpha, 0.25);
}

#[test]
fn a_mask_original_keeps_its_mode() {
    let mut app = app();
    let mask = app.world_mut().resource_mut::<Assets<StandardMaterial>>().add(StandardMaterial { alpha_mode: AlphaMode::Mask(0.3), ..default() });
    let e = cube(&mut app, &mask, MaterialAlpha { alpha: 0.25 });
    app.update();
    let c = copy(&app, &faded_copy(&app, e));
    assert_eq!((c.base.alpha_mode, c.base.base_color.alpha(), c.extension.alpha), (AlphaMode::Mask(0.3), 1.0, 0.25));
}

#[test]
fn an_authored_not_shadow_caster_stays() {
    let mut app = app();
    let m = red(&mut app);
    let e = cube(&mut app, &m, (NotShadowCaster, MaterialAlpha { alpha: 1.0 }));
    let plain = cube(&mut app, &m, MaterialAlpha { alpha: 0.0 });
    app.update();
    assert!(app.world().get::<NotShadowCaster>(e).is_some());
    assert!(app.world().get::<NotShadowCaster>(plain).is_none(), "alpha 0 adds none: the dither hides the shadow");
    app.world_mut().entity_mut(e).remove::<MaterialAlpha>();
    app.update();
    assert!(app.world().get::<NotShadowCaster>(e).is_some(), "removal keeps it");
}

#[test]
fn a_shape_given_another_material_is_faded_again() {
    let mut app = app();
    let m = red(&mut app);
    let e = cube(&mut app, &m, MaterialAlpha { alpha: 0.5 });
    app.update();
    let first = faded_copy(&app, e);

    // A TS save re-inserts the same original: the same copy comes back.
    app.world_mut().entity_mut(e).insert(MeshMaterial3d(m.clone()));
    app.update();
    assert_eq!(faded_copy(&app, e), first);

    // A new colour: a copy of the new material, at the same alpha.
    let blue = app.world_mut().resource_mut::<Assets<StandardMaterial>>().add(StandardMaterial::from(Color::srgb(0.0, 0.0, 1.0)));
    app.world_mut().entity_mut(e).insert(MeshMaterial3d(blue.clone()));
    app.update();
    let c = copy(&app, &faded_copy(&app, e));
    assert_eq!((c.base.base_color.to_srgba().blue, c.extension.alpha), (1.0, 0.5));

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
    faded_copy(&app, parent);
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
    let c = faded_copy(&app, e);
    app.world_mut().resource_mut::<Assets<StandardMaterial>>().get_mut(&m).unwrap().perceptual_roughness = 0.1;
    app.update();
    let c = copy(&app, &c);
    assert_eq!((c.base.perceptual_roughness, c.extension.alpha), (0.1, 0.5));
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

/// What each scene mesh draws, by glTF material name: its `StandardMaterial`, or its fade copy.
#[derive(Clone, Debug, PartialEq)]
enum Drawn {
    Standard(Handle<StandardMaterial>),
    Fade(Handle<FadeMaterial>),
    /// A script material.
    Other,
}

fn drawn_by_name(app: &App, root: Entity) -> Vec<(String, Drawn)> {
    let w = app.world();
    let mut out: Vec<_> = meshes(w, root)
        .into_iter()
        .map(|(e, n)| {
            let d = match (w.get::<MeshMaterial3d<StandardMaterial>>(e), w.get::<MeshMaterial3d<FadeMaterial>>(e)) {
                (Some(m), None) => Drawn::Standard(m.0.clone()),
                (None, Some(m)) => Drawn::Fade(m.0.clone()),
                (None, None) => Drawn::Other,
                (Some(_), Some(_)) => panic!("{n} holds both"),
            };
            (n, d)
        })
        .collect();
    out.sort_by(|a, b| a.0.cmp(&b.0));
    out
}

fn standard(d: &Drawn) -> Handle<StandardMaterial> {
    match d {
        Drawn::Standard(h) => h.clone(),
        d => panic!("not a StandardMaterial: {d:?}"),
    }
}

fn fade(d: &Drawn) -> Handle<FadeMaterial> {
    match d {
        Drawn::Fade(h) => h.clone(),
        d => panic!("not a fade copy: {d:?}"),
    }
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
    for ((n, c), (_, original)) in faded.iter().zip(&originals) {
        let (c, original) = (fade(c), standard(original));
        assert_eq!(t.original(c.id()), Some(&original), "{n} draws a copy of its original");
        let (c, o) = (copy(&app, &c), get(&app, &original));
        assert_eq!((c.base.base_color, c.base.alpha_mode, c.extension.alpha), (o.base_color, o.alpha_mode, 0.5));
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
    for (n, d) in drawn_by_name(&app, root) {
        assert!(t.original(fade(&d).id()).is_some(), "{n} is faded again");
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
    // Faded first, so scene-material must ask alpha.rs for the hull's originals.
    let root = ship(&mut app, MaterialAlpha { alpha: 0.5 });
    app.world_mut().entity_mut(root).insert((ScriptMaterial3d(s.clone()), bevy_ts_material::GltfMaterialFilter("hull".into())));
    app.update();
    let script = |app: &App| meshes(app.world(), root).into_iter().filter(|(e, _)| app.world().get::<ScriptMaterial3d>(*e).is_some_and(|m| m.0 == s)).map(|(_, n)| n).collect::<Vec<_>>();
    assert_eq!(script(&app), ["hull", "hull"], "hull keeps the script material, unfaded");
    let trim = drawn_by_name(&app, root).into_iter().find(|(n, _)| n == "trim").unwrap().1;
    fade(&trim);

    if remove_alpha_first {
        app.world_mut().entity_mut(root).remove::<MaterialAlpha>();
        app.update();
        assert_eq!(script(&app).len(), 2);
        assert_eq!(drawn_by_name(&app, root)[2], originals[2], "trim restored");
        for (_, h) in &app.world().get::<SceneMaterialTargets>(root).unwrap().0 {
            assert!(originals.iter().any(|(_, o)| standard(o) == *h), "scene-material keeps an original, not a copy");
        }
        app.world_mut().entity_mut(root).remove::<ScriptMaterial3d>();
        app.update();
        assert_eq!(drawn_by_name(&app, root), originals);
    } else {
        app.world_mut().entity_mut(root).remove::<ScriptMaterial3d>();
        app.update();
        app.update();
        let t = app.world().get::<MaterialAlphaTargets>(root).unwrap();
        for (n, d) in drawn_by_name(&app, root) {
            assert!(t.original(fade(&d).id()).is_some(), "{n} fades once its script material goes");
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
