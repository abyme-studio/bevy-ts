//! `PlayAnimation` on Bevy's `Fox.glb`, headless: the engine's plugins with no window and no GPU,
//! time stepped by a fixed 1/60 s.

use bevy::animation::graph::{AnimationGraph, AnimationGraphHandle};
use bevy::animation::{AnimationPlayer, RepeatAnimation};
use bevy::prelude::*;
use bevy::world_serialization::{WorldAssetRoot, WorldInstance, WorldInstanceReady, WorldInstanceSpawner};
use bevy_ts_animation::{AnimationReports, AnimationTargets, PlayAnimation, SharedGraph};
use crate::common::*;
use std::time::Duration;

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
    app.world_mut().get_mut::<PlayAnimation>(root).unwrap().repeat = RepeatAnimation::Never;
    app.update();
    assert_eq!(player(&app, root).animation(run).unwrap().repeat_mode(), RepeatAnimation::Never);
}

#[test]
fn repeat_count_finishes() {
    let mut app = app();
    let root = fox(&mut app, SURVEY);
    ready(&mut app, root);
    app.update();
    let survey = node(&mut app, SURVEY);
    let duration = {
        let clip: Handle<AnimationClip> = load(&app, SURVEY);
        until(&mut app, "the clip", |w| w.resource::<Assets<AnimationClip>>().contains(&clip));
        app.world().resource::<Assets<AnimationClip>>().get(&clip).unwrap().duration()
    };
    // Three times through at speed 4, then the count changes to 2: 3 completions of 2 is finished.
    app.world_mut().get_mut::<PlayAnimation>(root).unwrap().speed = 4.0;
    let frames = (duration * 3.2 / 4.0 / (1.0 / 60.0)).ceil() as usize;
    for _ in 0..frames {
        app.update();
    }
    let a = player(&app, root).animation(survey).unwrap();
    assert!(a.completions() >= 3 && !a.is_finished(), "{} completions, forever", a.completions());
    app.world_mut().get_mut::<PlayAnimation>(root).unwrap().repeat = RepeatAnimation::Count(2);
    app.update();
    let a = player(&app, root).animation(survey).unwrap();
    assert_eq!(a.repeat_mode(), RepeatAnimation::Count(2));
    assert!(a.is_finished(), "{} completions of a count of 2", a.completions());
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
