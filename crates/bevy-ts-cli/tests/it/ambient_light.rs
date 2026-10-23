//! A script-inserted Rust component starts from its `Default`: `AmbientLight`'s colour is a
//! hole TS never writes, so it stays white and brightness lights the scene.
#![cfg(target_os = "macos")]

use crate::common;

use bevy::light::AmbientLight;
use bevy::prelude::*;
use bevy::render::view::screenshot::{Screenshot, ScreenshotCaptured};
use bevy::window::ExitCondition;
use bevy::winit::WinitPlugin;
use bevy_ts::generation::{DevSource, ReloadStatus};
use bevy_ts_cli::compile::Compiler;
use bevy_ts_cli::headless::{HeadlessPlugin, Offscreen};
use bevy_ts_v8::V8Options;
use common::{project, reload, sync_pipelines, until};
use std::sync::{Arc, Mutex};

/// A white cube and no lights but the ambient one; `insert` goes on the camera.
fn game(insert: &str) -> String {
    format!(
        r##"import {{ app, Startup, Transform }} from "bevy-ts";
import type {{ Commands }} from "bevy-ts";
import {{ AmbientLight, Camera3d }} from "bevy-ts/bevy";
export function setup(commands: Commands) {{
  const cam = commands.spawn(Camera3d, Transform.lookingAt([3, 3, 3], [0, 0, 0]));
  {insert}
  commands.spawn(Transform.of({{}}), <entity><mesh shape="cuboid" size={{[2, 2, 2]}} /><material color="#ffffff" /></entity>);
}}
app.addSystems(Startup, setup);
"##
    )
}

/// The camera's `AmbientLight`, if any, and the frame's mean brightness (0..255).
fn render(name: &str, insert: &str) -> (Option<AmbientLight>, f64) {
    let dir = project(name, &[("src/game.tsx", &game(insert)), ("package.json", "{}")]);
    let mut app = App::new();
    app.add_plugins(
        DefaultPlugins
            .build()
            .disable::<WinitPlugin>()
            .set(WindowPlugin { primary_window: None, exit_condition: ExitCondition::DontExit, ..default() })
            .set(sync_pipelines()),
    )
    .add_plugins(HeadlessPlugin)
    .add_plugins(bevy_ts::BevyTsPlugin::new(bevy_ts_v8::factory(V8Options::default())).dev(true))
    .add_plugins(bevy_ts_tsx::TsxPlugin)
    .insert_resource(DevSource(Arc::new(Compiler::new(dir.join("src/game.tsx"), true).unwrap())));
    app.finish();
    app.cleanup();
    let r = reload(&mut app, false);
    assert_eq!(r.status, ReloadStatus::Applied, "{:?}", r.errors);
    for _ in 0..30 {
        app.update();
    }
    let world = app.world_mut();
    let light = world.query_filtered::<&AmbientLight, With<Camera>>().iter(world).next().cloned();
    let shot = Arc::new(Mutex::new(None::<Image>));
    let s = shot.clone();
    let offscreen = app.world().resource::<Offscreen>().0.clone();
    app.world_mut().spawn(Screenshot::image(offscreen)).observe(move |c: On<ScreenshotCaptured>| {
        *s.lock().unwrap() = Some(c.image.clone());
    });
    // The screenshot comes back from the render thread, which a loaded machine slows by any amount.
    let mut img = None;
    until(&mut app, &format!("{name}: the screenshot never arrived"), |_| {
        img = shot.lock().unwrap().take();
        img.is_some()
    });
    let px = img.unwrap().data.expect("screenshot data");
    let rgb: u64 = px.as_chunks::<4>().0.iter().map(|p| p[0] as u64 + p[1] as u64 + p[2] as u64).sum();
    (light, rgb as f64 / (px.len() / 4 * 3) as f64)
}

#[test]
fn a_script_inserted_ambient_light_keeps_its_white_and_its_brightness_lights_the_scene() {
    let (_, global) = render("ambient-global", "");
    let insert = |b: u32| format!("commands.entity(cam).insert(AmbientLight.of({{ brightness: {b} }}));");
    let (light, at80) = render("ambient-80", &insert(80));
    let (_, at400) = render("ambient-400", &insert(400));
    let (_, at4000) = render("ambient-4000", &insert(4000));

    // The colour TS cannot write is the Default's white; the fields it gives are written.
    let light = light.expect("the camera has an AmbientLight");
    assert_eq!(light.color, Color::WHITE);
    assert_eq!(light.brightness, 80.0);
    assert!(light.affects_lightmapped_meshes);

    // The global default is white at 80, so the same per-camera light draws the same frame.
    assert!((at80 - global).abs() < 1.0, "per-camera 80 {at80} vs global 80 {global}");
    assert!(at400 > at80 + 5.0, "400 {at400} is no brighter than 80 {at80}");
    assert!(at4000 > at400 + 5.0, "4000 {at4000} is no brighter than 400 {at400}");
}
