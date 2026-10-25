//! Scene animation (openspec change `scene-animation`): [`PlayAnimation`] beside a
//! `WorldAssetRoot` names a glTF clip, and Rust plays it on the scene's animation players the way
//! Bevy's `many_foxes` does: once the scene is ready, every player gets the shared graph and
//! `AnimationTransitions`, and a changed clip cross-fades. No JS runs for it.
//!
//! A root that is `paused`, or `cull`ed while none of its meshes is in view, is parked (change
//! `animation-control`): its players lose `AnimationGraphHandle`, so Bevy samples and writes
//! nothing for them, and get it back to go on where they stood. [`Animations`] reads a root's clip
//! from TS.
//!
//! A root with a `graph` plays that `AnimationGraph` in place of the shared one: every clip node
//! in it, blended by its weights and masks (stage 2 of `animation-control`).

use bevy::animation::graph::{AnimationGraph, AnimationGraphHandle, AnimationNodeIndex, AnimationNodeType};
use bevy::animation::transition::AnimationTransitions;
use bevy::animation::{ActiveAnimation, AnimationClip, AnimationPlayer, RepeatAnimation};
use bevy::app::AnimationSystems;
use bevy::asset::{AssetLoadFailedEvent, AssetPath};
use bevy::gltf::Gltf;
use bevy::ecs::entity::EntityHashSet;
use bevy::platform::collections::{HashMap, HashSet};
use bevy::prelude::*;
use bevy::ecs::system::SystemParam;
use bevy::world_serialization::{WorldAssetRoot, WorldInstance, WorldInstanceReady, WorldInstanceSpawner};
use bevy_ts_native::{BevyTsNativeExt, HostRecord, export};
use std::time::Duration;

/// Plays a glTF clip on the scene under this entity, which carries `WorldAssetRoot`. A changed
/// `clip` cross-fades over `transition_secs`; a changed `speed` or `repeat` applies to the clip
/// already playing. With a `graph`, plays every clip node of that graph instead. Removing it
/// stops the players.
#[derive(Component, Reflect, Clone, Debug, PartialEq)]
#[reflect(Component, Default, Clone, Debug)]
pub struct PlayAnimation {
    /// The clip, written as an asset path with a label (`"models/Fox.glb#Animation2"`).
    pub clip: Handle<AnimationClip>,
    /// An `AnimationGraph` to play in place of `clip`: every clip node in it, with `repeat` and
    /// `speed`, blended as Bevy blends them. A built graph from TS, or a `.animgraph.ron` path.
    /// `clip` and `transition_secs` are unused while it is set; a changed graph restarts.
    pub graph: Handle<AnimationGraph>,
    /// Bevy's `RepeatAnimation`: `Never`, `Count(n)` or `Forever` (the default).
    pub repeat: RepeatAnimation,
    /// Bevy's `set_speed`: negative plays backwards, 0 holds the pose and still costs a playing
    /// clip (use `paused`).
    pub speed: f32,
    pub transition_secs: f32,
    /// Pauses the players and parks them: no bone is sampled or written until it is false again,
    /// when the clip goes on where it stood.
    pub paused: bool,
    /// Parks the root while none of its meshes is in any view, and wakes it the frame after one
    /// is. A parked clip's time stands still.
    pub cull: bool,
}

impl Default for PlayAnimation {
    fn default() -> Self {
        Self { clip: Handle::default(), graph: Handle::default(), repeat: RepeatAnimation::Forever, speed: 1.0, transition_secs: 0.25, paused: false, cull: false }
    }
}

/// What a [`PlayAnimation`] root drives, found when its scene was ready: its animation players,
/// the meshes whose visibility `cull` reads, and the graph the players were given ([`graph_of`]).
#[derive(Component, Clone, Debug, Default)]
pub struct AnimationTargets {
    pub players: Vec<Entity>,
    pub meshes: Vec<Entity>,
    pub graph: Handle<AnimationGraph>,
}

