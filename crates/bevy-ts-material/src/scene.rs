//! A script material on a glTF scene root (openspec change `scene-material`): the root's
//! `ScriptMaterial3d` draws the meshes its scene spawned, optionally only those whose glTF
//! material has one name. The root has no mesh, so it is the source of truth: each swapped mesh's
//! `StandardMaterial` is kept on the root and put back when the material goes. A mesh faded by
//! `MaterialAlpha` keeps its original, which `alpha.rs` knows; put back, it is faded again.

use crate::alpha::{FadeMaterial, standard_of};
use crate::instance::{self, ScriptInstance};
use crate::render::ScriptMaterial3d;
use bevy::ecs::entity::EntityHashSet;
use bevy::gltf::GltfMaterialName;
use bevy::prelude::*;
use bevy::world_serialization::{WorldAssetRoot, WorldInstance, WorldInstanceReady, WorldInstanceSpawner};

/// On a scene root beside `ScriptMaterial3d`: only meshes whose glTF material is named this
/// (`GltfMaterialName`, exact) take it. `<material kind gltfMaterial="...">` in TSX.
#[derive(Component, Clone, Debug, PartialEq, Eq)]
pub struct GltfMaterialFilter(pub String);

/// The meshes a root swapped, each with the `StandardMaterial` it had.
#[derive(Component, Clone, Debug, Default)]
pub struct SceneMaterialTargets(pub Vec<(Entity, Handle<StandardMaterial>)>);

/// Every swap and restore, in `PostUpdate`: a root's scene ready, or its material or filter
/// changed. Work on the same meshes orders after it, and sees the swap done (its commands apply
/// at the sync point Bevy inserts before).
#[derive(SystemSet, Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct SceneMaterialSet;

/// Roots whose scene became ready since the last `PostUpdate`.
#[derive(Resource, Default)]
struct ReadyRoots(EntityHashSet);

pub(crate) fn plugin(app: &mut App) {
    app.init_resource::<ReadyRoots>()
        .add_observer(on_ready)
        .add_systems(PostUpdate, (changed, follow_instance).chain().in_set(SceneMaterialSet).before(crate::render::check_entities_needing_specialization));
}

/// The scene is ready (spawned, or spawned again after a reload): swap its meshes in
/// [`SceneMaterialSet`].
fn on_ready(ready: On<WorldInstanceReady>, mut roots: ResMut<ReadyRoots>) {
    roots.0.insert(ready.entity);
}

type Swapping = Or<(With<ScriptMaterial3d>, With<SceneMaterialTargets>)>;
type RootChanged = (With<WorldAssetRoot>, Or<(Changed<ScriptMaterial3d>, Changed<GltfMaterialFilter>)>);

/// A root whose material or filter changed, or went, is brought to what it now asks.
fn changed(
    roots: Query<Entity, RootChanged>,
    swapped: Query<(), Swapping>,
    mut ready: ResMut<ReadyRoots>,
    mut removed_material: RemovedComponents<ScriptMaterial3d>,
    mut removed_filter: RemovedComponents<GltfMaterialFilter>,
    mut removed_instance: RemovedComponents<ScriptInstance>,
    mut commands: Commands,
) {
    let gone = removed_material
        .read()
        .chain(removed_filter.read())
        .chain(removed_instance.read())
        .chain(ready.0.drain())
        .filter(|e| swapped.contains(*e));
    let mut seen = EntityHashSet::default();
    for root in roots.iter().chain(gone) {
        if seen.insert(root) {
            commands.queue(move |world: &mut World| sync(world, root));
        }
    }
}

/// A root's instance params changed: each mesh it swapped takes them, in place.
fn follow_instance(roots: Query<(&ScriptInstance, &SceneMaterialTargets), Changed<ScriptInstance>>, mut commands: Commands) {
    for (si, targets) in &roots {
        for (e, _) in &targets.0 {
            let (e, si) = (*e, si.clone());
            commands.queue(move |world: &mut World| instance::put(world, e, si));
        }
    }
}

/// The entities `root`'s scene instance spawned, once it is ready; none before, or with no
/// scene. Not the root's descendants: TSX children of the root are not in it. The one walk over a
/// scene's own entities, for anything that changes them per root (materials, opacity).
pub fn scene_entities(world: &World, root: Entity) -> Vec<Entity> {
    let instance = world.get::<WorldInstance>(root).map(|i| **i);
    match (instance, world.get_resource::<WorldInstanceSpawner>()) {
        (Some(i), Some(s)) if s.instance_is_ready(i) => s.iter_instance_entities(i).collect(),
        _ => vec![],
    }
}

/// Brings the meshes of `root`'s scene instance to what the root asks: the root's script material
/// on each mesh that matches its filter, every other swapped mesh back to its own material.
/// Entities the instance did not spawn (TSX children) are never touched.
pub fn sync(world: &mut World, root: Entity) {
    let Ok(r) = world.get_entity(root) else { return };
    let material = r.get::<ScriptMaterial3d>().cloned();
    let si = r.get::<ScriptInstance>().cloned();
    let filter = r.get::<GltfMaterialFilter>().map(|f| f.0.clone());
    let mut targets = r.get::<SceneMaterialTargets>().cloned().unwrap_or_default().0;
    let scene = scene_entities(world, root);
    let in_scene: EntityHashSet = scene.iter().copied().collect();
    let wants = |world: &World, e: Entity| {
        material.is_some()
            && in_scene.contains(&e)
            && world.get::<Mesh3d>(e).is_some()
            && filter.as_ref().is_none_or(|f| world.get::<GltfMaterialName>(e).is_some_and(|n| n.0 == *f))
    };
    // Restore what no longer matches; drop what a respawned scene despawned.
    let mut kept = Vec::with_capacity(targets.len());
    for (e, original) in targets.drain(..) {
        if world.get_entity(e).is_err() {
            continue;
        }
        if wants(world, e) {
            kept.push((e, original));
        } else {
            instance::remove(world, e);
            world.entity_mut(e).remove::<ScriptMaterial3d>().insert(MeshMaterial3d(original));
        }
    }
    if let Some(m) = &material {
        for (e, _) in &kept {
            if world.get::<ScriptMaterial3d>(*e) != Some(m) {
                world.entity_mut(*e).remove::<(MeshMaterial3d<StandardMaterial>, MeshMaterial3d<FadeMaterial>)>().insert(m.clone());
            }
        }
        let known: EntityHashSet = kept.iter().map(|(e, _)| *e).collect();
        for e in scene {
            if known.contains(&e) || !wants(world, e) {
                continue;
            }
            let Some(original) = standard_of(world, e) else { continue };
            world.entity_mut(e).remove::<(MeshMaterial3d<StandardMaterial>, MeshMaterial3d<FadeMaterial>)>().insert(m.clone());
            kept.push((e, original));
        }
    }
    // The root's instance params on every mesh it swapped; none, when it has none.
    for (e, _) in &kept {
        match &si {
            Some(si) => instance::put(world, *e, si.clone()),
            None => instance::remove(world, *e),
        }
    }
    let mut r = world.entity_mut(root);
    if kept.is_empty() {
        r.remove::<SceneMaterialTargets>();
    } else {
        r.insert(SceneMaterialTargets(kept));
    }
}
