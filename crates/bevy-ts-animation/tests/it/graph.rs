//! Graph roots (change `animation-control`, stage 2): a `PlayAnimation` with a `graph` plays every
//! clip node of that `AnimationGraph` in place of the shared graph, blended by its weights and
//! masks.

use bevy::animation::graph::{AnimationGraph, AnimationGraphHandle, AnimationNodeType};
use bevy::animation::{AnimationPlayer, AnimationTargetId};
use bevy::platform::collections::HashMap;
use bevy::prelude::*;
use bevy::world_serialization::WorldAssetRoot;
use bevy_ts_animation::{AnimationReports, AnimationTargets, PlayAnimation, SharedGraph};
use crate::common::*;

/// A bone of the fox's left hind leg, as Bevy's `animation_masks` names it.
const LEG: &str = "root/_rootJoint/b_Root_00/b_Hip_01/b_LeftLeg01_015";

fn graph_fox(app: &mut App, graph: Handle<AnimationGraph>) -> Entity {
    let scene = load(app, SCENE);
    app.world_mut().spawn((WorldAssetRoot(scene), PlayAnimation { graph, ..default() })).id()
}

fn add(app: &mut App, graph: AnimationGraph) -> Handle<AnimationGraph> {
    app.world_mut().resource_mut::<Assets<AnimationGraph>>().add(graph)
}

fn clip(app: &App, path: &str) -> Handle<AnimationClip> {
    load(app, path)
}

/// One clip, alone under the graph's root.
fn one(app: &mut App, path: &str) -> Handle<AnimationGraph> {
    let (g, _) = AnimationGraph::from_clip(clip(app, path));
    add(app, g)
}

/// Every animated bone under the root, by target id.
fn bones(world: &World, root: Entity) -> HashMap<AnimationTargetId, Transform> {
    let mut out = HashMap::default();
    let mut stack = vec![root];
    while let Some(e) = stack.pop() {
        if let Some(id) = world.get::<AnimationTargetId>(e) {
            out.insert(*id, *world.get::<Transform>(e).unwrap());
        }
        if let Some(c) = world.get::<Children>(e) {
            stack.extend(c.iter());
        }
    }
    out
}

fn handle(app: &App, root: Entity) -> Option<Handle<AnimationGraph>> {
    let p = targets(app.world(), root)[0];
    app.world().get::<AnimationGraphHandle>(p).map(|h| h.0.clone())
}

fn playing(app: &App, root: Entity) -> Vec<u32> {
    let mut n: Vec<u32> = player(app, root).playing_animations().map(|(n, _)| n.index() as u32).collect();
    n.sort();
    n
}

fn steps(app: &mut App, n: usize) {
    for _ in 0..n {
        app.update();
    }
}

/// Waits until the root's players play `n` clips: the graph and its clips have loaded.
fn plays(app: &mut App, root: Entity, n: usize) {
    ready(app, root);
    until(app, "the graph to play", |w| {
        let p = w.get::<AnimationTargets>(root).unwrap().players[0];
        w.get::<AnimationPlayer>(p).unwrap().playing_animations().count() == n
    });
}

fn close(a: Vec3, b: Vec3) -> bool {
    a.abs_diff_eq(b, 1e-4)
}

#[test]
fn a_ron_graph_plays() {
    let mut app = app();
    let graph: Handle<AnimationGraph> = load(&app, "anims/fox_blend.animgraph.ron");
    let root = graph_fox(&mut app, graph.clone());
    plays(&mut app, root, 2);
    assert_eq!(handle(&app, root), Some(graph), "the root's players have its graph");
    assert_ne!(handle(&app, root), Some(app.world().resource::<SharedGraph>().handle.clone()));
    assert_eq!(playing(&app, root), vec![1, 2], "both clip nodes play");
}

#[test]
fn a_graph_blends_two_clips() {
    let mut app = app();
    let survey = one(&mut app, SURVEY);
    let run = one(&mut app, RUN);
    let mut g = AnimationGraph::new();
    let b = g.add_blend(1.0, g.root);
    g.add_clip(clip(&app, SURVEY), 0.5, b);
    g.add_clip(clip(&app, RUN), 0.5, b);
    let blend = add(&mut app, g);
    let roots = [graph_fox(&mut app, survey), graph_fox(&mut app, run), graph_fox(&mut app, blend)];
    for (r, n) in roots.iter().zip([1, 1, 2]) {
        plays(&mut app, *r, n);
    }
    steps(&mut app, 20);
    assert_eq!(playing(&app, roots[2]), vec![2, 3]);
    let [a, b, c] = roots.map(|r| bones(app.world(), r));
    let mut between = 0;
    for (id, t) in &c {
        let (ta, tb) = (a[id].translation, b[id].translation);
        let (lo, hi) = (ta.min(tb) - Vec3::splat(1e-3), ta.max(tb) + Vec3::splat(1e-3));
        assert!(t.translation.cmpge(lo).all() && t.translation.cmple(hi).all(), "{id:?}: {} not between {ta} and {tb}", t.translation);
        if !close(t.translation, ta) && !close(t.translation, tb) {
            between += 1;
        }
    }
    assert!(between > 0, "some bone lies strictly between the two clips' poses");
}