/// The graph a root's players play: its own `graph`, else the shared one. The one place this is
/// decided; [`AnimationTargets::graph`] records it for parking and removal.
pub fn graph_of(play: &PlayAnimation, shared: &SharedGraph) -> Handle<AnimationGraph> {
    if is_graph_root(play) { play.graph.clone() } else { shared.handle.clone() }
}

/// The root plays its own graph.
fn is_graph_root(play: &PlayAnimation) -> bool {
    play.graph != Handle::default()
}

/// The clip nodes of a graph, in node order.
pub fn clip_nodes(graph: &AnimationGraph) -> impl Iterator<Item = (AnimationNodeIndex, &Handle<AnimationClip>)> {
    graph.nodes().filter_map(|n| match &graph.get(n)?.node_type {
        AnimationNodeType::Clip(c) => Some((n, c)),
        _ => None,
    })
}

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

    /// The clip's node, if any root has named it.
    pub fn node_of(&self, clip: &Handle<AnimationClip>) -> Option<AnimationNodeIndex> {
        self.nodes.get(&clip.id()).copied()
    }

    /// How many clips the graph holds.
    pub fn clips(&self) -> usize {
        self.nodes.len()
    }
}

/// What was logged once: roots warned about having no players or about both `clip` and `graph`,
/// and clip paths that failed to load, with how many lines each kind took.
#[derive(Resource, Debug, Default)]
pub struct AnimationReports {
    pub roots: EntityHashSet,
    pub both: EntityHashSet,
    pub clips: HashSet<String>,
    pub warnings: usize,
    pub errors: usize,
}

/// The animation player entities under `root`, the root included.
pub fn players(world: &World, root: Entity) -> Vec<Entity> {
    under(world, root, |e| world.get::<AnimationPlayer>(e).is_some())
}

/// The mesh entities under `root`, the root included: what `cull` reads the visibility of.
pub fn meshes(world: &World, root: Entity) -> Vec<Entity> {
    under(world, root, |e| world.get::<Mesh3d>(e).is_some())
}

