//! `VisibilityRange` (visibility-range): stock BRP inserts and reads it with `bevy_ts::type_data`'s
//! serde for `Range<f32>`, and a TSX detail mesh and its impostor cross-fade by camera distance,
//! drawn to an offscreen image, no window shown.

use crate::common;

use bevy::camera::visibility::VisibilityRange;
use bevy::prelude::*;
use serde_json::json;

const RANGE: &str = "bevy_camera::visibility::range::VisibilityRange";

#[test]
fn brp_inserts_and_gets_visibility_range() {
    use bevy::remote::builtin_methods::{process_remote_get_components_request, process_remote_insert_components_request};
    let mut app = App::new();
    app.add_plugins(MinimalPlugins).add_plugins(bevy_ts::BevyTsPlugin::new(|| Box::new(bevy_ts::fake::FakeHandle::default().backend())));
    let world = app.world_mut();
    let e = world.spawn_empty().id();
    let margins = json!({ "start_margin": { "start": 0.0, "end": 10.0 }, "end_margin": { "start": 40.0, "end": 50.0 }, "use_aabb": false });
    let params = json!({ "entity": e, "components": { RANGE: margins } });
    world.run_system_cached_with(process_remote_insert_components_request, Some(params)).unwrap().unwrap();
    let range = world.get::<VisibilityRange>(e).expect("inserted");
    assert_eq!((range.start_margin.clone(), range.end_margin.clone()), (0.0..10.0, 40.0..50.0));
    let got = world
        .run_system_cached_with(process_remote_get_components_request, Some(json!({ "entity": e, "components": [RANGE] })))
        .unwrap()
        .unwrap();
    assert_eq!(got["components"][RANGE], margins);
}

#[cfg(target_os = "macos")]
mod hlod {
    use super::common::{project, reload};
    use bevy::camera::RenderTarget;
    use bevy::camera::visibility::ViewVisibility;
    use bevy::prelude::*;
    use bevy::render::render_resource::TextureFormat;
    use bevy::render::view::screenshot::{Screenshot, ScreenshotCaptured};
    use bevy::window::ExitCondition;
    use bevy::winit::WinitPlugin;
    use bevy_ts::generation::{DevSource, ReloadStatus};
    use bevy_ts_cli::compile::Compiler;
    use bevy_ts_v8::V8Options;
    use std::path::Path;
    use std::sync::{Arc, Mutex};

    const W: u32 = 160;
    const H: u32 = 120;

    /// A red detail cube that fades out over 8..12 m and a blue impostor at the same spot that
    /// fades in over the same margin.
    const GAME: &str = r##"import { app } from "bevy-ts";
import { Camera3d, Tonemapping, Transform, VisibilityRange } from "bevy-ts/bevy";

app.scene("s", () => (
  <>
    <entity name="cam"><Camera3d /><Tonemapping value="None" /><Transform translation={[0, 0, 4]} /></entity>
    <entity name="detail">
      <Transform /><mesh shape="cuboid" size={[1, 1, 1]} /><material color="#ff0000" unlit />
      <VisibilityRange start_margin={{ start: 0, end: 0 }} end_margin={{ start: 8, end: 12 }} />
    </entity>
    <entity name="impostor">
      <Transform /><mesh shape="cuboid" size={[1, 1, 1]} /><material color="#0000ff" unlit />
      <VisibilityRange start_margin={{ start: 8, end: 12 }} end_margin={{ start: 1000, end: 1001 }} />
    </entity>
  </>
));
"##;

    #[derive(Resource, Clone)]
    struct Target(Handle<Image>);

