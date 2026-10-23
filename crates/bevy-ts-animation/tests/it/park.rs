//! Parking (change `animation-control`): a `paused` root, or a `cull` root none of whose meshes is
//! in view, loses `AnimationGraphHandle` on its players, so Bevy writes none of its bones, and gets
//! it back to go on where it stood.

use bevy::animation::graph::AnimationGraphHandle;
use bevy::animation::{AnimationPlayer, AnimationTargetId};
use bevy::camera::{ImageRenderTarget, RenderTarget};
use bevy::image::Image;
use bevy::prelude::*;
use bevy::render::render_resource::TextureFormat;
use bevy::world_serialization::WorldAssetRoot;
use bevy_ts_animation::{PlayAnimation, SharedGraph};
use crate::common::*;

fn has_graph(app: &App, root: Entity) -> Vec<bool> {
    targets(app.world(), root).iter().map(|p| app.world().get::<AnimationGraphHandle>(*p).is_some()).collect()
}

fn parked(app: &App, root: Entity) -> bool {
    let g = has_graph(app, root);
    assert!(!g.is_empty(), "the root has players");
    assert!(g.iter().all(|h| *h == g[0]), "every player alike: {g:?}");
    !g[0]
}

fn set(app: &mut App, root: Entity, f: impl FnOnce(&mut PlayAnimation)) {
    f(&mut app.world_mut().get_mut::<PlayAnimation>(root).unwrap());
}

fn seek(app: &mut App, root: Entity) -> f32 {
    let run = node(app, RUN);
    player(app, root).animation(run).unwrap().seek_time()
}

/// Every bone under the root and its `Transform`.
fn bones(world: &mut World, root: Entity) -> Vec<(Entity, Transform)> {
    let mut out = vec![];
    let mut stack = vec![root];
    while let Some(e) = stack.pop() {
        if world.get::<AnimationTargetId>(e).is_some() {
            out.push((e, *world.get::<Transform>(e).unwrap()));
        }
        if let Some(c) = world.get::<Children>(e) {
            stack.extend(c.iter());
        }
    }
    out
}

fn steps(app: &mut App, n: usize) {
    for _ in 0..n {
        app.update();
    }
}

#[test]
fn paused_parks_and_resumes_where_it_stood() {
    let mut app = app();
    let root = fox(&mut app, RUN);
    ready(&mut app, root);
    steps(&mut app, 30);
    assert!(!parked(&app, root));
    set(&mut app, root, |p| p.paused = true);
    app.update();
    assert!(parked(&app, root), "no player keeps the graph handle");
    assert!(player(&app, root).all_paused(), "Bevy's own pause state matches");
    let at = seek(&mut app, root);
    assert!(at > 0.0);
    let before = bones(app.world_mut(), root);
    assert!(!before.is_empty());
    let tick = app.world_mut().change_tick();
    steps(&mut app, 10);
    assert_eq!(bones(app.world_mut(), root), before, "no bone moved");
    for (b, _) in &before {
        let changed = app.world().entity(*b).get_ref::<Transform>().unwrap().last_changed();
        assert!(!changed.is_newer_than(tick, app.world().read_change_tick()), "bone {b} was written while parked");
    }
    assert_eq!(seek(&mut app, root), at, "the clip's time stood still");

    set(&mut app, root, |p| p.paused = false);
    app.update();
    assert!(!parked(&app, root));
    let graph = app.world().resource::<SharedGraph>().handle.clone();
    assert_eq!(app.world().get::<AnimationGraphHandle>(targets(app.world(), root)[0]).unwrap().0, graph);
    assert!(!player(&app, root).all_paused());
    steps(&mut app, 3);
    let now = seek(&mut app, root);
    assert!(now > at && now < at + 0.1, "plays on from {at}: {now}");
}

