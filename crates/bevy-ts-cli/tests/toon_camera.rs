//! The toon camera from TSX: an orthographic `Projection`, both prepass markers and
//! `Tonemapping::None`, with no Rust in the game (script-materials, "An orthographic prepass
//! camera from TSX").

mod common;

use bevy::camera::ScalingMode;
use bevy::core_pipeline::prepass::{DepthPrepass, NormalPrepass};
use bevy::core_pipeline::tonemapping::Tonemapping;
use bevy::prelude::*;
use bevy_ts::generation::ReloadStatus;
use bevy_ts::log::LogLevel;
use common::*;

const GAME: &str = r#"import { app } from "bevy-ts";
import { Camera3d, DepthPrepass, NormalPrepass, Projection, Tonemapping, Transform } from "bevy-ts/bevy";

app.scene("s", () => (
  <entity name="camera">
    <Camera3d />
    <Projection value={{ Orthographic: { scaling_mode: { FixedVertical: { viewport_height: 13 } } } }} />
    <DepthPrepass />
    <NormalPrepass />
    <Tonemapping value="None" />
    <Transform translation={[0, 20, 14]} />
  </entity>
));
"#;

fn camera_app(name: &str, src: &str) -> (std::path::PathBuf, App) {
    let dir = project(name, &[("game.tsx", src)]);
    let mut app = app(&dir.join("game.tsx"));
    app.add_plugins(AssetPlugin::default())
        .register_type::<Camera3d>()
        .register_type::<Projection>()
        .register_type::<DepthPrepass>()
        .register_type::<NormalPrepass>()
        .register_type::<Tonemapping>();
    (dir, app)
}

#[test]
fn a_variant_the_type_lacks_is_reported_at_the_element() {
    let src = GAME.replace("value={{ Orthographic: { scaling_mode: { FixedVertical: { viewport_height: 13 } } } }}", "value=\"Isometric\"");
    let (_dir, mut app) = camera_app("toon-camera-bad", &src);
    reload(&mut app, false);
    app.update();
    let m = messages(&app);
    assert!(m.iter().any(|m| m.contains("bevy_camera::projection::Projection has no variant Isometric") && m.contains("Orthographic")), "{m:?}");
}

#[test]
fn a_tsx_camera_is_orthographic_with_prepasses_and_no_tonemapping() {
    let (dir, mut app) = camera_app("toon-camera", GAME);
    let r = reload(&mut app, false);
    assert_eq!(r.status, ReloadStatus::Applied, "{:?}", r.errors);
    app.update();
    let errors: Vec<_> = logs(&app).into_iter().filter(|e| matches!(e.level, LogLevel::Warn | LogLevel::Error)).map(|e| e.message).collect();
    assert!(errors.is_empty(), "{errors:?}");
    let (cam, height) = camera(&mut app);
    assert_eq!(height, 13.0);

    // A changed prop writes the same camera in place, still orthographic.
    write(&dir, "game.tsx", &GAME.replace("viewport_height: 13", "viewport_height: 20"));
    let r = reload(&mut app, false);
    assert_eq!(r.status, ReloadStatus::Applied, "{:?}", r.errors);
    app.update();
    assert_eq!(camera(&mut app), (cam, 20.0));
}

/// The one camera, checked for both prepasses, no tonemapping and an orthographic projection
/// that keeps `from_world`'s other fields; with its height.
fn camera(app: &mut App) -> (Entity, f32) {
    let world = app.world_mut();
    let (e, projection, tonemapping) = world
        .query_filtered::<(Entity, &Projection, &Tonemapping), (With<Camera3d>, With<DepthPrepass>, With<NormalPrepass>)>()
        .single(world)
        .expect("one camera with both prepasses");
    assert_eq!(*tonemapping, Tonemapping::None);
    let Projection::Orthographic(o) = projection else { panic!("not orthographic: {projection:?}") };
    assert_eq!((o.near, o.far), (0.0, 1000.0), "the unset fields keep OrthographicProjection::from_world's");
    let ScalingMode::FixedVertical { viewport_height } = o.scaling_mode else { panic!("{:?}", o.scaling_mode) };
    (e, viewport_height)
}
