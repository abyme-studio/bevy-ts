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

/// A project's asset root: `assets/` beside its `package.json`, `tsconfig.json` or `bevy-ts.json`.
pub fn asset_root(project_root: &std::path::Path) -> std::path::PathBuf {
    project_root.join("assets")
}

/// Bevy's `AssetPlugin` reading from `root` (an absolute path, so the binary's location and the
/// working directory do not matter), watched so a level saved by anyone reloads (levels decision 3).
pub fn asset_plugin(root: &std::path::Path) -> AssetPlugin {
    asset_plugin_with(root, true)
}

/// As [`asset_plugin`], with Bevy's file watcher off: a shipped game's assets do not change.
pub fn asset_plugin_unwatched(root: &std::path::Path) -> AssetPlugin {
    asset_plugin_with(root, false)
}

fn asset_plugin_with(root: &std::path::Path, watch: bool) -> AssetPlugin {
    // Bevy's file watcher strips the root from the paths it reports, which are canonical: a
    // symlinked root (macOS `/var` is `/private/var`) would panic its thread.
    let root = root.canonicalize().unwrap_or_else(|_| root.to_path_buf());
    let root = root.as_path();
    AssetPlugin { file_path: root.to_string_lossy().into_owned(), watch_for_changes_override: Some(watch), ..default() }
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

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::asset::io::AssetSourceId;

    #[test]
    fn dev_reads_assets_from_the_project() {
        let project = std::env::temp_dir().join("bevy-ts-assets-root");
        let _ = std::fs::remove_dir_all(&project);
        std::fs::create_dir_all(project.join("src")).unwrap();
        std::fs::create_dir_all(project.join("assets/textures")).unwrap();
        std::fs::write(project.join("package.json"), "{}").unwrap();
        std::fs::write(project.join("src/game.ts"), "").unwrap();
        std::fs::write(project.join("assets/textures/hello.txt"), "hi").unwrap();
        // The root `bevy-ts dev` hands Bevy: `assets/` beside the project's manifest.
        let compiler = crate::compile::Compiler::new(project.join("src/game.ts"), true).unwrap();
        let root = asset_root(&compiler.root);
        assert_eq!(root, project.canonicalize().unwrap().join("assets"));

        let mut app = App::new();
        app.add_plugins((MinimalPlugins, asset_plugin(&root)));
        let server = app.world().resource::<AssetServer>().clone();
        let source = server.get_source(AssetSourceId::Default).unwrap();
        let bytes = bevy::tasks::block_on(async {
            let mut reader = source.reader().read(std::path::Path::new("textures/hello.txt")).await.unwrap();
            let mut out = Vec::new();
            reader.read_to_end(&mut out).await.unwrap();
            out
        });
        assert_eq!(bytes, b"hi");
    }
}
