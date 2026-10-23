//! Scene animation (openspec change `scene-animation`): [`PlayAnimation`] beside a
//! `WorldAssetRoot` names a glTF clip, and Rust plays it on the scene's animation players the way
//! Bevy's `many_foxes` does: once the scene is ready, every player gets the shared graph and
//! `AnimationTransitions`, and a changed clip cross-fades. No JS runs for it.

use bevy::animation::graph::{AnimationGraph, AnimationGraphHandle, AnimationNodeIndex};
use bevy::animation::transition::AnimationTransitions;
use bevy::animation::{ActiveAnimation, AnimationClip, AnimationPlayer, RepeatAnimation};
use bevy::app::AnimationSystems;
use bevy::asset::{AssetLoadFailedEvent, AssetPath};
use bevy::gltf::Gltf;
use bevy::ecs::entity::EntityHashSet;
use bevy::platform::collections::{HashMap, HashSet};
use bevy::prelude::*;
use bevy::world_serialization::{WorldAssetRoot, WorldInstance, WorldInstanceReady, WorldInstanceSpawner};
use std::time::Duration;

/// Plays a glTF clip on the scene under this entity, which carries `WorldAssetRoot`. A changed
/// `clip` cross-fades over `transition_secs`; a changed `speed` or `repeat` applies to the clip
/// already playing. Removing it stops the players.
#[derive(Component, Reflect, Clone, Debug, PartialEq)]
#[reflect(Component, Default, Clone, Debug)]
pub struct PlayAnimation {
    /// The clip, written as an asset path with a label (`"models/Fox.glb#Animation2"`).
    pub clip: Handle<AnimationClip>,
    /// Forever when true, once when false.
    pub repeat: bool,
    pub speed: f32,
    pub transition_secs: f32,
}

impl Default for PlayAnimation {
    fn default() -> Self {
        Self { clip: Handle::default(), repeat: true, speed: 1.0, transition_secs: 0.25 }
    }
}

/// The animation players a [`PlayAnimation`] root drives, found when its scene was ready.
#[derive(Component, Clone, Debug, Default)]
pub struct AnimationTargets(pub Vec<Entity>);

/// The one graph every player shares, with a node per distinct clip, added on first use, and the
/// glTF files those clips come from, held loaded. Neither ever shrinks.
#[derive(Resource, Debug)]
pub struct SharedGraph {
    pub handle: Handle<AnimationGraph>,
    nodes: HashMap<AssetId<AnimationClip>, AnimationNodeIndex>,
    /// Without its file held, a clip named later (`#Animation1` after `#Animation2`) loads the
    /// file again, and every scene from it spawns again
    /// (<https://github.com/bevyengine/bevy/issues/10549>).
    files: HashMap<AssetPath<'static>, Handle<Gltf>>,
}

impl FromWorld for SharedGraph {
    fn from_world(world: &mut World) -> Self {
        let handle = world.resource_mut::<Assets<AnimationGraph>>().add(AnimationGraph::new());
        Self { handle, nodes: HashMap::default(), files: HashMap::default() }
    }
}

impl SharedGraph {
    /// The clip's node, added to the graph the first time any root names it.
    pub fn node(&mut self, graphs: &mut Assets<AnimationGraph>, clip: &Handle<AnimationClip>) -> Option<AnimationNodeIndex> {
        if let Some(n) = self.nodes.get(&clip.id()) {
            return Some(*n);
        }
        let mut graph = graphs.get_mut(&self.handle)?;
        let root = graph.root;
        let n = graph.add_clip(clip.clone(), 1.0, root);
        self.nodes.insert(clip.id(), n);
        Some(n)
    }

    /// Holds the glTF file the clip comes from loaded, from the first time a root names it.
    pub fn hold_file(&mut self, server: &AssetServer, clip: &Handle<AnimationClip>) {
        let Some(path) = clip.path() else { return };
        let file = path.without_label();
        let gltf = file.get_full_extension().is_some_and(|e| e == "glb" || e == "gltf");
        if gltf && !self.files.contains_key(&file) {
            let file = file.clone_owned();
            self.files.insert(file.clone(), server.load(file));
        }
    }

