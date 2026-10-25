//! The tests' app: the engine's plugins with `PlayAnimation`, headless, no window and no GPU,
//! time stepped by a fixed 1/60 s, with Bevy's `Fox.glb`.
#![allow(dead_code)]

use bevy::animation::graph::{AnimationGraph, AnimationNodeIndex};
use bevy::animation::AnimationPlayer;
use bevy::prelude::*;
use bevy::render::RenderPlugin;
use bevy::render::settings::{RenderCreation, WgpuSettings};
use bevy::time::TimeUpdateStrategy;
use bevy::window::ExitCondition;
use bevy::winit::WinitPlugin;
use bevy::world_serialization::WorldAssetRoot;
use bevy_ts_animation::{AnimationTargets, PlayAnimation, PlayAnimationPlugin, SharedGraph};
use std::path::PathBuf;
use std::time::Duration;

pub const DT: f32 = 1.0 / 60.0;
pub const SCENE: &str = "models/Fox.glb#Scene0";
pub const SURVEY: &str = "models/Fox.glb#Animation0";
pub const WALK: &str = "models/Fox.glb#Animation1";
pub const RUN: &str = "models/Fox.glb#Animation2";

/// An asset root holding `Fox.glb`, `still.gltf` (a scene with no animation) and the graph
/// fixtures under `tests/assets`.
pub fn assets() -> PathBuf {
    let dir = std::env::temp_dir().join(format!("bevy-ts-animation-{}", std::process::id()));
    std::fs::create_dir_all(dir.join("models")).unwrap();
    let fox = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../examples/foxes/assets/models/Fox.glb");
    std::fs::copy(fox, dir.join("models/Fox.glb")).unwrap();
    std::fs::create_dir_all(dir.join("anims")).unwrap();
    let fixtures = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/assets");
    std::fs::copy(fixtures.join("anims/fox_blend.animgraph.ron"), dir.join("anims/fox_blend.animgraph.ron")).unwrap();
    std::fs::write(
        dir.join("models/still.gltf"),
        r#"{"asset":{"version":"2.0"},"scene":0,"scenes":[{"nodes":[0]}],"nodes":[{"name":"still"}]}"#,
    )
    .unwrap();
    dir.canonicalize().unwrap()
}

/// The engine's plugins with `PlayAnimation`, and nothing loaded.
pub fn bare_app() -> App {
    bare_app_with(|_| {})
}

/// [`bare_app`] with more plugins, added before `finish`.
pub fn bare_app_with(more: impl FnOnce(&mut App)) -> App {
    let mut app = App::new();
    let render = RenderPlugin { render_creation: RenderCreation::Automatic(Box::new(WgpuSettings { backends: None, ..default() })), ..default() };
    app.add_plugins(
        DefaultPlugins
            .build()
            .disable::<WinitPlugin>()
            .disable::<bevy::log::LogPlugin>()
            .set(WindowPlugin { primary_window: None, exit_condition: ExitCondition::DontExit, ..default() })
            .set(render)
            .set(AssetPlugin { file_path: assets().to_string_lossy().into_owned(), watch_for_changes_override: Some(false), ..default() }),
    )
    .add_plugins(PlayAnimationPlugin)
    .insert_resource(TimeUpdateStrategy::ManualDuration(Duration::from_secs_f32(DT)));
    more(&mut app);
    // The glTF loader registers in `finish`, which `update` alone never runs.
    app.finish();
    app.cleanup();
    app
}

pub fn app() -> App {
    let mut app = bare_app();
    // The file loaded first, as `many_foxes` does: Bevy 0.20 loads a file again for each label
    // requested before it is alive (bevy#10549), and a second load spawns the scene again.
    let fox: Handle<bevy::gltf::Gltf> = load(&app, "models/Fox.glb");
    until(&mut app, "Fox.glb", |w| w.resource::<AssetServer>().is_loaded_with_dependencies(&fox));
    app.insert_resource(Preloaded(fox));
    app
}

#[derive(Resource)]
pub struct Preloaded(#[allow(dead_code)] pub Handle<bevy::gltf::Gltf>);

pub fn load<A: Asset>(app: &App, path: &str) -> Handle<A> {
    app.world().resource::<AssetServer>().load(path.to_string())
}

pub fn fox(app: &mut App, clip: &str) -> Entity {
    let scene = load(app, SCENE);
    let clip = load(app, clip);
    app.world_mut().spawn((WorldAssetRoot(scene), PlayAnimation { clip, ..default() })).id()
}

/// Updates until `done`, or fails after 30 s of loading.
pub fn until(app: &mut App, what: &str, done: impl Fn(&mut World) -> bool) {
    let start = std::time::Instant::now();
    while !done(app.world_mut()) {
        // Asset loads run on other threads, which a loaded machine slows by any amount.
        assert!(start.elapsed() < Duration::from_secs(120), "timed out waiting for {what}");
        app.update();
        std::thread::sleep(Duration::from_millis(2));
    }
}

pub fn targets(world: &World, root: Entity) -> Vec<Entity> {
    world.get::<AnimationTargets>(root).map(|t| t.players.clone()).unwrap_or_default()
}

pub fn ready(app: &mut App, root: Entity) {
    until(app, "the fox to play", |w| !targets(w, root).is_empty());
}

pub fn node(app: &mut App, clip: &str) -> AnimationNodeIndex {
    let clip: Handle<AnimationClip> = load(app, clip);
    app.world_mut().resource_scope(|world, mut shared: Mut<SharedGraph>| {
        shared.node(&mut world.resource_mut::<Assets<AnimationGraph>>(), &clip).unwrap()
    })
}

pub fn player(app: &App, root: Entity) -> &AnimationPlayer {
    let p = targets(app.world(), root)[0];
    app.world().get::<AnimationPlayer>(p).unwrap()
}

