//! Where a reflected or heap host call reads or writes: its place (openspec ts-heap-fields-2,
//! decision 4). The one owner of the place model; `reflect_codec::call` is its one reader.
//!
//! Every such call puts its place in its first two `f64` slots, its target in the third, then its
//! own (a write's byte length in the fourth):
//!
//! | Place | Slot 0 | Slot 1 |
//! | --- | --- | --- |
//! | an entity's component | the entity's bits (≥ 0) | 0 |
//! | a resource | -1 | 0 |
//! | the running system's `Local` | -2 | the `Local`'s index in the system |
//! | a message | -3 | its sequence number |
//! | the running observer's value | -4 | 0 |
//!
//! A `Local` and an observer's value are found through [`RunPlace`], which the runner sets once per
//! system or observer run before it calls JS and clears after. A place that is not live fails the
//! call naming it.

use bevy::ecs::component::ComponentId;
use bevy::prelude::*;

pub const RESOURCE: f64 = -1.0;
pub const LOCAL: f64 = -2.0;
pub const MESSAGE: f64 = -3.0;
pub const OBSERVER: f64 = -4.0;

/// A host call's place, read from its first two slots.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Place {
    Entity(Entity),
    Resource,
    Local(u32),
    Message(u64),
    Observer,
}

impl Place {
    pub fn decode(kind: f64, id: f64) -> Result<Place, String> {
        if kind >= 0.0 {
            let bits = kind as u64;
            return Entity::try_from_bits(bits).map(Place::Entity).ok_or_else(|| format!("{bits} is not an entity"));
        }
        match kind {
            RESOURCE => Ok(Place::Resource),
            LOCAL => Ok(Place::Local(id as u32)),
            MESSAGE => Ok(Place::Message(id as u64)),
            OBSERVER => Ok(Place::Observer),
            _ => Err(format!("{kind} is no place")),
        }
    }
}

impl std::fmt::Display for Place {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Place::Entity(e) => write!(f, "entity {e}"),
            Place::Resource => f.write_str("the resource"),
            Place::Local(i) => write!(f, "the running system's Local {i}"),
            Place::Message(s) => write!(f, "message {s}"),
            Place::Observer => f.write_str("the running observer's value"),
        }
    }
}

/// The running observer's value: its bytes, owned by the drain's queued event for the whole run,
/// and the component it is a value of (a lifecycle event's), or `None` for an event's own.
#[derive(Clone, Copy, Debug)]
pub struct ObserverValue {
    pub ptr: *const u8,
    pub component: Option<ComponentId>,
}

/// The places of the run in progress that a host call may name. Only the runner writes it.
#[derive(Resource, Default, Debug)]
pub struct RunPlace {
    pub observer: Option<ObserverValue>,
}

// SAFETY: the pointer is read only on the main world's thread, inside the run that set it, while
// the value it points at is alive (the drain holds it until every observer of it has run).
unsafe impl Send for RunPlace {}
// SAFETY: as above.
unsafe impl Sync for RunPlace {}

/// Sets the running observer's value for `f`, and puts back what was there after.
pub fn with_observer<R>(world: &mut World, value: Option<ObserverValue>, f: impl FnOnce(&mut World) -> R) -> R {
    let before = std::mem::replace(&mut world.get_resource_or_init::<RunPlace>().observer, value);
    let r = f(world);
    world.resource_mut::<RunPlace>().observer = before;
    r
}