    /// How many clips the graph holds.
    pub fn clips(&self) -> usize {
        self.nodes.len()
    }
}

/// What was logged once: roots warned about having no players, and clip paths that failed to
/// load, with how many lines each kind took.
#[derive(Resource, Debug, Default)]
pub struct AnimationReports {
    pub roots: EntityHashSet,
    pub clips: HashSet<String>,
    pub warnings: usize,
    pub errors: usize,
}

/// The animation player entities under `root`, the root included.
pub fn players(world: &World, root: Entity) -> Vec<Entity> {
    let mut out = vec![];
    let mut stack = vec![root];
    while let Some(e) = stack.pop() {
        if world.get::<AnimationPlayer>(e).is_some() {
            out.push(e);
        }
        if let Some(c) = world.get::<Children>(e) {
            stack.extend(c.iter());
        }
    }
    out
}

/// [`players`] for a system.
pub fn players_in(root: Entity, children: &Query<&Children>, players: &Query<(), With<AnimationPlayer>>) -> Vec<Entity> {
    std::iter::once(root).chain(children.iter_descendants(root)).filter(|e| players.contains(*e)).collect()
}

/// Plays `PlayAnimation` on scene roots.
pub struct PlayAnimationPlugin;

impl Plugin for PlayAnimationPlugin {
    fn build(&self, app: &mut App) {
        app.register_type::<PlayAnimation>()
            .init_resource::<SharedGraph>()
            .init_resource::<AnimationReports>()
            .add_observer(on_ready)
            .add_systems(PostUpdate, (stop_removed, apply_changed, report_failed_clips).chain().before(AnimationSystems));
    }
}

/// The scene under a root is ready (spawned, or spawned again after a reload): play on its
/// players.
fn on_ready(ready: On<WorldInstanceReady>, roots: Query<(), With<PlayAnimation>>, mut commands: Commands) {
    let root = ready.entity;
    if roots.contains(root) {
        commands.queue(move |world: &mut World| start(world, root));
    }
}

/// Finds the root's players, gives each the shared graph and fresh transitions, plays the clip
/// and records the players on the root.
fn start(world: &mut World, root: Entity) {
    let Some(play) = world.get::<PlayAnimation>(root).cloned() else { return };
    let found = players(world, root);
    if found.is_empty() {
        let mut reports = world.resource_mut::<AnimationReports>();
        if reports.roots.insert(root) {
            reports.warnings += 1;
            let scene = world.get::<WorldAssetRoot>(root).and_then(|s| s.0.path()).map_or_else(|| "?".into(), |p| p.to_string());
            let clip = play.clip.path().map_or_else(|| "none".into(), |p| p.to_string());
            warn!(
                "PlayAnimation on {root}: scene {scene} has no AnimationPlayer, so clip {clip} plays on nothing. A player exists only where the scene's own glTF file has animations"
            );
        }
    }
    let graph = world.resource::<SharedGraph>().handle.clone();
    for &p in &found {
        world.entity_mut(p).insert((AnimationGraphHandle(graph.clone()), AnimationTransitions::new()));
        // A reload's players start fresh: nothing stays from the old scene.
        if let Some(mut player) = world.get_mut::<AnimationPlayer>(p) {
            player.stop_all();
        }
    }
    world.entity_mut(root).insert(AnimationTargets(found));
    world.resource_scope(|world, mut shared: Mut<SharedGraph>| {
        world.resource_scope(|world, mut graphs: Mut<Assets<AnimationGraph>>| {
            let node = shared.node(&mut graphs, &play.clip);
            let targets = world.get::<AnimationTargets>(root).cloned().unwrap_or_default();
            for p in targets.0 {
                if let Ok(mut e) = world.get_entity_mut(p)
                    && let Some((mut player, mut transitions)) = e.get_components_mut::<(&mut AnimationPlayer, &mut AnimationTransitions)>().ok()
                {
                    apply(&play, node, &mut player, &mut transitions);
                }
            }
        });
    });
}

