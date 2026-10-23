//! Rust messages and events for TS, by reflect type path.
//!
//! Bevy's `ReflectMessage` can only write and its `ReflectEvent` cannot see an entity target, so
//! bevy-ts registers its own type data per type: [`ReflectTsMessage`] reads `Messages<M>` from a
//! cursor and writes into it, [`ReflectTsEvent`] builds an observer (for every trigger, or for one
//! entity's) and triggers. Both hand over reflected values; the caller turns them into JSON.

use bevy::ecs::event::{EntityTrigger, GlobalTrigger, PropagateEntityTrigger};
use bevy::ecs::message::Messages;
use bevy::ecs::traversal::Traversal;
use bevy::ecs::world::DeferredWorld;
use bevy::prelude::*;
use bevy::reflect::{CreateTypeData, GetTypeRegistration, PartialReflect};

/// A Rust message TS can read and write: `app.ts_message::<M>()`.
#[derive(Clone, Copy)]
pub struct ReflectTsMessage {
    read: fn(&World, usize, &mut dyn FnMut(&dyn PartialReflect)) -> usize,
    end: fn(&World) -> usize,
    write: fn(&mut World, &dyn PartialReflect) -> Result<(), String>,
}

impl ReflectTsMessage {
    /// Visits each message at or after `cursor` still in `Messages<M>`, oldest first, and returns
    /// the cursor past the last one.
    pub fn read(&self, world: &World, cursor: usize, visit: &mut dyn FnMut(&dyn PartialReflect)) -> usize {
        (self.read)(world, cursor, visit)
    }

    /// The cursor past every message written so far, where a restart puts a reader.
    pub fn end(&self, world: &World) -> usize {
        (self.end)(world)
    }

    /// Writes `value` as a message, for Rust readers later in the frame.
    pub fn write(&self, world: &mut World, value: &dyn PartialReflect) -> Result<(), String> {
        (self.write)(world, value)
    }
}

impl<M: Message + FromReflect + TypePath> CreateTypeData<M> for ReflectTsMessage {
    fn create_type_data(_: ()) -> Self {
        ReflectTsMessage {
            read: |world, cursor, visit| {
                let Some(messages) = world.get_resource::<Messages<M>>() else { return cursor };
                let end = messages.oldest_message_count() + messages.len();
                for id in cursor.max(messages.oldest_message_count())..end {
                    if let Some((m, _)) = messages.get_message(id) {
                        visit(m);
                    }
                }
                end.max(cursor)
            },
            end: |world| world.get_resource::<Messages<M>>().map_or(0, |m| m.oldest_message_count() + m.len()),
            write: |world, value| {
                let m = M::from_reflect(value).ok_or_else(|| format!("{} cannot be built from that value", M::type_path()))?;
                world.write_message(m).map(|_| ()).ok_or_else(|| format!("{} is not added as a message", M::type_path()))
            },
        }
    }
}

/// What a Rust event's TS observer is handed: the event, the current target and the first
/// target of a bubbling event (both `None` for a global event).
pub type TsEventSink = Box<dyn Fn(&dyn Reflect, Option<Entity>, Option<Entity>, DeferredWorld) + Send + Sync>;

/// A Rust event TS can observe and trigger: `app.ts_event::<E>()` or `app.ts_entity_event::<E>()`.
#[derive(Clone, Copy)]
pub struct ReflectTsEvent {
    /// An `EntityEvent`, with a target.
    pub entity: bool,
    observe: fn(Option<Entity>, TsEventSink) -> Observer,
    trigger: fn(&mut World, &dyn PartialReflect) -> Result<(), String>,
}

impl ReflectTsEvent {
    /// A Bevy observer passing every trigger of the event to `sink`, or only those whose current
    /// target is `target`.
    pub fn observe(&self, target: Option<Entity>, sink: TsEventSink) -> Observer {
        (self.observe)(target, sink)
    }

    /// Triggers the event built from `value` (an entity event carries its target).
    pub fn trigger(&self, world: &mut World, value: &dyn PartialReflect) -> Result<(), String> {
        (self.trigger)(world, value)
    }

    /// The type data of a global event, whose trigger is Bevy's `GlobalTrigger`.
    pub fn global<E>() -> Self
    where
        E: for<'a> Event<Trigger<'a> = GlobalTrigger> + Reflect + FromReflect + TypePath,
    {
        ReflectTsEvent {
            entity: false,
            observe: |target, sink| {
                let o = Observer::new(move |ev: On<E>, world: DeferredWorld| sink(ev.event(), None, None, world));
                match target {
                    Some(e) => o.with_entity(e),
                    None => o,
                }
            },
            trigger: |world, value| {
                let e = E::from_reflect(value).ok_or_else(|| format!("{} cannot be built from that value", E::type_path()))?;
                world.trigger(e);
                Ok(())
            },
        }
    }

    /// The type data of an entity event, bubbling or not.
    pub fn entity<E>() -> Self
    where
        E: EntityEvent + Reflect + FromReflect + TypePath,
        for<'a> E::Trigger<'a>: OriginalTarget + Default,
    {
        ReflectTsEvent {
            entity: true,
            observe: |target, sink| {
                let o = Observer::new(move |ev: On<E>, world: DeferredWorld| {
                    let current = ev.event().event_target();
                    sink(ev.event(), Some(current), ev.trigger().original().or(Some(current)), world)
                });
                match target {
                    Some(e) => o.with_entity(e),
                    None => o,
                }
            },
            trigger: |world, value| {
                let e = E::from_reflect(value).ok_or_else(|| format!("{} cannot be built from that value", E::type_path()))?;
                world.trigger(e);
                Ok(())
            },
        }
    }
}

