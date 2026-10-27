//! Headless runs: no window and no winit. The loop is driven by `ScheduleRunnerPlugin`, and
//! every camera renders into one offscreen image, the target of every screenshot
//! (`bevy-ts/screenshot`, `BEVY_TS_SCREENSHOT`). UI draws into that image too: with no window,
//! Bevy finds no default UI camera, so headless names one.

use bevy::app::ScheduleRunnerPlugin;
use bevy::camera::RenderTarget;
use bevy::prelude::*;
use bevy::render::render_resource::TextureFormat;
use bevy::ui::{IsDefaultUiCamera, UiSystems};
use bevy::window::ExitCondition;
use bevy_ts_remote::screenshot::ScreenshotTarget;
use bevy::winit::WinitPlugin;
use std::time::Duration;

/// The offscreen image's size unless [`HeadlessSize`] sets another.
pub const SIZE: (u32, u32) = (960, 540);

/// The size the offscreen image is made at: read once by [`HeadlessPlugin`] when it is built, so
/// insert it before the plugin. Unset, [`SIZE`]. Changing it afterwards does nothing: the image
/// owns the size, and whatever wants it reads the [`Offscreen`] image.
#[derive(Resource, Clone, Copy, Debug, PartialEq, Eq)]
pub struct HeadlessSize(pub UVec2);

impl Default for HeadlessSize {
    fn default() -> Self {
        HeadlessSize(UVec2::new(SIZE.0, SIZE.1))
    }
}

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

/// Registers each extra asset source (`name://path`, `bevy-ts.json`'s `assetSources`) on its
/// folder, an absolute path: Bevy's file reader and, where the folder exists, its file watcher,
/// which runs when the default root's does. No writer, so nothing writes into a source's folder.
/// Before `AssetPlugin` builds, which drops a source registered later.
pub fn register_sources(app: &mut App, sources: &[(String, std::path::PathBuf)]) {
    use bevy::asset::io::{AssetSource, AssetSourceBuilder};
    for (name, folder) in sources {
        let path = folder.to_string_lossy().into_owned();
        let builder = AssetSourceBuilder::new(AssetSource::get_default_reader(path.clone())).with_watcher(AssetSource::get_default_watcher(path, Duration::from_millis(300)));
        app.register_asset_source(name.clone(), builder);
    }
}

pub struct HeadlessPlugin;

impl Plugin for HeadlessPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(ScheduleRunnerPlugin::run_loop(Duration::from_secs_f64(1.0 / 60.0)));
        let size = app.world().get_resource::<HeadlessSize>().copied().unwrap_or_default();
        app.insert_resource(size);
        let mut image = Image::new_target_texture(size.0.x, size.0.y, TextureFormat::Rgba8UnormSrgb, None);
        image.asset_usage = bevy::asset::RenderAssetUsages::default();
        let handle = app.world_mut().resource_mut::<Assets<Image>>().add(image);
        app.insert_resource(bevy_ts::WindowImage(handle.clone()));
        app.insert_resource(Offscreen(handle.clone()));
        app.insert_resource(ScreenshotTarget(RenderTarget::Image(handle.into())));
        app.add_observer(|add: On<Add<Camera>>, offscreen: Res<Offscreen>, mut commands: Commands| {
            commands.entity(add.entity).insert(RenderTarget::Image(offscreen.0.clone().into()));
        });
        app.add_systems(PostUpdate, default_ui_camera.before(UiSystems::Prepare));
    }

    /// After `RenderPlugin::finish` puts the `RenderDevice` in the main world: a size past the
    /// GPU's largest texture would panic deep in wgpu, so it stops the start here.
    fn finish(&self, app: &mut App) {
        let Some(device) = app.world().get_resource::<bevy::render::renderer::RenderDevice>() else { return };
        let size = app.world().resource::<HeadlessSize>().0;
        if let Err(e) = fits(size, device.limits().max_texture_dimension_2d) {
            eprintln!("bevy-ts: {e}");
            std::process::exit(2);
        }
    }
}

