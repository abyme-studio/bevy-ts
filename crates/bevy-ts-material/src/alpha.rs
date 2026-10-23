//! `MaterialAlpha` (openspec change `material-alpha`): fades what an entity draws. Its targets
//! are the entity's own `MeshMaterial3d<StandardMaterial>` and, on a glTF scene root, the meshes
//! its scene spawned. Originals are shared (every red cube, every instance of a model), so each
//! target draws with a copy owned by the entity, one per distinct original, edited in place as
//! `alpha` changes. Removing it puts the originals back and drops the copies.

use bevy::asset::AssetEventSystems;
use bevy::ecs::entity::EntityHashSet;
use bevy::platform::collections::HashMap;
use bevy::prelude::*;
use bevy::world_serialization::WorldInstanceReady;

use crate::scene::{SceneMaterialSet, SceneMaterialTargets, scene_entities};

/// Fades the entity's materials: each target draws with a copy whose `base_color` alpha is the
/// original's times `alpha` (clamped to 0..1), in `Blend` below 1 when the original was `Opaque`,
/// `Mask` or `AlphaToCoverage`. Remove it when not fading: while it is on, the entity's meshes no
/// longer batch with others sharing their material.
#[derive(Component, Reflect, Clone, Copy, Debug, PartialEq)]
#[reflect(Component, Default)]
pub struct MaterialAlpha {
    pub alpha: f32,
}

impl Default for MaterialAlpha {
    fn default() -> Self {
        Self { alpha: 1.0 }
    }
}

type Id = AssetId<StandardMaterial>;

/// What a faded entity owns: its copies, and the meshes it faded at the last sync.
#[derive(Component, Debug, Default)]
pub struct MaterialAlphaTargets {
    /// The meshes drawing one of the copies at the last sync.
    pub targets: Vec<Entity>,
    /// By original: the original and its copy.
    copies: HashMap<Id, (Handle<StandardMaterial>, Handle<StandardMaterial>)>,
    /// By copy: its original.
    originals: HashMap<Id, Id>,
}

impl MaterialAlphaTargets {
    /// The copies the entity owns.
    pub fn copies(&self) -> impl Iterator<Item = &Handle<StandardMaterial>> {
        self.copies.values().map(|(_, c)| c)
    }

    /// The original a copy was made from; `None` for a handle that is not one of its copies.
    pub fn original(&self, copy: Id) -> Option<&Handle<StandardMaterial>> {
        self.originals.get(&copy).map(|o| &self.copies[o].0)
    }
}

/// Faded roots whose scene became ready since the last `PostUpdate`.
#[derive(Resource, Default)]
struct ReadyRoots(EntityHashSet);

pub(crate) fn plugin(app: &mut App) {
    app.register_type::<MaterialAlpha>().init_resource::<ReadyRoots>().add_observer(on_ready).add_systems(
        PostUpdate,
        // After scene-material, so its swaps and restores are applied before this reads the meshes.
        changed.after(AssetEventSystems).after(SceneMaterialSet).run_if(|q: Query<(), Faded>| !q.is_empty()),
    );
}

/// A faded root's scene is ready (spawned, or spawned again after a reload): fade its meshes.
fn on_ready(ready: On<WorldInstanceReady>, faded: Query<(), With<MaterialAlpha>>, mut roots: ResMut<ReadyRoots>) {
    if faded.contains(ready.entity) {
        roots.0.insert(ready.entity);
    }
}

/// Faded, or faded until `MaterialAlpha` was removed: the system has work only with one.
type Faded = Or<(With<MaterialAlpha>, With<MaterialAlphaTargets>)>;

/// `MaterialAlpha` added or changed, or removed from an entity that still holds copies.
type Asks = Or<(Changed<MaterialAlpha>, (With<MaterialAlphaTargets>, Without<MaterialAlpha>))>;

/// The entities `entity` fades: itself, and the meshes of its scene instance (none before it is
/// ready), those that hold a `StandardMaterial`. TSX children are not in it; a child takes its own
/// `MaterialAlpha`. A mesh scene-material gave a script material holds none.
fn targets(world: &World, entity: Entity) -> Vec<Entity> {
    let mut out = vec![entity];
    out.extend(scene_entities(world, entity));
    out.retain(|e| world.get::<MeshMaterial3d<StandardMaterial>>(*e).is_some());
    out
}

/// What a copy of `original` draws with at `alpha`.
fn faded(original: &StandardMaterial, alpha: f32) -> (Color, AlphaMode) {
    let color = original.base_color.with_alpha(original.base_color.alpha() * alpha);
    let mode = match original.alpha_mode {
        AlphaMode::Opaque | AlphaMode::Mask(_) | AlphaMode::AlphaToCoverage if alpha < 1.0 => AlphaMode::Blend,
        m => m,
    };
    (color, mode)
}

fn copy_of(original: &StandardMaterial, alpha: f32) -> StandardMaterial {
    let (base_color, alpha_mode) = faded(original, alpha);
    StandardMaterial { base_color, alpha_mode, ..original.clone() }
}

