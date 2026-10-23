//! `PlayAnimation` end to end against the real V8 backend and Bevy's `Fox.glb`: placed from TSX
//! with a bound clip, and from a level with no TS animation code. The engine's plugins with no
//! window and no GPU.

use crate::common;

use bevy::animation::graph::{AnimationGraph, AnimationNodeIndex};
use bevy::animation::{AnimationClip, AnimationPlayer};
use bevy::prelude::*;
use bevy::render::RenderPlugin;
use bevy::render::settings::{RenderCreation, WgpuSettings};
use bevy::world_serialization::WorldAssetRoot;
use bevy_ts_animation::{AnimationTargets, PlayAnimation, SharedGraph};
use bevy_ts_levels::engine::{Levels, LevelsPlugin};
use common::*;
use std::path::Path;
use std::time::{Duration, Instant};

fn anim_app(dir: &Path, entry: &str) -> App {
    let assets = dir.join("assets").canonicalize().unwrap();
    let render = RenderPlugin { render_creation: RenderCreation::Automatic(Box::new(WgpuSettings { backends: None, ..default() })), ..default() };
    let mut app = App::new();
    app.add_plugins(bevy_ts_cli::headless::default_plugins(true).set(render).set(bevy_ts_cli::headless::asset_plugin_unwatched(&assets)))
        .add_plugins(bevy_ts::BevyTsPlugin::new(bevy_ts_v8::factory(Default::default())).dev(true))
        .add_plugins(bevy_ts_tsx::TsxPlugin)
        .add_plugins(bevy_ts_animation::PlayAnimationPlugin)
        .add_plugins(LevelsPlugin);
    app.world_mut().resource_mut::<Levels>().asset_root = assets;
    // The glTF loader registers in `finish`, which `update` alone never runs.
    app.finish();
    app.cleanup();
    app.insert_resource(bevy_ts::generation::DevSource(std::sync::Arc::new(bevy_ts_cli::compile::Compiler::new(dir.join(entry), true).unwrap())));
    app
}