/// Brings one player to what `play` asks: a new clip cross-fades in, the same clip takes the
/// speed and repeat in place, no clip stops it.
fn apply(play: &PlayAnimation, node: Option<AnimationNodeIndex>, player: &mut AnimationPlayer, transitions: &mut AnimationTransitions) {
    let configure = |a: &mut ActiveAnimation| {
        a.set_speed(play.speed);
        if play.repeat { a.repeat() } else { a.set_repeat(RepeatAnimation::Never) };
    };
    let Some(node) = node.filter(|_| play.clip != Handle::default()) else {
        player.stop_all();
        *transitions = AnimationTransitions::new();
        return;
    };
    if transitions.get_main_animation() == Some(node) && player.is_playing_animation(node) {
        if let Some(a) = player.animation_mut(node) {
            configure(a);
        }
        return;
    }
    let fade = if play.transition_secs > 0.0 && play.transition_secs.is_finite() { Duration::from_secs_f32(play.transition_secs) } else { Duration::ZERO };
    configure(transitions.play(player, node, fade));
}

/// Applies changed (and added) `PlayAnimation`s: to the recorded players, or by walking a root
/// whose scene was already ready. A root whose scene is not ready waits for [`on_ready`].
#[allow(clippy::type_complexity)]
fn apply_changed(
    roots: Query<(Entity, &PlayAnimation, Option<&AnimationTargets>, Option<&WorldInstance>), Changed<PlayAnimation>>,
    mut players: Query<(&mut AnimationPlayer, &mut AnimationTransitions)>,
    spawner: Option<Res<WorldInstanceSpawner>>,
    server: Res<AssetServer>,
    mut shared: ResMut<SharedGraph>,
    mut graphs: ResMut<Assets<AnimationGraph>>,
    mut commands: Commands,
) {
    for (root, play, targets, instance) in &roots {
        shared.hold_file(&server, &play.clip);
        match targets {
            Some(targets) => {
                let node = shared.node(&mut graphs, &play.clip);
                for &p in &targets.0 {
                    if let Ok((mut player, mut transitions)) = players.get_mut(p) {
                        apply(play, node, &mut player, &mut transitions);
                    }
                }
            }
            None if instance.is_some_and(|i| spawner.as_ref().is_some_and(|s| s.instance_is_ready(**i))) => {
                commands.queue(move |world: &mut World| start(world, root));
            }
            None => {}
        }
    }
}

/// A root that lost `PlayAnimation` (or was despawned) stops its players and is forgotten by the
/// once-per-root warning, so a new root reusing its entity is warned about in turn.
fn stop_removed(
    mut removed: RemovedComponents<PlayAnimation>,
    targets: Query<&AnimationTargets>,
    mut players: Query<(&mut AnimationPlayer, &mut AnimationTransitions)>,
    mut reports: ResMut<AnimationReports>,
    mut commands: Commands,
) {
    for root in removed.read() {
        reports.roots.remove(&root);
        let Ok(t) = targets.get(root) else { continue };
        for &p in &t.0 {
            if let Ok((mut player, mut transitions)) = players.get_mut(p) {
                player.stop_all();
                *transitions = AnimationTransitions::new();
            }
        }
        commands.entity(root).try_remove::<AnimationTargets>();
    }
}

/// A clip that fails to load is logged once per path.
fn report_failed_clips(mut failed: MessageReader<AssetLoadFailedEvent<AnimationClip>>, mut reported: ResMut<AnimationReports>) {
    for f in failed.read() {
        let path = f.path.to_string();
        if reported.clips.insert(path.clone()) {
            reported.errors += 1;
            error!("PlayAnimation: clip {path} failed to load: {}", f.error);
        }
    }
}