    fn app(entry: &Path) -> App {
        let mut app = super::common::render_app();
        app.add_plugins(
            DefaultPlugins
                .build()
                .disable::<WinitPlugin>()
                .set(WindowPlugin { primary_window: None, exit_condition: ExitCondition::DontExit, ..default() })
                .set(super::common::sync_pipelines()),
        )
        .add_plugins(bevy_ts::BevyTsPlugin::new(bevy_ts_v8::factory(V8Options::default())).dev(true))
        .add_plugins(bevy_ts_tsx::TsxPlugin)
        .insert_resource(ClearColor(Color::BLACK))
        .insert_resource(DevSource(Arc::new(Compiler::new(entry, true).unwrap())));
        while app.plugins_state() == bevy::app::PluginsState::Adding {
            bevy::tasks::tick_global_task_pools_on_main_thread();
        }
        app.finish();
        app.cleanup();
        let mut image = Image::new_target_texture(W, H, TextureFormat::Rgba8UnormSrgb, None);
        image.asset_usage = bevy::asset::RenderAssetUsages::default();
        let target = app.world_mut().resource_mut::<Assets<Image>>().add(image);
        app.insert_resource(Target(target));
        app.add_observer(|add: On<Add<Camera>>, target: Res<Target>, mut commands: Commands| {
            commands.entity(add.entity).insert(RenderTarget::Image(target.0.clone().into()));
        });
        assert_eq!(reload(&mut app, false).status, ReloadStatus::Applied);
        app
    }

    /// Red and blue pixel counts in a picture.
    fn counts(img: &Image) -> (usize, usize) {
        let d = img.data.as_ref().unwrap();
        let px = d.chunks_exact(4);
        let red = px.clone().filter(|p| p[0] > 128 && p[2] < 64).count();
        let blue = px.filter(|p| p[2] > 128 && p[0] < 64).count();
        (red, blue)
    }

    /// Draws until `done` accepts a picture's counts; panics with the last counts after many frames.
    fn picture(app: &mut App, what: &str, done: impl Fn((usize, usize)) -> bool) -> (usize, usize) {
        let target = app.world().resource::<Target>().0.clone();
        let shot: Arc<Mutex<Option<Image>>> = Arc::default();
        let mut last = None;
        for frame in 0..600 {
            app.update();
            if frame % 10 == 5 {
                let s = shot.clone();
                app.world_mut().spawn(Screenshot::image(target.clone())).observe(move |e: On<ScreenshotCaptured>| {
                    *s.lock().unwrap() = Some(e.image.clone());
                });
            }
            if let Some(img) = shot.lock().unwrap().take() {
                let c = counts(&img);
                if done(c) {
                    return c;
                }
                last = Some(c);
            }
        }
        panic!("{what}: the picture never settled; last (red, blue) pixels: {last:?}");
    }

    fn camera_at(app: &mut App, z: f32) {
        let world = app.world_mut();
        let mut q = world.query_filtered::<&mut Transform, With<Camera3d>>();
        q.single_mut(world).unwrap().translation.z = z;
    }

    fn visible(app: &mut App, name: &str) -> bool {
        let world = app.world_mut();
        let mut q = world.query::<(&Name, &ViewVisibility)>();
        q.iter(world).find(|(n, _)| n.as_str() == name).map(|(_, v)| v.get()).expect(name)
    }

    #[test]
    fn detail_and_impostor_swap_by_distance_and_cross_fade_in_the_margin() {
        let dir = project("visibility-range-hlod", &[("game.tsx", GAME)]);
        let mut app = app(&dir.join("game.tsx"));

        let (red, blue) = picture(&mut app, "4 m", |(r, b)| r > 50 && b == 0);
        assert!(red > 50 && blue == 0, "at 4 m only red: {red} red, {blue} blue");
        assert!(visible(&mut app, "detail") && !visible(&mut app, "impostor"), "at 4 m the detail is visible, the impostor not");

        camera_at(&mut app, 20.0);
        let (red, blue) = picture(&mut app, "20 m", |(r, b)| r == 0 && b > 0);
        assert!(red == 0 && blue > 0, "at 20 m only blue: {red} red, {blue} blue");
        assert!(!visible(&mut app, "detail") && visible(&mut app, "impostor"), "at 20 m the impostor is visible, the detail not");

        camera_at(&mut app, 10.0);
        let (red, blue) = picture(&mut app, "10 m", |(r, b)| r > 5 && b > 5);
        assert!(red > 5 && blue > 5, "at 10 m both fade: {red} red, {blue} blue");
    }
}