/// What asks for a sync: `MaterialAlpha` added, changed or removed, a scene ready, a mesh given a
/// material (its owner is the nearest ancestor with `MaterialAlpha`), an original reloaded.
fn changed(
    asks: Query<Entity, Asks>,
    mut ready: ResMut<ReadyRoots>,
    given: Query<Entity, Changed<MeshMaterial3d<StandardMaterial>>>,
    owners: Query<(Entity, Option<&MaterialAlphaTargets>), With<MaterialAlpha>>,
    parents: Query<&ChildOf>,
    mut events: MessageReader<AssetEvent<StandardMaterial>>,
    mut commands: Commands,
) {
    let mut reloaded: Vec<(Entity, Id)> = Vec::new();
    for ev in events.read() {
        if let AssetEvent::Modified { id } = ev {
            reloaded.extend(owners.iter().filter(|(_, t)| t.is_some_and(|t| t.copies.contains_key(id))).map(|(e, _)| (e, *id)));
        }
    }
    for (e, id) in reloaded {
        commands.queue(move |world: &mut World| recopy(world, e, id));
    }
    let given = given.iter().filter_map(|e| std::iter::once(e).chain(parents.iter_ancestors(e)).find(|a| owners.contains(*a)));
    let mut seen = EntityHashSet::default();
    for e in asks.iter().chain(ready.0.drain()).chain(given) {
        if seen.insert(e) {
            commands.queue(move |world: &mut World| sync(world, e));
        }
    }
}

/// An original changed (a glTF saved again): its copy is made again from it.
fn recopy(world: &mut World, entity: Entity, original: Id) {
    let (Some(alpha), Some(targets)) = (world.get::<MaterialAlpha>(entity), world.get::<MaterialAlphaTargets>(entity)) else { return };
    let alpha = alpha.alpha.clamp(0.0, 1.0);
    let Some((o, c)) = targets.copies.get(&original).cloned() else { return };
    let mut assets = world.resource_mut::<Assets<StandardMaterial>>();
    let Some(new) = assets.get(&o).map(|o| copy_of(o, alpha)) else { return };
    if let Some(mut c) = assets.get_mut(&c) {
        *c = new;
    }
}

/// Brings `entity`'s targets to what it asks: each draws with the entity's copy of its original,
/// at the entity's alpha; with no `MaterialAlpha`, each gets its original back and the copies go.
pub fn sync(world: &mut World, entity: Entity) {
    let Ok(e) = world.get_entity(entity) else { return };
    let Some(alpha) = e.get::<MaterialAlpha>().map(|m| m.alpha.clamp(0.0, 1.0)) else {
        restore(world, entity);
        return;
    };
    // Edited in place, not taken and inserted: a bound alpha syncs every frame.
    let mut state = match world.get_mut::<MaterialAlphaTargets>(entity) {
        Some(mut t) => std::mem::take(&mut *t),
        None => default(),
    };
    let targets = targets(world, entity);
    for &t in &targets {
        let h = world.get::<MeshMaterial3d<StandardMaterial>>(t).expect("a target").0.clone();
        if state.originals.contains_key(&h.id()) {
            continue;
        }
        let copy = match state.copies.get(&h.id()) {
            Some((_, c)) => c.clone(),
            None => {
                let mut assets = world.resource_mut::<Assets<StandardMaterial>>();
                let Some(m) = assets.get(&h).map(|o| copy_of(o, alpha)) else { continue };
                let c = assets.add(m);
                state.originals.insert(c.id(), h.id());
                state.copies.insert(h.id(), (h, c.clone()));
                c
            }
        };
        world.entity_mut(t).insert(MeshMaterial3d(copy));
    }
    // A changed alpha: rewrite only the copies whose values differ, so an unchanged one is not
    // prepared again.
    let mut assets = world.resource_mut::<Assets<StandardMaterial>>();
    for (o, c) in state.copies.values() {
        let Some(want) = assets.get(o).map(|o| faded(o, alpha)) else { continue };
        if assets.get(c).is_some_and(|c| (c.base_color, c.alpha_mode) != want)
            && let Some(mut c) = assets.get_mut(c)
        {
            (c.base_color, c.alpha_mode) = want;
        }
    }
    state.targets = targets.into_iter().filter(|t| world.get::<MeshMaterial3d<StandardMaterial>>(*t).is_some_and(|m| state.originals.contains_key(&m.0.id()))).collect();
    match world.get_mut::<MaterialAlphaTargets>(entity) {
        Some(mut t) => *t = state,
        None => {
            world.entity_mut(entity).insert(state);
        }
    }
}

/// `MaterialAlpha` removed: every mesh still drawing one of the entity's copies draws its original
/// again, and dropping `MaterialAlphaTargets` frees the copies. A copy scene-material kept as a
/// mesh's own material, to put back when its script material goes, becomes the original.
fn restore(world: &mut World, entity: Entity) {
    let Some(state) = world.entity_mut(entity).take::<MaterialAlphaTargets>() else { return };
    if let Some(mut kept) = world.get_mut::<SceneMaterialTargets>(entity) {
        for (_, h) in kept.0.iter_mut() {
            if let Some(o) = state.original(h.id()) {
                *h = o.clone();
            }
        }
    }
    let mut candidates = targets(world, entity);
    candidates.extend(state.targets.iter().copied());
    for t in candidates {
        let Some(h) = world.get_entity(t).ok().and_then(|e| e.get::<MeshMaterial3d<StandardMaterial>>()) else { continue };
        if let Some(o) = state.original(h.0.id()).cloned() {
            world.entity_mut(t).insert(MeshMaterial3d(o));
        }
    }
}