#[test]
fn speed_zero_does_not_park() {
    let mut app = app();
    let root = fox(&mut app, RUN);
    ready(&mut app, root);
    app.update();
    set(&mut app, root, |p| p.speed = 0.0);
    steps(&mut app, 3);
    assert!(!parked(&app, root));
    let run = node(&mut app, RUN);
    assert!(!player(&app, root).animation(run).unwrap().is_paused());
}

#[test]
fn spawned_paused() {
    let mut app = app();
    let scene = load(&app, SCENE);
    let clip = load(&app, RUN);
    let root = app.world_mut().spawn((WorldAssetRoot(scene), PlayAnimation { clip, paused: true, ..default() })).id();
    ready(&mut app, root);
    assert!(parked(&app, root));
    steps(&mut app, 5);
    let run = node(&mut app, RUN);
    let a = player(&app, root).animation(run).expect("the run clip is the root's");
    assert!(a.is_paused());
    assert_eq!(a.seek_time(), 0.0);
}

#[test]
fn removing_a_parked_root_restores_the_handle() {
    let mut app = app();
    let root = fox(&mut app, RUN);
    ready(&mut app, root);
    set(&mut app, root, |p| p.paused = true);
    app.update();
    let players = targets(app.world(), root);
    assert!(parked(&app, root));
    app.world_mut().entity_mut(root).remove::<PlayAnimation>();
    app.update();
    for p in players {
        assert!(app.world().get::<AnimationGraphHandle>(p).is_some(), "player {p} has its handle back");
        assert_eq!(app.world().get::<AnimationPlayer>(p).unwrap().playing_animations().count(), 0);
    }
}

/// A camera rendering to a small image, at `from` looking at the origin, where the foxes stand.
fn camera(app: &mut App, from: Vec3) -> Entity {
    let image = Image::new_target_texture(64, 64, TextureFormat::Rgba8UnormSrgb, None);
    let image = app.world_mut().resource_mut::<Assets<Image>>().add(image);
    app.world_mut()
        .spawn((Camera3d::default(), RenderTarget::Image(ImageRenderTarget::from(image)), Transform::from_translation(from).looking_at(Vec3::ZERO, Vec3::Y)))
        .id()
}

fn look(app: &mut App, camera: Entity, at: Vec3) {
    let mut t = app.world_mut().get_mut::<Transform>(camera).unwrap();
    *t = t.looking_at(at, Vec3::Y);
}

const EYE: Vec3 = Vec3::new(0.0, 80.0, 300.0);
const AWAY: Vec3 = Vec3::new(0.0, 80.0, 600.0);

#[test]
fn culled_roots_park_off_screen() {
    let mut app = app();
    let cam = camera(&mut app, EYE);
    let culled = fox(&mut app, RUN);
    let kept = fox(&mut app, RUN);
    set(&mut app, culled, |p| p.cull = true);
    ready(&mut app, culled);
    ready(&mut app, kept);
    steps(&mut app, 3);
    assert!(!parked(&app, culled), "in view: plays");

    look(&mut app, cam, AWAY);
    steps(&mut app, 3);
    assert!(parked(&app, culled), "out of every view: parked");
    assert!(!parked(&app, kept), "cull false: plays on");

    look(&mut app, cam, Vec3::ZERO);
    steps(&mut app, 3);
    assert!(!parked(&app, culled), "back in view: wakes");
}

#[test]
fn paused_while_culled_stays_parked() {
    let mut app = app();
    let cam = camera(&mut app, EYE);
    look(&mut app, cam, AWAY);
    let root = fox(&mut app, RUN);
    set(&mut app, root, |p| {
        p.cull = true;
        p.paused = true;
    });
    ready(&mut app, root);
    steps(&mut app, 3);
    assert!(parked(&app, root));
    look(&mut app, cam, Vec3::ZERO);
    steps(&mut app, 3);
    assert!(parked(&app, root), "in view but paused: still parked");
    set(&mut app, root, |p| p.paused = false);
    steps(&mut app, 2);
    assert!(!parked(&app, root), "unpaused in view: wakes");
}
