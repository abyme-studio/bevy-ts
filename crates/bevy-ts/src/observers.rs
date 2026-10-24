//! The observer queue. TS triggers (`commands.trigger`) and Bevy lifecycle events on watched
//! components push here; the queue drains after each TS system's commands and at each stage
//! start, one JS call per event and target.
//!
//! `commands.entity(e).observe(fn)` attaches a TS observer to `e` alone. Each attachment is a Bevy
//! observer entity watching `e` and naming the function's declaration, so Bevy despawns it with
//! `e`; the drain runs the declaration's observer for a hop whose target `e` is watched by it.

use crate::manifest::{Lifecycle, SystemDecl};
use bevy::ecs::observer::ObservedBy;
use bevy::ecs::component::ComponentId;
use bevy::prelude::*;
use std::collections::VecDeque;

#[derive(Clone, Debug, PartialEq)]
pub enum Trigger {
    /// A TS event type, by index in `TsPayloads::events`.
    Event(usize),
    Lifecycle(Lifecycle, ComponentId),
}

#[derive(Clone, Debug)]
pub struct Pending {
    pub trigger: Trigger,
    pub bytes: Vec<u8>,
    /// The target and, for a bubbling entity event, its `ChildOf` ancestors. Empty for a global
    /// event.
    pub path: Vec<Entity>,
    /// The first target of a bubbling trigger, `On<E>.original`; `None` for a global event.
    pub original: Option<Entity>,
    /// Per hop of `path`, the declarations of the TS observers attached to it when the trigger
    /// was queued, so a target despawned before the drain still runs them, as Bevy's would.
    pub attached: Vec<Vec<String>>,
}

#[derive(Resource, Default, Debug)]
pub struct ObserverQueue(pub VecDeque<Pending>);

/// The Bevy observers feeding lifecycle triggers for the live generation.
#[derive(Resource, Default, Debug)]
pub struct LifecycleObservers(pub Vec<Entity>);

/// A Bevy observer entity standing for a TS observer attached to the entity it watches.
#[derive(Component, Clone, Debug)]
pub struct TsEntityObserver {
    /// The observer function's declaration, `file#name`.
    pub name: String,
}

/// Never triggered: the event a [`TsEntityObserver`]'s Bevy observer watches its entity for, so
/// the attachment lives and dies with the entity as Bevy's entity observers do.
#[derive(EntityEvent)]
struct Watched {
    entity: Entity,
}

/// Observers attached by `observe` since the last drain, by key index and declaration, for the
/// drain to build into `TsSystems::targeted`.
#[derive(Resource, Default, Debug)]
pub struct NewTargeted(pub Vec<(u32, SystemDecl)>);

/// Attaches the observer `decl` (key index `index` in the live generation) to `entity`.
pub fn observe(world: &mut World, entity: Entity, index: u32, decl: SystemDecl) {
    let name = decl.name.clone();
    world.spawn((Observer::new(|_: On<Watched>| {}).with_entity(entity), TsEntityObserver { name }));
    world.get_resource_or_init::<NewTargeted>().0.push((index, decl));
}

/// The declarations of the TS observers attached to `entity`, in attachment order.
pub fn attached(world: &World, entity: Entity) -> Vec<String> {
    let Some(by) = world.get::<ObservedBy>(entity) else { return vec![] };
    by.get().iter().filter_map(|o| world.get::<TsEntityObserver>(*o)).map(|o| o.name.clone()).collect()
}

/// [`attached`] for each hop of a path.
pub fn attached_path(world: &World, path: &[Entity]) -> Vec<Vec<String>> {
    path.iter().map(|e| attached(world, *e)).collect()
}

/// [`attached`] from inside a Bevy observer.
type Attachments<'w, 's> = (Query<'w, 's, &'static ObservedBy>, Query<'w, 's, &'static TsEntityObserver>);

fn attached_in((by, names): &Attachments, entity: Entity) -> Vec<String> {
    by.get(entity).map(|by| by.get().iter().filter_map(|o| names.get(*o).ok()).map(|o| o.name.clone()).collect()).unwrap_or_default()
}

/// Every declaration a live attachment names, sorted, for the next generation to rebuild.
pub fn observed(world: &mut World) -> Vec<String> {
    let mut names: Vec<String> = world.query::<&TsEntityObserver>().iter(world).map(|o| o.name.clone()).collect();
    names.sort();
    names.dedup();
    names
}

/// Despawns the attachments naming any of `names` (a restart passes every name).
pub fn detach(world: &mut World, names: impl Fn(&str) -> bool) {
    let gone: Vec<Entity> = world.query::<(Entity, &TsEntityObserver)>().iter(world).filter(|(_, o)| names(&o.name)).map(|(e, _)| e).collect();
    for e in gone {
        let _ = world.try_despawn(e);
    }
}

/// The target and its ancestors, for a bubbling event.
pub fn propagation_path(world: &World, target: Entity, bubble: bool) -> Vec<Entity> {
    let mut path = vec![target];
    if bubble {
        let mut at = target;
        while let Some(p) = world.get::<ChildOf>(at).map(ChildOf::parent) {
            if path.contains(&p) {
                break;
            }
            path.push(p);
            at = p;
        }
    }
    path
}

/// Replaces the lifecycle observers with ones for `watched` (kind, component) pairs.
pub fn install(world: &mut World, watched: &[(Lifecycle, ComponentId)]) {
    let old = std::mem::take(&mut world.get_resource_or_init::<LifecycleObservers>().0);
    for o in old {
        let _ = world.try_despawn(o);
    }
    let mut spawned = Vec::new();
    for kind in [Lifecycle::Add, Lifecycle::Insert, Lifecycle::Remove] {
        let ids: Vec<ComponentId> = watched.iter().filter(|(k, _)| *k == kind).map(|(_, id)| *id).collect();
        if ids.is_empty() {
            continue;
        }
        let watch = ids.clone();
        // A despawn's `Remove` runs before Bevy's `ObservedBy` hook, so the attachments are still
        // readable here.
        let push = move |entity: Entity, components: &[ComponentId], queue: Option<ResMut<ObserverQueue>>, hidden: Query<(), With<crate::ScriptHidden>>, a: Attachments| {
            let Some(mut queue) = queue else { return };
            if hidden.contains(entity) {
                return;
            }
            for id in components.iter().filter(|c| watch.contains(c)) {
                let attached = vec![attached_in(&a, entity)];
                queue.0.push_back(Pending { trigger: Trigger::Lifecycle(kind, *id), bytes: vec![], path: vec![entity], original: Some(entity), attached });
            }
        };
        let observer = match kind {
            Lifecycle::Add => Observer::new(move |ev: On<Add<()>>, q: Option<ResMut<ObserverQueue>>, h: Query<(), With<crate::ScriptHidden>>, a: Attachments| push(ev.event().entity, ev.trigger().components, q, h, a)),
            Lifecycle::Insert => Observer::new(move |ev: On<Insert<()>>, q: Option<ResMut<ObserverQueue>>, h: Query<(), With<crate::ScriptHidden>>, a: Attachments| push(ev.event().entity, ev.trigger().components, q, h, a)),
            Lifecycle::Remove => Observer::new(move |ev: On<Remove<()>>, q: Option<ResMut<ObserverQueue>>, h: Query<(), With<crate::ScriptHidden>>, a: Attachments| push(ev.event().entity, ev.trigger().components, q, h, a)),
        };
        spawned.push(world.spawn(observer.with_components(ids)).id());
    }
    world.resource_mut::<LifecycleObservers>().0 = spawned;
}
