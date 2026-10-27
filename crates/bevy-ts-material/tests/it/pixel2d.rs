//! Pixel-art 2D (change `pixel-art-2d`): `YSort` writes `Transform` z before Bevy's propagation,
//! once per change, and never `GlobalTransform`.

use bevy::prelude::*;
use bevy::transform::TransformPlugin;
use bevy_ts::log::{LogLevel, ScriptLog};
use bevy_ts_material::pixel2d::y_sort_z;
use bevy_ts_material::{Pixel2dPlugin, YSort};

fn app() -> App {
    let mut app = App::new();
    app.add_plugins((MinimalPlugins, TransformPlugin, Pixel2dPlugin)).init_resource::<ScriptLog>();
    app
}

fn sorted(app: &mut App, layer: f32, y: f32, offset: f32) -> Entity {
    app.world_mut().spawn((YSort { layer, offset }, Transform::from_xyz(0.0, y, 0.0))).id()
}

fn z(app: &App, e: Entity) -> f32 {
    app.world().get::<Transform>(e).unwrap().translation.z
}

fn drawn_z(app: &App, e: Entity) -> f32 {
    app.world().get::<GlobalTransform>(e).unwrap().translation().z
}

fn logged(app: &App, level: LogLevel) -> Vec<String> {
    app.world().resource::<ScriptLog>().since(0).filter(|e| e.level == level).map(|e| e.message.clone()).collect()
}

#[test]
fn z_is_the_layer_less_the_base_y() {
    let mut app = app();
    let plain = sorted(&mut app, 1.0, 20.0, 0.0);
    let feet = sorted(&mut app, 1.0, 20.0, -8.0);
    let below = sorted(&mut app, 3.0, -100.0, 0.0);
    app.update();
    assert_eq!(z(&app, plain), y_sort_z(1.0, 20.0, 0.0));
    assert!((z(&app, plain) - (1.0 - 20.0e-5)).abs() < 1e-6, "{}", z(&app, plain));
    assert!((z(&app, feet) - (1.0 - 12.0e-5)).abs() < 1e-6, "the offset moves the base: {}", z(&app, feet));
    assert!((z(&app, below) - (3.0 + 100.0e-5)).abs() < 1e-6, "{}", z(&app, below));
    assert!(z(&app, feet) > z(&app, plain), "a lower base draws in front");
    // The drawn z is Bevy's propagation of it, the same frame.
    assert_eq!(drawn_z(&app, plain), z(&app, plain));
}

#[test]
fn past_the_clamp_sprites_tie_with_one_warning_and_layers_never_mix() {
    let mut app = app();
    let top = sorted(&mut app, 1.0, 60_000.0, 0.0);
    let one_low = sorted(&mut app, 1.0, -49_999.0, 0.0);
    let two_high = sorted(&mut app, 2.0, 49_999.0, 0.0);
    for _ in 0..3 {
        app.update();
    }
    assert_eq!(z(&app, top), y_sort_z(1.0, 49_999.0, 0.0), "clamped to 49999");
    let warnings = logged(&app, LogLevel::Warn);
    assert_eq!(warnings.len(), 1, "{warnings:?}");
    assert!(warnings[0].contains(&format!("entity {top}")) && warnings[0].contains("60000"), "{}", warnings[0]);
    assert!(z(&app, two_high) > z(&app, one_low), "layer 2 at its top is in front of layer 1 at its bottom: {} vs {}", z(&app, two_high), z(&app, one_low));
}

#[test]
fn one_pixel_changes_z_at_layer_127_and_layer_200_fails() {
    let mut app = app();
    let a = sorted(&mut app, 127.0, 0.0, 0.0);
    let b = sorted(&mut app, 127.0, 1.0, 0.0);
    let bad = app.world_mut().spawn((YSort { layer: 200.0, offset: 0.0 }, Transform::from_xyz(0.0, 5.0, 4.0), Name::new("tree"))).id();
    app.update();
    app.update();
    assert!(z(&app, a) > z(&app, b), "{} vs {}", z(&app, a), z(&app, b));
    assert_eq!(z(&app, bad), 4.0, "an out-of-range layer leaves z alone");
    let errors = logged(&app, LogLevel::Error);
    assert_eq!(errors.len(), 1, "{errors:?}");
    assert!(errors[0].contains("YSort.layer 200") && errors[0].contains("\"tree\""), "{}", errors[0]);
}

#[test]
fn a_shadow_child_draws_just_behind_its_parent() {
    let mut app = app();
    let hero = sorted(&mut app, 1.0, 10.0, 0.0);
    let shadow = app.world_mut().spawn((Transform::from_xyz(0.0, 0.0, -1e-5), ChildOf(hero))).id();
    let two_up = sorted(&mut app, 1.0, 12.0, 0.0);
    app.update();
    assert!(drawn_z(&app, shadow) < drawn_z(&app, hero), "behind its parent");
    assert!(drawn_z(&app, shadow) > drawn_z(&app, two_up), "in front of what stands two pixels up");
    assert_eq!(z(&app, shadow), -1e-5, "a child without YSort keeps its own z");
}

#[test]
fn ysort_owns_z_and_leaves_a_still_entity_unchanged() {
    let mut app = app();
    let e = sorted(&mut app, 1.0, 30.0, 0.0);
    app.update();
    let want = z(&app, e);
    app.world_mut().get_mut::<Transform>(e).unwrap().translation.z = 5.0;
    app.update();
    assert_eq!(z(&app, e), want, "a script's z is overwritten the same frame");
    let changed = app.world().entity(e).get_ref::<Transform>().unwrap().last_changed();
    for _ in 0..100 {
        app.update();
        assert_eq!(drawn_z(&app, e), want);
    }
    assert_eq!(app.world().entity(e).get_ref::<Transform>().unwrap().last_changed(), changed, "a still entity's Transform is not written");
}

#[test]
fn a_moved_entitys_global_transform_is_bevys_propagation() {
    let mut app = app();
    let e = sorted(&mut app, 2.0, 0.0, -4.0);
    app.update();
    for y in [5.0, -12.5, 300.0] {
        app.world_mut().get_mut::<Transform>(e).unwrap().translation.y = y;
        app.update();
        let t = *app.world().get::<Transform>(e).unwrap();
        assert_eq!(*app.world().get::<GlobalTransform>(e).unwrap(), GlobalTransform::from(t), "y {y}");
        assert_eq!(t.translation.z, y_sort_z(2.0, y, -4.0));
    }
}