#[test]
fn an_additive_blend_adds() {
    let mut app = app();
    let run = one(&mut app, RUN);
    let mut g = AnimationGraph::new();
    g.add_clip(clip(&app, RUN), 1.0, g.root);
    let a = g.add_additive_blend(1.0, g.root);
    g.add_clip(clip(&app, SURVEY), 1.0, a);
    let layered = add(&mut app, g);
    let roots = [graph_fox(&mut app, run), graph_fox(&mut app, layered)];
    plays(&mut app, roots[0], 1);
    plays(&mut app, roots[1], 2);
    steps(&mut app, 20);
    let [alone, both] = roots.map(|r| bones(app.world(), r));
    let differ = both.iter().filter(|(id, t)| !close(t.translation, alone[*id].translation) || !t.rotation.abs_diff_eq(alone[*id].rotation, 1e-4)).count();
    assert!(differ > 0, "the additive layer moves some bone off the run alone");
}

#[test]
fn a_mask_keeps_a_bone_still() {
    let mut app = app();
    let leg = AnimationTargetId::from_names(LEG.split('/').map(Name::new).collect::<Vec<_>>().iter());
    let mut g = AnimationGraph::new();
    g.add_target_to_mask_group(leg, 0);
    g.add_clip_with_mask(clip(&app, RUN), 1, 1.0, g.root);
    let masked = add(&mut app, g);
    let root = graph_fox(&mut app, masked);
    plays(&mut app, root, 1);
    steps(&mut app, 2);
    let before = bones(app.world(), root);
    assert!(before.contains_key(&leg), "the fox has the bone {LEG}");
    steps(&mut app, 20);
    let after = bones(app.world(), root);
    assert_eq!(before[&leg], after[&leg], "the masked bone stays still");
    assert!(after.iter().any(|(id, t)| *t != before[id]), "the other bones animate");
}

#[test]
fn a_changed_graph_restarts() {
    let mut app = app();
    let run = one(&mut app, RUN);
    let survey = one(&mut app, SURVEY);
    let root = graph_fox(&mut app, run.clone());
    plays(&mut app, root, 1);
    steps(&mut app, 30);
    let first = player(&app, root).playing_animations().next().map(|(n, _)| *n).unwrap();
    assert!(player(&app, root).animation(first).unwrap().elapsed() > 0.3);
    app.world_mut().get_mut::<PlayAnimation>(root).unwrap().graph = survey.clone();
    steps(&mut app, 2);
    assert_eq!(handle(&app, root), Some(survey), "the players take the new graph");
    let a = player(&app, root).animation(first).unwrap();
    assert!(a.elapsed() < 0.1, "the new graph starts from 0, not a fade: {}", a.elapsed());

    // Clearing the graph returns the root to its clip in the shared graph.
    let walk = clip(&app, WALK);
    {
        let mut play = app.world_mut().get_mut::<PlayAnimation>(root).unwrap();
        play.graph = Handle::default();
        play.clip = walk;
    }
    steps(&mut app, 2);
    let shared = app.world().resource::<SharedGraph>().handle.clone();
    assert_eq!(handle(&app, root), Some(shared));
    let walk = node(&mut app, WALK);
    assert!(player(&app, root).is_playing_animation(walk), "the clip plays");
}

#[test]
fn an_edited_graph_plays_new_clips() {
    let mut app = app();
    let graph = one(&mut app, RUN);
    let root = graph_fox(&mut app, graph.clone());
    plays(&mut app, root, 1);
    let survey = clip(&app, SURVEY);
    let added = {
        let mut graphs = app.world_mut().resource_mut::<Assets<AnimationGraph>>();
        let mut g = graphs.get_mut(&graph).unwrap();
        let root = g.root;
        g.add_clip(survey, 1.0, root)
    };
    steps(&mut app, 2);
    assert!(player(&app, root).is_playing_animation(added), "a new clip node starts");
    let run = player(&app, root).playing_animations().map(|(n, _)| *n).find(|n| *n != added).unwrap();
    {
        let mut graphs = app.world_mut().resource_mut::<Assets<AnimationGraph>>();
        graphs.get_mut(&graph).unwrap()[run].node_type = AnimationNodeType::Blend;
    }
    steps(&mut app, 2);
    assert!(!player(&app, root).is_playing_animation(run), "a node that is no longer a clip stops");
    assert!(player(&app, root).is_playing_animation(added));
}

#[test]
fn graph_and_clip_warns() {
    let mut app = app();
    let graph = one(&mut app, SURVEY);
    let scene = load(&app, SCENE);
    let run = clip(&app, RUN);
    let root = app.world_mut().spawn((WorldAssetRoot(scene), PlayAnimation { clip: run, graph: graph.clone(), ..default() })).id();
    plays(&mut app, root, 1);
    steps(&mut app, 3);
    let reports = app.world().resource::<AnimationReports>();
    assert!(reports.both.contains(&root));
    assert_eq!(reports.warnings, 1, "one warning");
    assert_eq!(handle(&app, root), Some(graph), "it plays the graph");
}

#[test]
fn a_parked_graph_root_wakes_with_its_graph() {
    let mut app = app();
    let graph = one(&mut app, RUN);
    let root = graph_fox(&mut app, graph.clone());
    plays(&mut app, root, 1);
    app.world_mut().get_mut::<PlayAnimation>(root).unwrap().paused = true;
    steps(&mut app, 2);
    assert_eq!(handle(&app, root), None, "parked");
    app.world_mut().get_mut::<PlayAnimation>(root).unwrap().paused = false;
    steps(&mut app, 2);
    assert_eq!(handle(&app, root), Some(graph), "it wakes with its own graph, not the shared one");
}
