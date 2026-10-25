//! `PlayAnimation` end to end against the real V8 backend and Bevy's `Fox.glb`: placed from TSX
//! with a bound clip, and from a level with no TS animation code. The engine's plugins with no
//! window and no GPU.

use crate::common;

use bevy::animation::graph::{AnimationGraph, AnimationNodeIndex};
use bevy::animation::{AnimationClip, AnimationPlayer, RepeatAnimation};
use bevy::prelude::*;
use bevy::render::RenderPlugin;
use bevy::render::settings::{RenderCreation, WgpuSettings};
use bevy::world_serialization::WorldAssetRoot;
use bevy_ts_animation::{AnimationTargets, PlayAnimation, SharedGraph};
use bevy_ts_levels::engine::{Levels, LevelsPlugin};
use common::*;
use std::path::Path;

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
    world.get::<AnimationTargets>(root).is_some_and(|t| t.players.iter().any(|p| world.get::<AnimationPlayer>(*p).is_some_and(|p| p.is_playing_animation(n))))
}

const GAME: &str = r#"import { app, signal } from "bevy-ts";
import { PlayAnimation, WorldAssetRoot } from "bevy-ts/bevy";
const clips = ["models/Fox.glb#Animation2", "models/Fox.glb#Animation1"];
const [clip, setClip] = signal(0);
app.entry("walk", () => { setClip(1); });
app.scene("foxes", () => (
  <entity key="fox">
    <WorldAssetRoot value="models/Fox.glb#Scene0" />
    <PlayAnimation clip={() => clips[clip()]} repeat={{ Count: 3 }} transition_secs={0.25} />
  </entity>
));
"#;

#[test]
fn a_tsx_fox_plays_and_its_bound_clip_cross_fades() {
    let dir = fox_project("animation-tsx", &[("package.json", "{}"), ("src/game.tsx", GAME)]);
    let mut app = anim_app(&dir, "src/game.tsx");
    let r = reload(&mut app, false);
    assert!(r.errors.is_empty(), "{:?}", r.errors);
    until(&mut app, "the fox", |app| {
        let w = app.world_mut();
        the_fox(w).is_some_and(|f| w.get::<AnimationTargets>(f).is_some_and(|t| !t.players.is_empty()))
    });
    app.update();
    let fox = the_fox(app.world_mut()).unwrap();
    let play = app.world().get::<PlayAnimation>(fox).unwrap().clone();
    assert_eq!(play.clip.path().unwrap().to_string(), "models/Fox.glb#Animation2");
    assert_eq!((play.repeat, play.speed, play.transition_secs, play.paused, play.cull), (RepeatAnimation::Count(3), 1.0, 0.25, false, false));
    assert!(playing(&mut app, fox, "models/Fox.glb#Animation2"), "the run plays");
    let run = node(&mut app, "models/Fox.glb#Animation2");
    let p = app.world().get::<AnimationTargets>(fox).unwrap().players[0];
    assert_eq!(app.world().get::<AnimationPlayer>(p).unwrap().animation(run).unwrap().repeat_mode(), RepeatAnimation::Count(3), "TSX's count reached the player");

    bevy_ts::plugin_api::call_entry(app.world_mut(), "walk", b"").unwrap();
    until(&mut app, "the walk", |app| {
        let w = app.world_mut();
        w.get::<PlayAnimation>(fox).is_some_and(|p| p.clip.path().is_some_and(|p| p.to_string().ends_with("#Animation1")))
    });
    app.update();
    assert!(playing(&mut app, fox, "models/Fox.glb#Animation1"), "the walk plays after the binding changed");
}

const LEVEL_GAME: &str = r#"import { app } from "bevy-ts";
app.level("levels/foxes.bsn");
"#;

const LEVEL: &str = r#"#Fox
WorldAssetRoot("models/Fox.glb#Scene0")
PlayAnimation { clip: "models/Fox.glb#Animation1", repeat: Count(3) }
"#;

