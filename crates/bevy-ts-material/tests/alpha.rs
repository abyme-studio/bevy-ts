//! `MaterialAlpha` (change `material-alpha`), headless: the engine's plugins with no window and no
//! GPU.

use bevy::prelude::*;
use bevy::render::RenderPlugin;
use bevy::render::settings::{RenderCreation, WgpuSettings};
use bevy::window::ExitCondition;
use bevy::winit::WinitPlugin;
use bevy_ts_material::{MaterialAlpha, MaterialAlphaTargets, ScriptMaterialRenderPlugin};
use std::path::PathBuf;

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
