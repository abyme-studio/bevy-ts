//! Headless runs: no window and no winit. The loop is driven by `ScheduleRunnerPlugin`, and
//! every camera renders into one offscreen image, which `BEVY_TS_SCREENSHOT` saves.

use bevy::app::ScheduleRunnerPlugin;
use bevy::camera::RenderTarget;
use bevy::prelude::*;
use bevy::render::render_resource::TextureFormat;
use bevy::render::view::screenshot::{Screenshot, save_to_disk};
use bevy::window::ExitCondition;
use bevy::winit::WinitPlugin;
use std::time::Duration;

pub const SIZE: (u32, u32) = (960, 540);

/// The image every camera renders into when headless.
#[derive(Resource, Clone)]
pub struct Offscreen(pub Handle<Image>);

/// Bevy's default plugins, windowed or headless.
pub fn default_plugins(headless: bool) -> bevy::app::PluginGroupBuilder {
    if headless {
        DefaultPlugins
            .build()
            .disable::<WinitPlugin>()
            .set(WindowPlugin { primary_window: None, exit_condition: ExitCondition::DontExit, ..default() })
    } else {
        DefaultPlugins.build()
    }
}

pub struct HeadlessPlugin;

impl Plugin for HeadlessPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(ScheduleRunnerPlugin::run_loop(Duration::from_secs_f64(1.0 / 60.0)));
        let mut image = Image::new_target_texture(SIZE.0, SIZE.1, TextureFormat::Rgba8UnormSrgb, None);
        image.asset_usage = bevy::asset::RenderAssetUsages::default();
        let handle = app.world_mut().resource_mut::<Assets<Image>>().add(image);
        app.insert_resource(Offscreen(handle));
        app.add_observer(|add: On<Add<Camera>>, offscreen: Res<Offscreen>, mut commands: Commands| {
            commands.entity(add.entity).insert(RenderTarget::Image(offscreen.0.clone().into()));
        });
    }
}

/// `BEVY_TS_SCREENSHOT=<path>` saves the view at frames 60 and 75 (`<path>` and
/// `<path>.2.png`), so a run can be checked without a window.
pub fn screenshot_on_request(mut commands: Commands, offscreen: Option<Res<Offscreen>>, mut frame: Local<u32>) {
    *frame += 1;
    let Ok(path) = std::env::var("BEVY_TS_SCREENSHOT") else { return };
    let path = match *frame {
        60 => path,
        75 => format!("{path}.2.png"),
        _ => return,
    };
    let shot = match offscreen {
        Some(o) => Screenshot::image(o.0.clone()),
        None => Screenshot::primary_window(),
    };
    commands.spawn(shot).observe(save_to_disk(path));
}
