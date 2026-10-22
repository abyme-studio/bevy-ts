//! `PlayAnimation` on Bevy's `Fox.glb`, headless: the engine's plugins with no window and no GPU,
//! time stepped by a fixed 1/60 s.

use bevy::animation::graph::{AnimationGraph, AnimationGraphHandle, AnimationNodeIndex};
use bevy::animation::{AnimationPlayer, RepeatAnimation};
use bevy::prelude::*;
use bevy::render::RenderPlugin;
use bevy::render::settings::{RenderCreation, WgpuSettings};
use bevy::time::TimeUpdateStrategy;
use bevy::window::ExitCondition;
use bevy::winit::WinitPlugin;
use bevy::world_serialization::{WorldAssetRoot, WorldInstance, WorldInstanceReady, WorldInstanceSpawner};
use bevy_ts_animation::{AnimationReports, AnimationTargets, PlayAnimation, PlayAnimationPlugin, SharedGraph};
use std::path::PathBuf;
use std::time::Duration;

const DT: f32 = 1.0 / 60.0;
const SCENE: &str = "models/Fox.glb#Scene0";
const SURVEY: &str = "models/Fox.glb#Animation0";
const WALK: &str = "models/Fox.glb#Animation1";
const RUN: &str = "models/Fox.glb#Animation2";

/// An asset root holding `Fox.glb` and `still.gltf`, a scene with no animation.
fn assets() -> PathBuf {
    let dir = std::env::temp_dir().join(format!("bevy-ts-animation-{}", std::process::id()));
    std::fs::create_dir_all(dir.join("models")).unwrap();
    let fox = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../examples/foxes/assets/models/Fox.glb");
    std::fs::copy(fox, dir.join("models/Fox.glb")).unwrap();
    std::fs::write(
        dir.join("models/still.gltf"),
        r#"{"asset":{"version":"2.0"},"scene":0,"scenes":[{"nodes":[0]}],"nodes":[{"name":"still"}]}"#,
    )
    .unwrap();
    dir.canonicalize().unwrap()
}

/// The engine's plugins with `PlayAnimation`, and nothing loaded.
fn bare_app() -> App {
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
    // The glTF loader registers in `finish`, which `update` alone never runs.
    app.finish();
    app.cleanup();
    app
}

fn app() -> App {
    let mut app = bare_app();
    // The file loaded first, as `many_foxes` does: Bevy 0.20 loads a file again for each label
    // requested before it is alive (bevy#10549), and a second load spawns the scene again.
    let fox: Handle<bevy::gltf::Gltf> = load(&app, "models/Fox.glb");
    until(&mut app, "Fox.glb", |w| w.resource::<AssetServer>().is_loaded_with_dependencies(&fox));
    app.insert_resource(Preloaded(fox));
    app
}