/// The entities under `root`, the root included, that `keep` keeps.
fn under(world: &World, root: Entity, keep: impl Fn(Entity) -> bool) -> Vec<Entity> {
    let mut out = vec![];
    let mut stack = vec![root];
    while let Some(e) = stack.pop() {
        if keep(e) {
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
            .add_systems(PostUpdate, (stop_removed, apply_changed, edited_graphs, park, report_failed_clips).chain().before(AnimationSystems))
            .ts_param::<Animations<'static, 'static>>();
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
/// and records the players and meshes on the root. A paused root then pauses and parks.
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
    let graph = graph_of(&play, world.resource::<SharedGraph>());
    warn_both(world, root, &play);
    for &p in &found {
        world.entity_mut(p).insert((AnimationGraphHandle(graph.clone()), AnimationTransitions::new()));
        // A reload's players start fresh: nothing stays from the old scene.
        if let Some(mut player) = world.get_mut::<AnimationPlayer>(p) {
            player.stop_all();
        }
    }
    let meshes = meshes(world, root);
    world.entity_mut(root).insert(AnimationTargets { players: found, meshes, graph });
    world.resource_scope(|world, mut shared: Mut<SharedGraph>| {
        world.resource_scope(|world, mut graphs: Mut<Assets<AnimationGraph>>| {
            let node = if is_graph_root(&play) { None } else { shared.node(&mut graphs, &play.clip) };
            let targets = world.get::<AnimationTargets>(root).cloned().unwrap_or_default();
            for p in targets.players {
                if let Ok(mut e) = world.get_entity_mut(p)
                    && let Some((mut player, mut transitions)) = e.get_components_mut::<(&mut AnimationPlayer, &mut AnimationTransitions)>().ok()
                {
                    match graphs.get(&play.graph).filter(|_| is_graph_root(&play)) {
                        Some(graph) => play_graph(&play, graph, &mut player),
                        // A graph not loaded yet plays when it is ([`edited_graphs`]).
                        None if is_graph_root(&play) => {}
                        None => apply(&play, node, &mut player, &mut transitions),
                    }
                    if play.paused {
                        player.pause_all();
                    }
                }
                if play.paused
                    && let Ok(mut e) = world.get_entity_mut(p)
                {
                    e.remove::<AnimationGraphHandle>();
                }
            }
        });
    });
}

/// Brings one player to what `play` asks: a new clip cross-fades in, the same clip takes the
/// speed and repeat in place, no clip stops it.
fn apply(play: &PlayAnimation, node: Option<AnimationNodeIndex>, player: &mut AnimationPlayer, transitions: &mut AnimationTransitions) {
    let configure = |a: &mut ActiveAnimation| {
        a.set_speed(play.speed).set_repeat(play.repeat);
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

/// Brings one player to a graph root's `play`: every clip node in `graph` playing with its speed
/// and repeat, and nothing else.
fn play_graph(play: &PlayAnimation, graph: &AnimationGraph, player: &mut AnimationPlayer) {
    let nodes: Vec<AnimationNodeIndex> = clip_nodes(graph).map(|(n, _)| n).collect();
    let gone: Vec<AnimationNodeIndex> = player.playing_animations().map(|(n, _)| *n).filter(|n| !nodes.contains(n)).collect();
    for n in gone {
        player.stop(n);
    }
    for n in nodes {
        player.play(n).set_speed(play.speed).set_repeat(play.repeat);
    }
}

/// A root with both `clip` and `graph` set is warned about once: it plays the graph.
fn warn_both(world: &mut World, root: Entity, play: &PlayAnimation) {
    if !(is_graph_root(play) && play.clip != Handle::default()) {
        return;
    }
    let mut reports = world.resource_mut::<AnimationReports>();
    if reports.both.insert(root) {
        reports.warnings += 1;
        let clip = play.clip.path().map_or_else(|| "?".into(), |p| p.to_string());
        warn!("PlayAnimation on {root}: both clip ({clip}) and graph are set; it plays the graph, and clip is unused");
    }
}

/// Applies changed (and added) `PlayAnimation`s: to the recorded players, or by walking a root
/// whose scene was already ready. A root whose scene is not ready waits for [`on_ready`].
#[allow(clippy::type_complexity)]
fn apply_changed(
    mut roots: Query<(Entity, &PlayAnimation, Option<&mut AnimationTargets>, Option<&WorldInstance>), Changed<PlayAnimation>>,
    mut players: Query<(&mut AnimationPlayer, &mut AnimationTransitions)>,
    spawner: Option<Res<WorldInstanceSpawner>>,
    server: Res<AssetServer>,
    mut shared: ResMut<SharedGraph>,
    mut graphs: ResMut<Assets<AnimationGraph>>,
    mut commands: Commands,
) {
    for (root, play, targets, instance) in &mut roots {
        if is_graph_root(play) && play.clip != Handle::default() {
            let play = play.clone();
            commands.queue(move |world: &mut World| warn_both(world, root, &play));
        } else {
            shared.hold_file(&server, &play.clip);
        }
        match targets {
            Some(mut targets) => {
                // A changed graph (or a graph set or cleared) restarts on the new one, no fade.
                let graph = graph_of(play, &shared);
                let restart = graph != targets.graph;
                if restart {
                    targets.graph = graph.clone();
                }
                let node = if is_graph_root(play) { None } else { shared.node(&mut graphs, &play.clip) };
                for &p in &targets.players {
                    let Ok((mut player, mut transitions)) = players.get_mut(p) else { continue };
                    if restart {
                        player.stop_all();
                        *transitions = AnimationTransitions::new();
                        // A parked root loses it again in `park`, this frame.
                        commands.entity(p).try_insert(AnimationGraphHandle(graph.clone()));
                    }
                    match graphs.get(&play.graph).filter(|_| is_graph_root(play)) {
                        Some(g) => play_graph(play, g, &mut player),
                        None if is_graph_root(play) => {}
                        None => apply(play, node, &mut player, &mut transitions),
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

/// A graph a root plays was loaded or changed (`assets.set`, a hot reload): its roots start the
/// clip nodes not yet playing and stop the ones gone. Weights and masks need nothing: Bevy reads
/// them from the asset each frame.
fn edited_graphs(
    mut events: MessageReader<AssetEvent<AnimationGraph>>,
    roots: Query<(&PlayAnimation, &AnimationTargets)>,
    mut players: Query<&mut AnimationPlayer>,
    graphs: Res<Assets<AnimationGraph>>,
) {
    let ids: HashSet<AssetId<AnimationGraph>> = events
        .read()
        .filter_map(|e| match e {
            AssetEvent::Added { id } | AssetEvent::Modified { id } | AssetEvent::LoadedWithDependencies { id } => Some(*id),
            _ => None,
        })
        .collect();
    if ids.is_empty() {
        return;
    }
    for (play, targets) in &roots {
        if !is_graph_root(play) || !ids.contains(&play.graph.id()) || targets.graph != play.graph {
            continue;
        }
        let Some(graph) = graphs.get(&play.graph) else { continue };
        for &p in &targets.players {
            if let Ok(mut player) = players.get_mut(p) {
                play_graph(play, graph, &mut player);
            }
        }
    }
}

/// A root that lost `PlayAnimation` (or was despawned) stops its players, gives back the graph
/// handle parking took, and is forgotten by the once-per-root warning, so a new root reusing its
/// entity is warned about in turn.
fn stop_removed(
    mut removed: RemovedComponents<PlayAnimation>,
    targets: Query<&AnimationTargets>,
    mut players: Query<(&mut AnimationPlayer, &mut AnimationTransitions, Has<AnimationGraphHandle>)>,
    mut reports: ResMut<AnimationReports>,
    mut commands: Commands,
) {
    for root in removed.read() {
        reports.roots.remove(&root);
        reports.both.remove(&root);
        let Ok(t) = targets.get(root) else { continue };
        for &p in &t.players {
            if let Ok((mut player, mut transitions, graph)) = players.get_mut(p) {
                player.stop_all();
                *transitions = AnimationTransitions::new();
                if !graph {
                    commands.entity(p).try_insert(AnimationGraphHandle(t.graph.clone()));
                }
            }
        }
        commands.entity(root).try_remove::<AnimationTargets>();
    }
}

/// Parks or wakes roots (design 2): `paused`, or `cull` with meshes none of which was in a view
/// last frame, parks. Only roots with either flag set, or whose `PlayAnimation` or targets just
/// changed, are looked at; a root's state is whether its first player has the graph handle.
#[allow(clippy::type_complexity)]
fn park(
    roots: Query<(Ref<PlayAnimation>, Ref<AnimationTargets>)>,
    mut players: Query<(&mut AnimationPlayer, Has<AnimationGraphHandle>)>,
    seen: Query<&ViewVisibility>,
    mut commands: Commands,
) {
    for (play, targets) in &roots {
        let changed = play.is_changed() || targets.is_changed();
        if !(play.paused || play.cull || changed) {
            continue;
        }
        if changed {
            for &p in &targets.players {
                if let Ok((mut player, _)) = players.get_mut(p) {
                    if play.paused && !player.all_paused() {
                        player.pause_all();
                    } else if !play.paused && player.playing_animations().any(|(_, a)| a.is_paused()) {
                        player.resume_all();
                    }
                }
            }
        }
        // A root whose players were just found has meshes Bevy has not judged in view yet: it
        // plays its first frame, and `cull` judges it from the next.
        let culled = play.cull
            && !targets.is_added()
            && !targets.meshes.is_empty()
            && !targets.meshes.iter().any(|&m| seen.get(m).is_ok_and(|v| v.get()));
        let parked = play.paused || culled;
        let Some(&first) = targets.players.first() else { continue };
        let Ok((_, graph)) = players.get(first) else { continue };
        if graph != parked {
            continue;
        }
        for &p in &targets.players {
            if parked {
                commands.entity(p).try_remove::<AnimationGraphHandle>();
            } else {
                commands.entity(p).try_insert(AnimationGraphHandle(targets.graph.clone()));
            }
        }
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

/// A root's clip as its first player has it: `Animations.status` from TS.
#[derive(HostRecord, Clone, Debug, PartialEq)]
pub struct AnimationStatus {
    /// The clip asset is loaded.
    pub loaded: bool,
    /// The clip's length in seconds; 0 until it is loaded.
    pub duration: f32,
    pub seek_time: f32,
    pub elapsed: f32,
    pub completions: u32,
    pub just_completed: bool,
    pub finished: bool,
    pub paused: bool,
    /// The root is parked: no bone is sampled or written.
    pub parked: bool,
    pub weight: f32,
}

/// Reads and drives a root's clip from TS (`bevy-ts/animation`), keyed by the root that carries
/// `PlayAnimation`, which names its clip by asset path. No node index reaches TS. On a graph root,
/// `status` reads the graph's first clip node, and the writers act on every clip it plays.
#[derive(SystemParam)]
pub struct Animations<'w, 's> {
    roots: Query<'w, 's, (&'static PlayAnimation, &'static AnimationTargets)>,
    players: Query<'w, 's, (&'static mut AnimationPlayer, Has<AnimationGraphHandle>)>,
    shared: Res<'w, SharedGraph>,
    graphs: Res<'w, Assets<AnimationGraph>>,
    clips: Res<'w, Assets<AnimationClip>>,
}

#[export(ts = "Animations", module = "bevy-ts/animation")]
impl Animations<'_, '_> {
    /// The root's clip, or `undefined` for an entity that is not a `PlayAnimation` root or whose
    /// scene has no player yet.
    pub fn status(&self, root: Entity) -> Option<AnimationStatus> {
        let (play, targets) = self.roots.get(root).ok()?;
        let (player, graph) = self.players.get(*targets.players.first()?).ok()?;
        let (node, clip) = self.nodes(play).into_iter().next()?;
        let a = player.animation(node)?;
        let clip = self.clips.get(clip);
        Some(AnimationStatus {
            loaded: clip.is_some(),
            duration: clip.map_or(0.0, |c| c.duration()),
            seek_time: a.seek_time(),
            elapsed: a.elapsed(),
            completions: a.completions(),
            just_completed: a.just_completed(),
            finished: a.is_finished(),
            paused: a.is_paused(),
            parked: !graph,
            weight: a.weight(),
        })
    }

    /// Bevy's `seek_to` on the root's clip on every player under it: events between the old
    /// time and `secs` fire on the next update. A parked root's clip moves, and shows once it
    /// wakes. False, changing nothing, where `status` is `undefined`.
    pub fn seek(&mut self, root: Entity, secs: f32) -> bool {
        self.each(root, |a| {
            a.seek_to(secs);
        })
    }

    /// Bevy's `rewind` on the root's clip on every player under it.
    pub fn rewind(&mut self, root: Entity) -> bool {
        self.each(root, |a| {
            a.rewind();
        })
    }

    /// Bevy's `replay` on the root's clip on every player under it: back to no time elapsed and
    /// no completions.
    pub fn replay(&mut self, root: Entity) -> bool {
        self.each(root, ActiveAnimation::replay)
    }
}

impl Animations<'_, '_> {
    /// The root's clip nodes and their clips: its graph's clip nodes in node order, else its
    /// clip's node in the shared graph.
    fn nodes(&self, play: &PlayAnimation) -> Vec<(AnimationNodeIndex, AssetId<AnimationClip>)> {
        if is_graph_root(play) {
            return self.graphs.get(&play.graph).map(|g| clip_nodes(g).map(|(n, c)| (n, c.id())).collect()).unwrap_or_default();
        }
        self.shared.node_of(&play.clip).map(|n| (n, play.clip.id())).into_iter().collect()
    }

    /// Runs `f` on the root's clips on every player under it; false where `status` is `None`.
    fn each(&mut self, root: Entity, f: impl Fn(&mut ActiveAnimation)) -> bool {
        if self.status(root).is_none() {
            return false;
        }
        let Ok((play, targets)) = self.roots.get(root) else { return false };
        let nodes = self.nodes(play);
        for &p in &targets.players {
            if let Ok((mut player, _)) = self.players.get_mut(p) {
                for (node, _) in &nodes {
                    if let Some(a) = player.animation_mut(*node) {
                        f(a);
                    }
                }
            }
        }
        true
    }
}
