//! The observer queue. TS triggers (`commands.trigger`) and Bevy lifecycle events on watched
//! components push here; the queue drains after each TS system's commands and at each stage
//! start, one JS call per event and target.

use crate::manifest::Lifecycle;
use bevy::ecs::component::ComponentId;
use bevy::ecs::world::DeferredWorld;
use bevy::prelude::*;
use std::collections::{HashMap, VecDeque};

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

/// Replaces the lifecycle observers with ones for `watched` (kind, component) pairs. `Discard`,
/// `Remove` and `Despawn` copy a layout-tier component's bytes at trigger time, since the TS
/// observer runs later, when the value (or the entity) is gone.
pub fn install(world: &mut World, watched: &[(Lifecycle, ComponentId)]) {
    let old = std::mem::take(&mut world.get_resource_or_init::<LifecycleObservers>().0);
    for o in old {
        let _ = world.try_despawn(o);
    }
    // Layout-tier components by id, with their size: only these carry a value.
    let mut sizes: HashMap<ComponentId, usize> = HashMap::new();
    if let Some(ts) = world.get_resource::<crate::components::TsComponents>() {
        let native = world.get_resource::<crate::layout::NativeLayouts>().cloned().unwrap_or_default();
        sizes = ts.writable_sizes(&native);
        sizes.extend(native.iter().map(|n| (n.id, n.layout.size)));
    }
    let mut spawned = Vec::new();
    for kind in [Lifecycle::Add, Lifecycle::Insert, Lifecycle::Discard, Lifecycle::Remove, Lifecycle::Despawn] {
        let ids: Vec<ComponentId> = watched.iter().filter(|(k, _)| *k == kind).map(|(_, id)| *id).collect();
        if ids.is_empty() {
            continue;
        }
        let watch: Vec<(ComponentId, Option<usize>)> = ids.iter().map(|id| (*id, kind.carries_value().then(|| sizes.get(id).copied()).flatten())).collect();
        let push = move |entity: Entity, components: &[ComponentId], mut world: DeferredWorld| {
            if world.get_resource::<ObserverQueue>().is_none() || world.get::<crate::ScriptHidden>(entity).is_some() {
                return;
            }
            for (id, size) in watch.iter().filter(|(c, _)| components.contains(c)) {
                let bytes = match (size, world.get_entity(entity).ok().and_then(|e| e.get_by_id(*id).ok())) {
                    // SAFETY: a layout-tier component is `size` bytes of plain data.
                    (Some(n), Some(p)) => unsafe { std::slice::from_raw_parts(p.as_ptr(), *n) }.to_vec(),
                    _ => vec![],
                };
                world.resource_mut::<ObserverQueue>().0.push_back(Pending { trigger: Trigger::Lifecycle(kind, *id), bytes, path: vec![entity] });
            }
        };
        let observer = match kind {
            Lifecycle::Add => Observer::new(move |ev: On<Add<()>>, w: DeferredWorld| push(ev.event().entity, ev.trigger().components, w)),
            Lifecycle::Insert => Observer::new(move |ev: On<Insert<()>>, w: DeferredWorld| push(ev.event().entity, ev.trigger().components, w)),
            Lifecycle::Discard => Observer::new(move |ev: On<Discard<()>>, w: DeferredWorld| push(ev.event().entity, ev.trigger().components, w)),
            Lifecycle::Remove => Observer::new(move |ev: On<Remove<()>>, w: DeferredWorld| push(ev.event().entity, ev.trigger().components, w)),
            Lifecycle::Despawn => Observer::new(move |ev: On<Despawn<()>>, w: DeferredWorld| push(ev.event().entity, ev.trigger().components, w)),
        };
        spawned.push(world.spawn(observer.with_components(ids)).id());
    }
    world.resource_mut::<LifecycleObservers>().0 = spawned;
}
