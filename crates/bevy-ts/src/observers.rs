//! The observer queue. TS triggers (`commands.trigger`), Bevy lifecycle events on watched
//! components and Rust events TS observes push here; the queue drains after each TS system's commands and at each stage
//! start, one JS call per event and target.
//!
//! `commands.entity(e).observe(fn)` attaches a TS observer to `e` alone. Each attachment is a Bevy
//! observer entity watching `e` and naming the function's declaration, so Bevy despawns it with
//! `e`; the drain runs the declaration's observer for a hop whose target `e` is watched by it.

use crate::manifest::{Lifecycle, SystemDecl};
use bevy::ecs::observer::ObservedBy;
use bevy::ecs::component::ComponentId;
use bevy::ecs::world::DeferredWorld;
use bevy::prelude::*;
use std::any::TypeId;
use std::collections::{HashMap, VecDeque};

#[derive(Clone, Debug, PartialEq)]
pub enum Trigger {
    /// A TS event type, by index in `TsPayloads::events`.
    Event(usize),
    Lifecycle(Lifecycle, ComponentId),
    /// A Rust event, by type id; its bytes are its JSON.
    Rust(TypeId),
}

#[derive(Clone, Debug)]
pub struct Pending {
    pub trigger: Trigger,
    pub bytes: Vec<u8>,
    /// A Rust event's value at each later hop of `path`, as Bevy rewrote its target for that hop
    /// (`On::event()`); empty when the value is the same at every hop.
    pub hops: Vec<Vec<u8>>,
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

/// The Bevy observers feeding Rust event triggers for the live generation.
#[derive(Resource, Default, Debug)]
pub struct RustObservers(pub Vec<Entity>);

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
        // A despawn's `Remove` runs before Bevy's `ObservedBy` hook, so the attachments are still
        // readable here.
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
                let attached = vec![attached(&world, entity)];
                world.resource_mut::<ObserverQueue>().0.push_back(Pending { trigger: Trigger::Lifecycle(kind, *id), bytes, hops: vec![], path: vec![entity], original: Some(entity), attached });
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

/// Replaces the Rust event observers with one per type in `types`. Each serializes the event once
/// and queues it; a later hop of the same bubbling trigger joins the queued one, so
/// `propagate(false)` drops the rest of its TS hops.
pub fn install_rust(world: &mut World, types: &[TypeId]) {
    let old = std::mem::take(&mut world.get_resource_or_init::<RustObservers>().0);
    for o in old {
        let _ = world.try_despawn(o);
    }
    let registry = world.resource::<AppTypeRegistry>().clone();
    let observers: Vec<Observer> = {
        let registry = registry.read();
        types.iter().filter_map(|id| crate::rust_events::observer(&registry, *id, sink(*id))).collect()
    };
    let spawned = observers.into_iter().map(|o| world.spawn(o).id()).collect();
    world.resource_mut::<RustObservers>().0 = spawned;
}

fn sink(id: TypeId) -> crate::rust_events::TsEventSink {
    Box::new(move |value: &dyn Reflect, current: Option<Entity>, original: Option<Entity>, mut world: DeferredWorld| {
        if world.get_resource::<ObserverQueue>().is_none() {
            return;
        }
        let attached = current.map(|e| attached(&world, e));
        // Serialized at every hop: Bevy rewrites the target in the value as the event bubbles.
        let json = {
            let registry = world.resource::<AppTypeRegistry>().read();
            crate::rust_events::to_json(&registry, value.as_partial_reflect())
        };
        let bytes = match json.and_then(|v| serde_json::to_vec(&v).map_err(|e| e.to_string())) {
            Ok(b) => b,
            Err(e) => {
                warn!(target: "bevy_ts", "{}: {e}; TS observers do not see it", value.reflect_type_path());
                return;
            }
        };
        let mut queue = world.resource_mut::<ObserverQueue>();
        if let (Some(c), Some(o)) = (current, original)
            && c != o
            && let Some(back) = queue.0.back_mut()
            && back.trigger == Trigger::Rust(id)
            && back.original == Some(o)
        {
            back.path.push(c);
            back.hops.push(bytes);
            back.attached.push(attached.unwrap_or_default());
            return;
        }
        let pending = Pending { trigger: Trigger::Rust(id), bytes, hops: vec![], path: current.into_iter().collect(), original, attached: attached.into_iter().collect() };
        world.resource_mut::<ObserverQueue>().0.push_back(pending);
    })
}