fn fox_project(name: &str, files: &[(&str, &str)]) -> std::path::PathBuf {
    let dir = project(name, files);
    std::fs::create_dir_all(dir.join("assets/models")).unwrap();
    std::fs::copy(Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/foxes/assets/models/Fox.glb"), dir.join("assets/models/Fox.glb")).unwrap();
    dir
}

fn until(app: &mut App, what: &str, done: impl Fn(&mut World) -> bool) {
    let start = Instant::now();
    while !done(app.world_mut()) {
        assert!(start.elapsed() < Duration::from_secs(60), "timed out waiting for {what}: {:?}", messages(app));
        app.update();
        std::thread::sleep(Duration::from_millis(2));
    }
}

fn the_fox(world: &mut World) -> Option<Entity> {
    world.query_filtered::<Entity, With<WorldAssetRoot>>().iter(world).next()
}

fn node(app: &mut App, clip: &str) -> AnimationNodeIndex {
    let clip: Handle<AnimationClip> = app.world().resource::<AssetServer>().load(clip.to_string());
    app.world_mut().resource_scope(|world, mut shared: Mut<SharedGraph>| shared.node(&mut world.resource_mut::<Assets<AnimationGraph>>(), &clip).unwrap())
}

fn playing(app: &mut App, root: Entity, clip: &str) -> bool {
    let n = node(app, clip);
    let world = app.world();
    world.get::<AnimationTargets>(root).is_some_and(|t| t.0.iter().any(|p| world.get::<AnimationPlayer>(*p).is_some_and(|p| p.is_playing_animation(n))))
}

const GAME: &str = r#"import { app, signal } from "bevy-ts";
import { PlayAnimation, WorldAssetRoot } from "bevy-ts/bevy";
const clips = ["models/Fox.glb#Animation2", "models/Fox.glb#Animation1"];
const [clip, setClip] = signal(0);
app.entry("walk", () => { setClip(1); });
app.scene("foxes", () => (
  <entity key="fox">
    <WorldAssetRoot value="models/Fox.glb#Scene0" />
    <PlayAnimation clip={() => clips[clip()]} transition_secs={0.25} />
  </entity>
));
"#;

#[test]
fn a_tsx_fox_plays_and_its_bound_clip_cross_fades() {
    let dir = fox_project("animation-tsx", &[("package.json", "{}"), ("src/game.tsx", GAME)]);
    let mut app = anim_app(&dir, "src/game.tsx");
    let r = reload(&mut app, false);
    assert!(r.errors.is_empty(), "{:?}", r.errors);
    until(&mut app, "the fox", |w| the_fox(w).is_some_and(|f| w.get::<AnimationTargets>(f).is_some_and(|t| !t.0.is_empty())));
    app.update();
    let fox = the_fox(app.world_mut()).unwrap();
    let play = app.world().get::<PlayAnimation>(fox).unwrap().clone();
    assert_eq!(play.clip.path().unwrap().to_string(), "models/Fox.glb#Animation2");
    assert_eq!((play.repeat, play.speed, play.transition_secs), (true, 1.0, 0.25));
    assert!(playing(&mut app, fox, "models/Fox.glb#Animation2"), "the run plays");

    bevy_ts::plugin_api::call_entry(app.world_mut(), "walk", b"").unwrap();
    until(&mut app, "the walk", |w| w.get::<PlayAnimation>(fox).is_some_and(|p| p.clip.path().is_some_and(|p| p.to_string().ends_with("#Animation1"))));
    app.update();
    assert!(playing(&mut app, fox, "models/Fox.glb#Animation1"), "the walk plays after the binding changed");
}

const LEVEL_GAME: &str = r#"import { app } from "bevy-ts";
app.level("levels/foxes.bsn");
"#;

const LEVEL: &str = r#"#Fox
WorldAssetRoot("models/Fox.glb#Scene0")
PlayAnimation { clip: "models/Fox.glb#Animation1" }
"#;

#[test]
fn a_level_fox_plays_with_no_ts() {
    let dir = fox_project("animation-level", &[("package.json", "{}"), ("src/game.ts", LEVEL_GAME), ("assets/levels/foxes.bsn", LEVEL)]);
    let mut app = anim_app(&dir, "src/game.ts");
    let r = reload(&mut app, false);
    assert!(r.errors.is_empty(), "{:?}", r.errors);
    until(&mut app, "the level fox", |w| the_fox(w).is_some_and(|f| w.get::<AnimationTargets>(f).is_some_and(|t| !t.0.is_empty())));
    app.update();
    let fox = the_fox(app.world_mut()).unwrap();
    assert!(playing(&mut app, fox, "models/Fox.glb#Animation1"), "the walk plays: {:?}", messages(&app));
}

const PICK: &str = r#"import { app, signal } from "bevy-ts";
import { WorldAssetRoot } from "bevy-ts/bevy";
const [hit, setHit] = signal("");
app.entry("hit", () => hit());
app.scene("foxes", () => (
  <entity key="fox" onPointerClick={({ entity, target }) => setHit((h) => `${h}${entity}>${target};`)}>
    <WorldAssetRoot value="models/Fox.glb#Scene0" />
  </entity>
));
"#;

/// Change scene-pick: a click on a mesh of a glTF scene bubbles to the handler on the entity
/// carrying `WorldAssetRoot`, and the event names the mesh.
#[test]
fn a_click_on_a_scene_mesh_runs_the_roots_handler() {
    use bevy::camera::NormalizedRenderTarget;
    use bevy::picking::backend::HitData;
    use bevy::picking::events::{Pointer, PointerClick};
    use bevy::picking::pointer::{Location, PointerButton, PointerId};
    let dir = fox_project("scene-pick", &[("package.json", "{}"), ("src/game.tsx", PICK)]);
    let mut app = anim_app(&dir, "src/game.tsx");
    let r = reload(&mut app, false);
    assert!(r.errors.is_empty(), "{:?}", r.errors);
    let mesh_under = |w: &mut World, root: Entity| {
        let meshes: Vec<Entity> = w.query_filtered::<Entity, With<Mesh3d>>().iter(w).collect();
        meshes.into_iter().find(|m| {
            let mut e = *m;
            while let Some(p) = w.get::<ChildOf>(e).map(ChildOf::parent) {
                if p == root {
                    return true;
                }
                e = p;
            }
            false
        })
    };
    until(&mut app, "the fox's meshes", |w| the_fox(w).is_some_and(|f| mesh_under(w, f).is_some()));
    let fox = the_fox(app.world_mut()).unwrap();
    let mesh = mesh_under(app.world_mut(), fox).unwrap();
    assert_ne!(mesh, fox, "the mesh is a descendant, not the root");
    let pointer = Pointer::new(PointerId::Mouse, Location { target: NormalizedRenderTarget::None { width: 1, height: 1 }, position: Vec2::ZERO });
    let hit = HitData::new(Entity::PLACEHOLDER, 0.0, None, None);
    app.world_mut().trigger(PointerClick { entity: mesh, pointer, button: PointerButton::Primary, hit, duration: std::time::Duration::ZERO, count: 1 });
    app.update();
    let out = bevy_ts::plugin_api::call_entry(app.world_mut(), "hit", b"").unwrap();
    // Once: one call, naming the root and the mesh.
    assert_eq!(String::from_utf8(out).unwrap(), format!("{}>{};", fox.to_bits(), mesh.to_bits()));
}