/// Adds `M` as a message, registers it for reflection and gives it [`ReflectTsMessage`].
pub fn register_message<M>(app: &mut App)
where
    M: Message + FromReflect + TypePath + GetTypeRegistration,
{
    app.add_message::<M>().register_type::<M>().register_type_data::<M, ReflectTsMessage>();
}

/// Registers the global event `E` for reflection and gives it [`ReflectTsEvent`].
pub fn register_event<E>(app: &mut App)
where
    E: for<'a> Event<Trigger<'a> = GlobalTrigger> + Reflect + FromReflect + TypePath + GetTypeRegistration,
{
    insert_event::<E>(app, ReflectTsEvent::global::<E>());
}

/// Registers the entity event `E` for reflection and gives it [`ReflectTsEvent`].
pub fn register_entity_event<E>(app: &mut App)
where
    E: EntityEvent + Reflect + FromReflect + TypePath + GetTypeRegistration,
    for<'a> E::Trigger<'a>: OriginalTarget + Default,
{
    insert_event::<E>(app, ReflectTsEvent::entity::<E>());
}

fn insert_event<E: GetTypeRegistration>(app: &mut App, data: ReflectTsEvent) {
    app.register_type::<E>();
    let registry = app.world().resource::<AppTypeRegistry>().clone();
    registry.write().get_mut(std::any::TypeId::of::<E>()).expect("registered just now").insert(data);
}

/// The first target of a bubbling trigger, for `On<E>.original`.
pub trait OriginalTarget {
    fn original(&self) -> Option<Entity>;
}

impl OriginalTarget for EntityTrigger {
    fn original(&self) -> Option<Entity> {
        None
    }
}

impl<const AUTO_PROPAGATE: bool, E: EntityEvent, T: Traversal<E>> OriginalTarget for PropagateEntityTrigger<AUTO_PROPAGATE, E, T> {
    fn original(&self) -> Option<Entity> {
        Some(self.original_event_target)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::reflect::structs::DynamicStruct;
    use std::sync::{Arc, Mutex};

    #[derive(Message, Reflect, Debug, PartialEq)]
    struct Scored {
        points: u32,
    }

    #[derive(Event, Reflect)]
    struct Ping {
        n: u32,
    }

    #[derive(EntityEvent, Reflect)]
    #[entity_event(propagate, auto_propagate)]
    struct Poke {
        entity: Entity,
    }

    fn scored(points: u32) -> DynamicStruct {
        let mut d = DynamicStruct::default();
        d.insert("points", points);
        d
    }

    #[test]
    fn messages_are_read_from_a_cursor_and_written() {
        let mut world = World::new();
        world.init_resource::<Messages<Scored>>();
        let t = <ReflectTsMessage as CreateTypeData<Scored>>::create_type_data(());
        t.write(&mut world, &scored(1)).unwrap();
        t.write(&mut world, &scored(2)).unwrap();
        let mut seen = vec![];
        let next = t.read(&world, 0, &mut |m| seen.push(m.try_downcast_ref::<Scored>().unwrap().points));
        assert_eq!((seen.as_slice(), next), (&[1, 2][..], 2));
        world.resource_mut::<Messages<Scored>>().update();
        t.write(&mut world, &scored(3)).unwrap();
        let mut seen = vec![];
        assert_eq!(t.read(&world, next, &mut |m| seen.push(m.try_downcast_ref::<Scored>().unwrap().points)), 3);
        assert_eq!(seen, [3], "only the unread one");
        world.resource_mut::<Messages<Scored>>().update();
        world.resource_mut::<Messages<Scored>>().update();
        let mut n = 0;
        assert_eq!(t.read(&world, 0, &mut |_| n += 1), 3);
        assert_eq!((n, t.end(&world)), (0, 3), "expired after two updates; the end stays put");
        assert!(t.write(&mut World::new(), &scored(1)).unwrap_err().contains("not added"));
    }

    #[test]
    fn events_are_observed_globally_or_per_entity_with_their_targets() {
        let mut world = World::new();
        let seen: Arc<Mutex<Vec<String>>> = Arc::default();
        let ping = ReflectTsEvent::global::<Ping>();
        let poke = ReflectTsEvent::entity::<Poke>();
        let parent = world.spawn_empty().id();
        let child = world.spawn(ChildOf(parent)).id();
        let s = seen.clone();
        world.spawn(ping.observe(None, Box::new(move |e, _, _, _| s.lock().unwrap().push(format!("ping {}", e.downcast_ref::<Ping>().unwrap().n)))));
        let s = seen.clone();
        world.spawn(poke.observe(None, Box::new(move |_, cur, orig, _| s.lock().unwrap().push(format!("poke {:?} {:?}", cur == Some(child), orig == Some(child))))));
        let s = seen.clone();
        world.spawn(poke.observe(Some(parent), Box::new(move |_, cur, _, _| s.lock().unwrap().push(format!("parent {:?}", cur == Some(parent))))));
        world.flush();
        let mut v = DynamicStruct::default();
        v.insert("n", 7u32);
        ping.trigger(&mut world, &v).unwrap();
        let mut v = DynamicStruct::default();
        v.insert("entity", child);
        poke.trigger(&mut world, &v).unwrap();
        let seen = seen.lock().unwrap().clone();
        assert_eq!(seen, ["ping 7", "poke true true", "poke false true", "parent true"]);
    }
}