/// Whether an image of `size` fits a GPU whose largest 2D texture is `max` on a side.
pub fn fits(size: UVec2, max: u32) -> Result<(), String> {
    if size.x <= max && size.y <= max {
        return Ok(());
    }
    Err(format!("BEVY_TS_SIZE={}x{}: larger than this GPU's largest texture, {max}x{max}", size.x, size.y))
}

/// Marks the `IsDefaultUiCamera` headless placed, so a game's own is told apart from it.
#[derive(Component)]
struct HeadlessUiCamera;

/// Bevy's default UI camera is the highest-order camera on the primary window; headless has no
/// window, so it gives `IsDefaultUiCamera` to the highest-order camera on the offscreen image, by
/// the same rule. A game's own `IsDefaultUiCamera` on another camera wins, as does a root's
/// `UiTargetCamera`; one on the camera headless already chose is taken as headless's.
fn default_ui_camera(
    mut commands: Commands,
    offscreen: Res<Offscreen>,
    cameras: Query<(Entity, &Camera, &RenderTarget)>,
    games: Query<(), (With<IsDefaultUiCamera>, Without<HeadlessUiCamera>)>,
    ours: Query<Entity, With<HeadlessUiCamera>>,
) {
    let pick = if games.is_empty() {
        cameras
            .iter()
            .filter(|(_, _, t)| matches!(t, RenderTarget::Image(i) if i.handle == offscreen.0))
            .max_by_key(|(e, c, _)| (c.order, *e))
            .map(|(e, _, _)| e)
    } else {
        None
    };
    for e in &ours {
        if Some(e) != pick {
            commands.entity(e).remove::<(IsDefaultUiCamera, HeadlessUiCamera)>();
        }
    }
    if let Some(e) = pick.filter(|e| !ours.contains(*e)) {
        commands.entity(e).insert((IsDefaultUiCamera, HeadlessUiCamera));
    }
}

/// `BEVY_TS_PIPELINE_TRACE=1` prints, to stderr, each time the render world's pipeline cache
/// empties its queue: seconds since the engine started, how many pipelines it holds, and the
/// longest render frame so far. The last line of a run is when all its pipelines were ready.
pub fn pipeline_trace(app: &mut App) {
    use bevy::render::render_resource::PipelineCache;
    use bevy::render::{Render, RenderApp, RenderSystems};
    use std::time::Instant;
    if std::env::var_os("BEVY_TS_PIPELINE_TRACE").is_none() {
        return;
    }
    let start = Instant::now();
    let Some(render) = app.get_sub_app_mut(RenderApp) else { return };
    let mut last: Option<Instant> = None;
    let mut longest = Duration::ZERO;
    let mut reported = 0;
    render.add_systems(Render, (move |cache: Res<PipelineCache>| {
        let now = Instant::now();
        if let Some(last) = last {
            longest = longest.max(now - last);
        }
        last = Some(now);
        let total = cache.pipelines().count();
        if total != reported && cache.waiting_pipelines().next().is_none() {
            reported = total;
            eprintln!("pipelines ready t={:.3}s total={total} longest_frame={:.1}ms", (now - start).as_secs_f64(), longest.as_secs_f64() * 1000.0);
        }
    }).in_set(RenderSystems::Cleanup));
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::asset::io::AssetSourceId;

    #[test]
    fn a_size_fits_up_to_the_gpu_limit_on_either_side() {
        assert_eq!(fits(UVec2::new(8192, 8192), 8192), Ok(()));
        for size in [UVec2::new(8193, 540), UVec2::new(960, 8193)] {
            let e = fits(size, 8192).unwrap_err();
            assert!(e.starts_with(&format!("BEVY_TS_SIZE={}x{}:", size.x, size.y)), "{e}");
            assert!(e.contains("8192"), "{e}");
        }
    }

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