#[test]
fn a_level_fox_plays_with_no_ts() {
    let dir = fox_project("animation-level", &[("package.json", "{}"), ("src/game.ts", LEVEL_GAME), ("assets/levels/foxes.bsn", LEVEL)]);
    let mut app = anim_app(&dir, "src/game.ts");
    let r = reload(&mut app, false);
    assert!(r.errors.is_empty(), "{:?}", r.errors);
    until(&mut app, "the level fox", |app| {
        let w = app.world_mut();
        the_fox(w).is_some_and(|f| w.get::<AnimationTargets>(f).is_some_and(|t| !t.players.is_empty()))
    });
    app.update();
    let fox = the_fox(app.world_mut()).unwrap();
    assert!(playing(&mut app, fox, "models/Fox.glb#Animation1"), "the walk plays: {:?}", messages(&app));
    assert_eq!(app.world().get::<PlayAnimation>(fox).unwrap().repeat, RepeatAnimation::Count(3));
    let walk = node(&mut app, "models/Fox.glb#Animation1");
    let p = app.world().get::<AnimationTargets>(fox).unwrap().players[0];
    assert_eq!(app.world().get::<AnimationPlayer>(p).unwrap().animation(walk).unwrap().repeat_mode(), RepeatAnimation::Count(3));
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
    until(&mut app, "the fox's meshes", |app| {
        let w = app.world_mut();
        the_fox(w).is_some_and(|f| mesh_under(w, f).is_some())
    });
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

const STATUS_GAME: &str = r#"import { app, Update } from "bevy-ts";
import type { Entity, Query } from "bevy-ts";
import { PlayAnimation, Transform, WorldAssetRoot } from "bevy-ts/bevy";
import type { Animations } from "bevy-ts/animation";
let frames = 0;
export function read(anims: Animations, roots: Query<[Entity, PlayAnimation]>, others: Query<[Entity, Transform]>) {
  if (++frames % 30 !== 0) return;
  roots.each((e, p) => {
    const s = anims.status(e);
    const tag = p.paused ? "held" : "run";
    console.log(s ? `${tag} loaded ${s.loaded} duration ${s.duration.toFixed(2)} seek ${s.seekTime.toFixed(2)} elapsed ${s.elapsed.toFixed(2)} completions ${s.completions} finished ${s.finished} paused ${s.paused} parked ${s.parked} weight ${s.weight}` : `${tag} none`);
  });
  let other: Entity | undefined;
  others.each((e) => { if (other === undefined && anims.status(e) === undefined) other = e; });
  console.log(`other ${other === undefined ? "missing" : "none"}`);
}
app.addSystems(Update, read);
app.scene("foxes", () => (
  <>
    <entity key="run">
      <WorldAssetRoot value="models/Fox.glb#Scene0" />
      <PlayAnimation clip="models/Fox.glb#Animation2" />
    </entity>
    <entity key="held">
      <WorldAssetRoot value="models/Fox.glb#Scene0" />
      <PlayAnimation clip="models/Fox.glb#Animation2" paused={true} />
    </entity>
  </>
));
"#;

#[test]
fn animations_status() {
    let dir = fox_project("animation-status", &[("package.json", "{}"), ("src/game.tsx", STATUS_GAME)]);
    let mut app = anim_app(&dir, "src/game.tsx");
    let r = reload(&mut app, false);
    assert!(r.errors.is_empty(), "{:?}", r.errors);
    let ready = |app: &mut App| {
        let w = app.world_mut();
        let roots: Vec<Entity> = w.query_filtered::<Entity, With<WorldAssetRoot>>().iter(w).collect();
        roots.len() == 2 && roots.iter().all(|f| w.get::<AnimationTargets>(*f).is_some_and(|t| !t.players.is_empty()))
    };
    until(&mut app, "both foxes", ready);
    // Half a second on, for two status lines from each.
    until(&mut app, "two status lines", |app| messages(app).iter().filter(|l| l.starts_with("run loaded")).count() >= 2);
    let m = messages(&app);
    let last = |tag: &str| m.iter().rev().find(|l| l.starts_with(tag)).cloned().unwrap_or_else(|| panic!("no {tag} in {m:?}"));
    let run = last("run loaded");
    let field = |line: &str, name: &str| line.split(' ').skip_while(|w| *w != name).nth(1).unwrap().to_string();
    assert_eq!(field(&run, "loaded"), "true", "{run}");
    assert!(field(&run, "duration").parse::<f32>().unwrap() > 0.5, "{run}");
    assert!(field(&run, "seek").parse::<f32>().unwrap() > 0.0, "{run}");
    assert_eq!((field(&run, "paused"), field(&run, "parked"), field(&run, "weight")), ("false".into(), "false".into(), "1".into()), "{run}");
    let held = last("held");
    assert_eq!((field(&held, "seek"), field(&held, "paused"), field(&held, "parked")), ("0.00".into(), "true".into(), "true".into()), "{held}");
    assert_eq!(last("other"), "other none", "{m:?}");
}

const BOOL_REPEAT: &str = r#"import { app } from "bevy-ts";
import { PlayAnimation, WorldAssetRoot } from "bevy-ts/bevy";
app.scene("foxes", () => (
  <entity key="fox">
    <WorldAssetRoot value="models/Fox.glb#Scene0" />
    <PlayAnimation clip="models/Fox.glb#Animation2" repeat={true} />
  </entity>
));
"#;

/// The old bool `repeat` is refused at the reflected write, naming the field and its type.
#[test]
fn a_bool_repeat_is_refused() {
    let dir = fox_project("animation-bool-repeat", &[("package.json", "{}"), ("src/game.tsx", BOOL_REPEAT)]);
    let mut app = anim_app(&dir, "src/game.tsx");
    let r = reload(&mut app, false);
    let said = format!("{:?} {:?}", r.errors, messages(&app));
    assert!(said.contains("PlayAnimation.repeat") && said.contains("RepeatAnimation"), "{said}");
}

const DRIVE_GAME: &str = r#"import { app, Update } from "bevy-ts";
import type { Entity, Query } from "bevy-ts";
import { PlayAnimation, Transform, WorldAssetRoot } from "bevy-ts/bevy";
import type { Animations } from "bevy-ts/animation";
let step = 0;
export function drive(anims: Animations, roots: Query<[Entity, PlayAnimation]>, others: Query<[Entity, Transform]>) {
  const rs: [Entity, string][] = [];
  let ready = true;
  roots.each((e, p) => {
    rs.push([e, p.paused ? "held" : "run"]);
    if (anims.status(e) === undefined) ready = false;
  });
  if (!ready || rs.length < 2) return;
  step++;
  const at = () => rs.map(([e, tag]) => `${tag} ${anims.status(e)!.seekTime.toFixed(2)}`).join(" ");
  if (step === 1) {
    // The frame both scenes are ready: start offsets, one on a parked root.
    console.log(`seeked ${rs.map(([e, tag]) => anims.seek(e, tag === "held" ? 0.3 : 0.7)).join(" ")}`);
    let other: Entity | undefined;
    others.each((e) => { if (other === undefined && anims.status(e) === undefined) other = e; });
    console.log(`other ${anims.seek(other!, 1)} ${anims.rewind(other!)} ${anims.replay(other!)}`);
  } else if (step === 3) {
    console.log(`after seek ${at()}`);
    for (const [e, tag] of rs) if (tag === "run") anims.rewind(e); else anims.replay(e);
  } else if (step === 5) {
    console.log(`after rewind ${at()}`);
  }
}
app.addSystems(Update, drive);
app.scene("foxes", () => (
  <>
    <entity key="run">
      <WorldAssetRoot value="models/Fox.glb#Scene0" />
      <PlayAnimation clip="models/Fox.glb#Animation2" />
    </entity>
    <entity key="held">
      <WorldAssetRoot value="models/Fox.glb#Scene0" />
      <PlayAnimation clip="models/Fox.glb#Animation2" paused={true} />
    </entity>
  </>
));
"#;

#[test]
fn animations_seek_rewind_replay() {
    let dir = fox_project("animation-seek", &[("package.json", "{}"), ("src/game.tsx", DRIVE_GAME)]);
    let mut app = anim_app(&dir, "src/game.tsx");
    // Frames of 1/60 s, not of wall time: a loaded machine's long frames would carry the run past
    // where two frames take it.
    app.insert_resource(bevy::time::TimeUpdateStrategy::ManualDuration(std::time::Duration::from_secs_f32(1.0 / 60.0)));
    let r = reload(&mut app, false);
    assert!(r.errors.is_empty(), "{:?}", r.errors);
    until(&mut app, "the rewind line", |app| messages(app).iter().any(|l| l.starts_with("after rewind")));
    let m = messages(&app);
    let line = |p: &str| m.iter().find(|l| l.starts_with(p)).cloned().unwrap_or_else(|| panic!("no {p} in {m:?}"));
    assert_eq!(line("seeked"), "seeked true true");
    assert_eq!(line("other"), "other false false false", "a non-root changes nothing");
    let field = |l: &str, tag: &str| -> f32 { l.split(' ').skip_while(|w| *w != tag).nth(1).unwrap().parse().unwrap() };
    let after = line("after seek");
    let run = field(&after, "run");
    assert!((0.7..0.8).contains(&run), "the run plays on from 0.7: {after}");
    assert_eq!(field(&after, "held"), 0.3, "a parked root's clip moved and stands still: {after}");
    let back = line("after rewind");
    assert!(field(&back, "run") < 0.2, "rewound to 0 and playing: {back}");
    assert_eq!(field(&back, "held"), 0.0, "replayed, still parked: {back}");
}

fn graph_handle(app: &App, root: Entity) -> Handle<AnimationGraph> {
    let p = app.world().get::<AnimationTargets>(root).unwrap().players[0];
    app.world().get::<bevy::animation::graph::AnimationGraphHandle>(p).unwrap().0.clone()
}

/// The clip nodes the root's first player plays, and each one's elapsed seconds.
fn clips_playing(app: &App, root: Entity) -> Vec<(usize, f32)> {
    let p = app.world().get::<AnimationTargets>(root).unwrap().players[0];
    let mut out: Vec<(usize, f32)> = app.world().get::<AnimationPlayer>(p).unwrap().playing_animations().map(|(n, a)| (n.index(), a.elapsed())).collect();
    out.sort_by_key(|(n, _)| *n);
    out
}

const GRAPH_GAME: &str = r#"import { app } from "bevy-ts";
import { AnimationGraph, PlayAnimation, WorldAssetRoot } from "bevy-ts/bevy";
const g = new AnimationGraph();
const b = g.addBlend(1, g.root);
g.addClip("models/Fox.glb#Animation0", 0.5, b);
g.addClip("models/Fox.glb#Animation2", 0.5, b);
app.scene("foxes", () => (
  <entity key="fox">
    <WorldAssetRoot value="models/Fox.glb#Scene0" />
    <PlayAnimation graph={g} />
  </entity>
));
"#;

#[test]
fn a_ts_built_graph_blends() {
    let dir = fox_project("animation-graph", &[("package.json", "{}"), ("src/game.tsx", GRAPH_GAME)]);
    let mut app = anim_app(&dir, "src/game.tsx");
    let r = reload(&mut app, false);
    assert!(r.errors.is_empty(), "{:?}", r.errors);
    until(&mut app, "the fox to play both clips", |app| {
        let w = app.world_mut();
        the_fox(w).is_some_and(|f| w.get::<AnimationTargets>(f).is_some_and(|t| !t.players.is_empty())) && {
            let f = the_fox(app.world_mut()).unwrap();
            clips_playing(app, f).len() == 2
        }
    });
    let fox = the_fox(app.world_mut()).unwrap();
    let handle = graph_handle(&app, fox);
    assert_ne!(handle, app.world().resource::<SharedGraph>().handle, "the fox has its own graph, not the shared one");
    let graphs = app.world().resource::<Assets<AnimationGraph>>();
    let g = graphs.get(&handle).expect("the built graph is an asset");
    let weights: Vec<f32> = g.nodes().map(|n| g[n].weight).collect();
    assert_eq!(weights, vec![1.0, 1.0, 0.5, 0.5]);
    assert_eq!(clips_playing(&app, fox).iter().map(|(n, _)| *n).collect::<Vec<_>>(), vec![2, 3], "both clip nodes play");
}

const REWEIGHT_GAME: &str = r#"import { app, assets, Update } from "bevy-ts";
import { AnimationGraph, PlayAnimation, WorldAssetRoot } from "bevy-ts/bevy";
const g = new AnimationGraph();
const b = g.addBlend(1, g.root);
const survey = g.addClip("models/Fox.glb#Animation0", 0.5, b);
const run = g.addClip("models/Fox.glb#Animation2", 0.5, b);
const ref = assets.add(g);
let frames = 0;
export function reweight() {
  if (++frames !== 40) return;
  g.get(survey)!.weight = 1;
  g.get(run)!.weight = 0;
  assets.set(ref, g);
  console.log("reweighted");
}
app.addSystems(Update, reweight);
app.scene("foxes", () => (
  <entity key="fox">
    <WorldAssetRoot value="models/Fox.glb#Scene0" />
    <PlayAnimation graph={ref} />
  </entity>
));
"#;

#[test]
fn a_ts_graph_reweights_live() {
    let dir = fox_project("animation-reweight", &[("package.json", "{}"), ("src/game.tsx", REWEIGHT_GAME)]);
    let mut app = anim_app(&dir, "src/game.tsx");
    let r = reload(&mut app, false);
    assert!(r.errors.is_empty(), "{:?}", r.errors);
    until(&mut app, "the reweight", |app| messages(app).iter().any(|l| l == "reweighted"));
    let fox = the_fox(app.world_mut()).unwrap();
    let before = clips_playing(&app, fox);
    assert_eq!(before.len(), 2, "both clips play: {before:?}");
    for _ in 0..3 {
        app.update();
    }
    let handle = graph_handle(&app, fox);
    let graphs = app.world().resource::<Assets<AnimationGraph>>();
    let g = graphs.get(&handle).unwrap();
    assert_eq!(g.nodes().map(|n| g[n].weight).collect::<Vec<_>>(), vec![1.0, 1.0, 1.0, 0.0], "the weights reached the asset");
    let after = clips_playing(&app, fox);
    assert_eq!(after.iter().map(|(n, _)| *n).collect::<Vec<_>>(), vec![2, 3], "no clip stopped");
    for ((_, a), (_, b)) in before.iter().zip(&after) {
        assert!(b > a, "no clip restarted: {before:?} then {after:?}");
    }
}
