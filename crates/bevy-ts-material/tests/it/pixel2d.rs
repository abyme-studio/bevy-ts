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

/// Entities whose `Mesh2d` changed this frame, seen in `Last`.
#[derive(Resource, Default)]
struct Requeued(Vec<Entity>);

#[test]
fn a_moved_z_marks_the_mesh_and_its_childs_changed_and_a_still_one_does_not() {
    let mut app = app();
    app.init_resource::<Requeued>().add_systems(Last, |q: Query<Entity, Changed<Mesh2d>>, mut r: ResMut<Requeued>| r.0 = q.iter().collect());
    let walker = app.world_mut().spawn((YSort { layer: 1.0, offset: 0.0 }, Transform::from_xyz(0.0, 10.0, 0.0), Mesh2d::default())).id();
    let shadow = app.world_mut().spawn((Transform::from_xyz(0.0, -8.0, -1e-5), Mesh2d::default(), ChildOf(walker))).id();
    let still = app.world_mut().spawn((YSort { layer: 1.0, offset: 0.0 }, Transform::from_xyz(50.0, 0.0, 0.0), Mesh2d::default())).id();
    app.update();
    app.update();
    assert!(app.world().resource::<Requeued>().0.is_empty(), "nothing moved: {:?}", app.world().resource::<Requeued>().0);
    app.world_mut().get_mut::<Transform>(walker).unwrap().translation.y = -10.0;
    app.update();
    let mut got = app.world().resource::<Requeued>().0.clone();
    got.sort();
    let mut want = vec![walker, shadow];
    want.sort();
    assert_eq!(got, want, "the walker and its shadow, not {still}");
    // x alone keeps z: not queued again.
    app.world_mut().get_mut::<Transform>(walker).unwrap().translation.x = 5.0;
    app.update();
    assert!(app.world().resource::<Requeued>().0.is_empty());
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

fn camera(app: &mut App, scale: f32) -> Entity {
    let projection = Projection::Orthographic(OrthographicProjection { scale, ..OrthographicProjection::default_2d() });
    app.world_mut().spawn((Camera2d, projection)).id()
}

fn scale(app: &App, e: Entity) -> Vec3 {
    app.world().get::<Transform>(e).unwrap().scale
}

#[test]
fn screen_size_scales_by_the_ruling_cameras_scale_and_owns_it() {
    let mut app = app();
    let cam = camera(&mut app, 0.25);
    let marker = app.world_mut().spawn(bevy_ts_material::ScreenSize::default()).id();
    let double = app.world_mut().spawn(bevy_ts_material::ScreenSize { scale: 2.0 }).id();
    app.update();
    assert_eq!(scale(&app, marker), Vec3::new(0.25, 0.25, 1.0), "at zoom 4 a texel is a quarter of a world unit");
    assert_eq!(scale(&app, double), Vec3::new(0.5, 0.5, 1.0));
    app.world_mut().get_mut::<Transform>(marker).unwrap().scale = Vec3::splat(3.0);
    app.update();
    assert_eq!(scale(&app, marker), Vec3::new(0.25, 0.25, 3.0), "a script's x and y are overwritten; z is its own");
    let Projection::Orthographic(o) = &mut *app.world_mut().get_mut::<Projection>(cam).unwrap() else { unreachable!() };
    o.scale = 1.0;
    app.update();
    assert_eq!(scale(&app, double).truncate(), Vec2::splat(2.0), "follows the camera's zoom");
    // A camera of a later order does not rule.
    app.world_mut().spawn((Camera2d, Camera { order: 1, ..default() }, Projection::Orthographic(OrthographicProjection { scale: 0.5, ..OrthographicProjection::default_2d() })));
    app.update();
    assert_eq!(scale(&app, double).truncate(), Vec2::splat(2.0));
}

// Stage 3: the pixel camera.

use bevy::camera::RenderTargetInfo;
use bevy::time::TimeUpdateStrategy;
use bevy_ts_material::PixelCamera;
use bevy_ts_material::pixel2d::{PixelCameraState, drawn_from};
use std::time::Duration;

/// A `Camera2d` with `pixel`, its target `width` × 540 logical pixels, at `x`; time steps 50 ms.
fn pixel_camera(app: &mut App, pixel: PixelCamera, width: u32, x: f32) -> Entity {
    app.insert_resource(TimeUpdateStrategy::ManualDuration(Duration::from_millis(50)));
    let mut camera = Camera::default();
    camera.computed.target_info = Some(RenderTargetInfo { physical_size: UVec2::new(width, 540), scale_factor: 1.0 });
    app.world_mut().spawn((Camera2d, camera, pixel, Transform::from_xyz(x, 0.0, 0.0))).id()
}

fn ortho(app: &App, e: Entity) -> OrthographicProjection {
    match app.world().get::<Projection>(e).unwrap() {
        Projection::Orthographic(o) => o.clone(),
        p => panic!("{p:?}"),
    }
}

/// The x the view is centred on, as Bevy draws it: its area spans `scale × -origin` to
/// `scale × (width - origin)` about the camera, `origin` being `width × viewport_origin.x`
/// (`bevy_camera-0.20.0/src/projection.rs:729-735`). Checked against [`drawn_from`].
fn view_x(app: &App, e: Entity) -> f32 {
    let w = app.world();
    let (camera, at) = (w.get::<Camera>(e).unwrap(), w.get::<GlobalTransform>(e).unwrap());
    let o = ortho(app, e);
    let width = camera.logical_viewport_size().unwrap().x;
    let centre = at.translation().x + o.scale * width * (0.5 - o.viewport_origin.x);
    let drawn = drawn_from(camera, at, w.get::<Projection>(e).unwrap(), w.get::<PixelCameraState>(e).unwrap()).unwrap().x;
    assert!((centre - drawn).abs() < 1e-4, "drawn_from {drawn}, Bevy's view {centre}");
    centre
}

#[test]
fn a_pixel_camera_zooms_by_whole_numbers_and_eases_between_them() {
    let mut app = app();
    let cam = pixel_camera(&mut app, PixelCamera { zoom: 2.0, ease: 0.25 }, 960, 0.0);
    app.update();
    assert_eq!(ortho(&app, cam).scale, 0.5, "zoom 2 is scale 1/2 from the first frame");
    assert_eq!(app.world().get::<Msaa>(cam), Some(&Msaa::Off), "multisampling off");
    app.world_mut().get_mut::<PixelCamera>(cam).unwrap().zoom = 4.0;
    app.update();
    app.update();
    // 0.1 s into the ease (the frame that saw the change, then one more).
    let mid = ortho(&app, cam).scale;
    assert!(mid < 0.5 && mid > 0.25, "strictly between: {mid}");
    for _ in 0..5 {
        app.update();
    }
    assert_eq!(ortho(&app, cam).scale, 0.25, "exact from 0.25 s on");
    // It owns the scale.
    let Projection::Orthographic(o) = &mut *app.world_mut().get_mut::<Projection>(cam).unwrap() else { unreachable!() };
    o.scale = 3.0;
    app.update();
    assert_eq!(ortho(&app, cam).scale, 0.25, "a script's scale is overwritten");
    // ease 0 jumps.
    let mut p = app.world_mut().get_mut::<PixelCamera>(cam).unwrap();
    p.ease = 0.0;
    p.zoom = 1.0;
    app.update();
    assert_eq!(ortho(&app, cam).scale, 1.0);
}

#[test]
fn a_zoom_that_is_not_whole_logs_one_error_and_changes_nothing() {
    let mut app = app();
    let cam = pixel_camera(&mut app, PixelCamera { zoom: 2.5, ease: 0.0 }, 960, 0.0);
    app.update();
    app.update();
    assert_eq!(ortho(&app, cam).scale, 1.0);
    let errors = logged(&app, LogLevel::Error);
    assert_eq!(errors.len(), 1, "{errors:?}");
    assert!(errors[0].contains("PixelCamera.zoom 2.5"), "{}", errors[0]);
}

#[test]
fn the_view_is_drawn_from_the_camera_rounded_to_whole_screen_pixels() {
    let mut app = app();
    let even = pixel_camera(&mut app, PixelCamera { zoom: 4.0, ease: 0.0 }, 960, 10.3);
    app.update();
    app.update();
    assert!((view_x(&app, even) - 10.25).abs() < 1e-4, "{}", view_x(&app, even));
    assert_eq!(app.world().get::<Transform>(even).unwrap().translation.x, 10.3, "Transform keeps the script's x");
    assert_eq!(app.world().get::<GlobalTransform>(even).unwrap().translation().x, 10.3, "and GlobalTransform is Bevy's");
    // On an odd width the centre falls between pixels: half a step more.
    app.world_mut().get_mut::<Camera>(even).unwrap().computed.target_info = Some(RenderTargetInfo { physical_size: UVec2::new(961, 540), scale_factor: 1.0 });
    app.update();
    assert!((view_x(&app, even) - 10.375).abs() < 1e-4, "{}", view_x(&app, even));
}

#[test]
fn a_second_pixel_camera_is_named_once_and_the_first_rules() {
    let mut app = app();
    let first = pixel_camera(&mut app, PixelCamera { zoom: 2.0, ease: 0.0 }, 960, 0.0);
    app.update();
    let second = pixel_camera(&mut app, PixelCamera { zoom: 4.0, ease: 0.0 }, 960, 0.0);
    for _ in 0..3 {
        app.update();
    }
    let warnings = logged(&app, LogLevel::Warn);
    assert_eq!(warnings.len(), 1, "{warnings:?}");
    assert!(warnings[0].contains(&format!("entity {first}")) && warnings[0].contains(&format!("entity {second}")), "{}", warnings[0]);
    assert_eq!(ortho(&app, first).scale, 0.5);
    assert_eq!(ortho(&app, second).scale, 1.0, "the second zooms nothing");
}

#[test]
fn screen_size_follows_the_pixel_camera() {
    let mut app = app();
    camera(&mut app, 1.0);
    pixel_camera(&mut app, PixelCamera { zoom: 4.0, ease: 0.0 }, 960, 0.0);
    let marker = app.world_mut().spawn(bevy_ts_material::ScreenSize::default()).id();
    app.update();
    assert_eq!(scale(&app, marker).truncate(), Vec2::splat(0.25), "the pixel camera rules, and its scale is this frame's");
}

// Stage 6: sprite clips.

use bevy_ts_material::{ClipMode, SpriteClip};

/// An app on 50 ms steps of virtual time with a sprite on a 4-tile grid playing `clip`.
fn clip_app(clip: SpriteClip) -> (App, Entity) {
    let mut app = app();
    app.add_plugins(AssetPlugin::default()).init_asset::<TextureAtlasLayout>().insert_resource(TimeUpdateStrategy::ManualDuration(Duration::from_millis(50)));
    let layout = app.world_mut().resource_mut::<Assets<TextureAtlasLayout>>().add(TextureAtlasLayout::from_grid(UVec2::splat(16), 4, 1, None, None));
    let sprite = Sprite { texture_atlas: Some(TextureAtlas { layout, index: 0 }), ..default() };
    let e = app.world_mut().spawn((sprite, clip)).id();
    (app, e)
}

/// Updates until `secs` of virtual time have passed.
fn run_for(app: &mut App, secs: f32) {
    let until = app.world().resource::<Time<Virtual>>().elapsed() + Duration::from_millis((secs * 1000.0).round() as u64);
    while app.world().resource::<Time<Virtual>>().elapsed() < until {
        app.update();
    }
}

fn index(app: &App, e: Entity) -> usize {
    app.world().get::<Sprite>(e).unwrap().texture_atlas.as_ref().unwrap().index
}

const WALK: SpriteClip = SpriteClip { first: 0, last: 3, fps: 10.0, mode: ClipMode::Repeat, playing: true, finished: false };

#[test]
fn a_clip_repeats_holds_once_and_ping_pongs() {
    // The first update starts the clock; 0.45 s after it.
    // Frames 0, 1, 2, 3, then 0; 0, 1, 2, 3, 2; and Once holds 3, shown its full time by 0.4 s.
    for (mode, want, finished) in [(ClipMode::Repeat, 0, false), (ClipMode::PingPong, 2, false), (ClipMode::Once, 3, true)] {
        let (mut app, e) = clip_app(SpriteClip { mode, ..WALK });
        app.update();
        run_for(&mut app, 0.45);
        assert_eq!(index(&app, e), want, "{mode:?} at 0.45 s");
        assert_eq!(app.world().get::<SpriteClip>(e).unwrap().finished, finished, "{mode:?}");
    }
    let (mut app, e) = clip_app(SpriteClip { mode: ClipMode::Once, ..WALK });
    app.update();
    run_for(&mut app, 0.35);
    assert_eq!(index(&app, e), 3);
    assert!(!app.world().get::<SpriteClip>(e).unwrap().finished, "on its last frame, not yet finished");
    run_for(&mut app, 0.65);
    assert_eq!(index(&app, e), 3, "held at 1 s");
    assert!(app.world().get::<SpriteClip>(e).unwrap().finished);
}

#[test]
fn paused_time_or_playing_false_holds_the_frame_and_a_new_range_restarts() {
    let (mut app, e) = clip_app(WALK);
    app.update();
    run_for(&mut app, 0.15);
    let held = index(&app, e);
    assert_eq!(held, 1);
    app.world_mut().resource_mut::<Time<Virtual>>().pause();
    for _ in 0..10 {
        app.update();
    }
    assert_eq!(index(&app, e), held, "paused virtual time holds");
    app.world_mut().resource_mut::<Time<Virtual>>().unpause();
    app.world_mut().get_mut::<SpriteClip>(e).unwrap().playing = false;
    for _ in 0..10 {
        app.update();
    }
    assert_eq!(index(&app, e), held, "playing false holds");
    let mut clip = app.world_mut().get_mut::<SpriteClip>(e).unwrap();
    clip.playing = true;
    clip.first = 2;
    app.update();
    assert_eq!(index(&app, e), 2, "a change of first starts again there");
}

#[test]
fn a_clip_past_its_tiles_or_upside_down_names_what_is_wrong() {
    for (clip, want) in [
        (SpriteClip { last: 4, ..WALK }, "SpriteClip.last 4 is past the layout's 4 tiles"),
        (SpriteClip { first: 3, last: 1, ..WALK }, "SpriteClip.last 1 is below SpriteClip.first 3"),
        (SpriteClip { fps: 0.0, ..WALK }, "SpriteClip.fps 0 is not above 0"),
    ] {
        let (mut app, e) = clip_app(clip);
        for _ in 0..3 {
            app.update();
        }
        let errors = logged(&app, LogLevel::Error);
        assert_eq!(errors.len(), 1, "{errors:?}");
        assert!(errors[0].contains(want), "{}", errors[0]);
        assert_eq!(index(&app, e), 0, "the frame is left alone");
    }
    let mut app = app();
    app.add_plugins(AssetPlugin::default()).init_asset::<TextureAtlasLayout>();
    app.world_mut().spawn((Sprite::default(), WALK));
    app.update();
    assert!(logged(&app, LogLevel::Error)[0].contains("no texture_atlas"));
}