#[derive(Resource)]
struct Preloaded(#[allow(dead_code)] Handle<bevy::gltf::Gltf>);

fn load<A: Asset>(app: &App, path: &str) -> Handle<A> {
    app.world().resource::<AssetServer>().load(path.to_string())
}

fn fox(app: &mut App, clip: &str) -> Entity {
    let scene = load(app, SCENE);
    let clip = load(app, clip);
    app.world_mut().spawn((WorldAssetRoot(scene), PlayAnimation { clip, ..default() })).id()
}

/// Updates until `done`, or fails after 30 s of loading.
fn until(app: &mut App, what: &str, done: impl Fn(&mut World) -> bool) {
    let start = std::time::Instant::now();
    while !done(app.world_mut()) {
        assert!(start.elapsed() < Duration::from_secs(30), "timed out waiting for {what}");
        app.update();
        std::thread::sleep(Duration::from_millis(2));
    }
}

fn targets(world: &World, root: Entity) -> Vec<Entity> {
    world.get::<AnimationTargets>(root).map(|t| t.0.clone()).unwrap_or_default()
}

fn ready(app: &mut App, root: Entity) {
    until(app, "the fox to play", |w| !targets(w, root).is_empty());
}

fn node(app: &mut App, clip: &str) -> AnimationNodeIndex {
    let clip: Handle<AnimationClip> = load(app, clip);
    app.world_mut().resource_scope(|world, mut shared: Mut<SharedGraph>| {
        shared.node(&mut world.resource_mut::<Assets<AnimationGraph>>(), &clip).unwrap()
    })
}

fn player(app: &App, root: Entity) -> &AnimationPlayer {
    let p = targets(app.world(), root)[0];
    app.world().get::<AnimationPlayer>(p).unwrap()
}

#[test]
fn plays_once_the_scene_is_ready() {
    let mut app = app();
    let root = fox(&mut app, RUN);
    ready(&mut app, root);
    app.update();
    let run = node(&mut app, RUN);
    let a = player(&app, root).animation(run).expect("the run clip plays");
    assert_eq!(a.repeat_mode(), RepeatAnimation::Forever);
    assert_eq!(a.speed(), 1.0);
    let p = targets(app.world(), root)[0];
    let graph = &app.world().get::<AnimationGraphHandle>(p).unwrap().0;
    assert_eq!(graph, &app.world().resource::<SharedGraph>().handle);
}

#[test]
fn plays_when_added_after_ready() {
    let mut app = app();
    let scene = load(&app, SCENE);
    let root = app.world_mut().spawn(WorldAssetRoot(scene)).id();
    until(&mut app, "the scene", |w| {
        let Some(i) = w.get::<WorldInstance>(root) else { return false };
        w.resource::<WorldInstanceSpawner>().instance_is_ready(**i)
    });
    for _ in 0..3 {
        app.update();
    }
    let clip = load(&app, WALK);
    app.world_mut().entity_mut(root).insert(PlayAnimation { clip, ..default() });
    app.update();
    let walk = node(&mut app, WALK);
    assert!(player(&app, root).is_playing_animation(walk), "the walk starts in the frame the insert is applied");
}

#[test]
fn a_new_clip_cross_fades() {
    let mut app = app();
    let root = fox(&mut app, RUN);
    ready(&mut app, root);
    app.update();
    let (run, walk) = (node(&mut app, RUN), node(&mut app, WALK));
    let clip = load(&app, WALK);
    app.world_mut().get_mut::<PlayAnimation>(root).unwrap().clip = clip;
    let weights = |app: &App| {
        let p = player(app, root);
        (p.animation(run).map_or(0.0, |a| a.weight()), p.animation(walk).map_or(0.0, |a| a.weight()))
    };
    // One frame in: the run barely faded.
    app.update();
    let (r, w) = weights(&app);
    assert!(r > 0.9 && w < 0.1, "at 0: run {r}, walk {w}");
    // Half of 0.25 s.
    for _ in 0..7 {
        app.update();
    }
    let (r, w) = weights(&app);
    assert!((r - 0.5).abs() < 0.1 && (w - 0.5).abs() < 0.1, "at half: run {r}, walk {w}");
    // Past 0.25 s: the run is gone and the walk repeats at full weight.
    for _ in 0..10 {
        app.update();
    }
    let p = player(&app, root);
    assert!(!p.is_playing_animation(run), "the run stopped");
    let a = p.animation(walk).unwrap();
    assert_eq!(a.weight(), 1.0);
    assert_eq!(a.repeat_mode(), RepeatAnimation::Forever);
}

#[test]
fn speed_applies_in_place() {
    let mut app = app();
    let root = fox(&mut app, RUN);
    ready(&mut app, root);
    for _ in 0..20 {
        app.update();
    }
    let run = node(&mut app, RUN);
    let before = player(&app, root).animation(run).unwrap().elapsed();
    assert!(before > 0.0);
    app.world_mut().get_mut::<PlayAnimation>(root).unwrap().speed = 2.0;
    app.update();
    let a = player(&app, root).animation(run).unwrap();
    assert_eq!(a.speed(), 2.0);
    assert!(a.elapsed() > before, "the clip kept its time: {} after {before}", a.elapsed());
    let repeat = false;
    app.world_mut().get_mut::<PlayAnimation>(root).unwrap().repeat = repeat;
    app.update();
    assert_eq!(player(&app, root).animation(run).unwrap().repeat_mode(), RepeatAnimation::Never);
}

#[test]
fn removing_it_stops_the_players() {
    let mut app = app();
    let root = fox(&mut app, SURVEY);
    ready(&mut app, root);
    app.update();
    let p = targets(app.world(), root)[0];
    app.world_mut().entity_mut(root).remove::<PlayAnimation>();
    app.update();
    assert_eq!(app.world().get::<AnimationPlayer>(p).unwrap().playing_animations().count(), 0);
    assert!(app.world().get::<AnimationTargets>(root).is_none());
}

#[test]
fn a_hundred_foxes_share_one_graph() {
    let mut app = app();
    let roots: Vec<Entity> = (0..100).map(|_| fox(&mut app, RUN)).collect();
    until(&mut app, "every fox", |w| roots.iter().all(|r| !targets(w, *r).is_empty()));
    let shared = app.world().resource::<SharedGraph>().handle.clone();
    for r in &roots {
        for p in targets(app.world(), *r) {
            assert_eq!(app.world().get::<AnimationGraphHandle>(p).unwrap().0, shared);
        }
    }
    assert_eq!(app.world().resource::<SharedGraph>().clips(), 1);
    // The graph's root and one clip node.
    assert_eq!(app.world().resource::<Assets<AnimationGraph>>().get(&shared).unwrap().graph.node_count(), 2);
}

#[test]
fn a_scene_with_no_animation_warns_once() {
    let mut app = app();
    let scene = load(&app, "models/still.gltf#Scene0");
    let clip = load(&app, RUN);
    let root = app.world_mut().spawn((WorldAssetRoot(scene), PlayAnimation { clip, ..default() })).id();
    until(&mut app, "the warning", |w| w.resource::<AnimationReports>().roots.contains(&root));
    // Ready again (a reload): no second warning.
    let i = **app.world().get::<WorldInstance>(root).unwrap();
    app.world_mut().trigger(WorldInstanceReady { entity: root, instance_id: i });
    app.update();
    assert_eq!(app.world().resource::<AnimationReports>().warnings, 1);
    assert!(targets(app.world(), root).is_empty());
}

#[test]
fn a_missing_clip_is_reported_once() {
    let mut app = app();
    let a = fox(&mut app, "models/Fox.glb#Animation9");
    let b = fox(&mut app, "models/Fox.glb#Animation9");
    until(&mut app, "the report", |w| w.resource::<AnimationReports>().errors > 0);
    for _ in 0..5 {
        app.update();
    }
    let reports = app.world().resource::<AnimationReports>();
    assert_eq!(reports.errors, 1);
    assert!(reports.clips.contains("models/Fox.glb#Animation9"), "{:?}", reports.clips);
    let _ = (a, b);
}

#[derive(Resource, Default)]
struct Readies(usize);

#[test]
fn a_clip_named_later_does_not_load_the_file_again() {
    let mut app = bare_app();
    app.init_resource::<Readies>().add_observer(|_: On<WorldInstanceReady>, mut r: ResMut<Readies>| r.0 += 1);
    let root = fox(&mut app, RUN);
    ready(&mut app, root);
    // The scene's and the clip's labels were asked for in one frame, so the file may load twice
    // at the start; let that settle.
    for _ in 0..120 {
        app.update();
        std::thread::sleep(Duration::from_millis(2));
    }
    let before = app.world().resource::<Readies>().0;
    let clip = load(&app, WALK);
    app.world_mut().get_mut::<PlayAnimation>(root).unwrap().clip = clip;
    for _ in 0..120 {
        app.update();
        std::thread::sleep(Duration::from_millis(2));
    }
    assert_eq!(app.world().resource::<Readies>().0, before, "the walk came from the held file: the scene did not spawn again");
    let walk = node(&mut app, WALK);
    assert!(player(&app, root).is_playing_animation(walk));
}
