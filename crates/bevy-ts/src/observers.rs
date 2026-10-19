//! The observer queue. TS triggers (`commands.trigger`) and Bevy lifecycle events on watched
//! components push here; the queue drains after each TS system's commands and at each stage
//! start, one JS call per event and target.

use crate::manifest::Lifecycle;
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
}

#[derive(Resource, Default, Debug)]
pub struct ObserverQueue(pub VecDeque<Pending>);

/// The Bevy observers feeding lifecycle triggers for the live generation.
#[derive(Resource, Default, Debug)]
pub struct LifecycleObservers(pub Vec<Entity>);

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
        let push = move |entity: Entity, components: &[ComponentId], queue: Option<ResMut<ObserverQueue>>| {
            let Some(mut queue) = queue else { return };
            for id in components.iter().filter(|c| watch.contains(c)) {
                queue.0.push_back(Pending { trigger: Trigger::Lifecycle(kind, *id), bytes: vec![], path: vec![entity] });
            }
        };
        let observer = match kind {
            Lifecycle::Add => Observer::new(move |ev: On<Add<()>>, q: Option<ResMut<ObserverQueue>>| push(ev.event().entity, ev.trigger().components, q)),
            Lifecycle::Insert => Observer::new(move |ev: On<Insert<()>>, q: Option<ResMut<ObserverQueue>>| push(ev.event().entity, ev.trigger().components, q)),
            Lifecycle::Remove => Observer::new(move |ev: On<Remove<()>>, q: Option<ResMut<ObserverQueue>>| push(ev.event().entity, ev.trigger().components, q)),
        };
        spawned.push(world.spawn(observer.with_components(ids)).id());
    }
    world.resource_mut::<LifecycleObservers>().0 = spawned;
}
