//! `bevy-ts/entities` and `bevy-ts/entities+watch`: the scene tree and its structural changes.
//! The stream is fed by lifecycle observers that exist only while someone watches, and sends at
//! most one message per frame.

use crate::Subscribers;
use bevy::ecs::component::ComponentId;
use bevy::prelude::*;
use bevy::remote::BrpResult;
use bevy_ts::components::{ComponentState, TsComponents};
use serde_json::{Map, Value, json};
use std::collections::{BTreeMap, BTreeSet, HashSet};

fn component_names(world: &World, entity: Entity) -> Vec<String> {
    let Ok(e) = world.get_entity(entity) else { return vec![] };
    let ts = world.resource::<TsComponents>();
    let mut out: Vec<String> = e
        .archetype()
        .components()
        .iter()
        .filter_map(|id| match ts.by_id(*id) {
            Some(c) if c.state == ComponentState::Retired => None,
            Some(c) => Some(c.decl.name.clone()),
            None => world.components().get_name(*id).map(|n| n.to_string()),
        })
        .collect();
    out.sort();
    out
}

fn info(world: &World, entity: Entity) -> Value {
    json!({
        "entity": entity.to_bits(),
        "name": world.get::<Name>(entity).map(|n| n.as_str().to_string()),
        "parent": world.get::<ChildOf>(entity).map(|c| c.parent().to_bits()),
        "components": component_names(world, entity),
    })
}

/// Components whose entities the engine keeps for itself: windows, monitors, gamepads and picking
/// pointers. Not `ScriptHidden`, which would hide them from TS queries too; matched by type path,
/// so this crate needs none of those Bevy features.
const ENGINE: [&str; 4] = ["bevy_window::window::Window", "bevy_window::monitor::Monitor", "bevy_input::gamepad::Gamepad", "bevy_picking::pointer::PointerId"];

fn engine_components(world: &World) -> Vec<ComponentId> {
    world.components().iter_registered().filter(|(_, c)| ENGINE.contains(&c.name().to_string().as_str())).map(|(id, _)| id).collect()
}

/// Observers and resource entities are bookkeeping, not scene content; `ScriptHidden` entities
/// are the editor's, and windows, monitors, gamepads and pointers the engine's.
fn internal(e: &EntityRef, engine: &[ComponentId]) -> bool {
    e.contains::<Observer>()
        || e.contains::<bevy::ecs::resource::IsResource>()
        || e.contains::<bevy_ts::ScriptHidden>()
        || engine.iter().any(|id| e.contains_id(*id))
}

fn all_entities(world: &World) -> Vec<Entity> {
    let engine = engine_components(world);
    let mut es: Vec<Entity> = world.iter_entities().filter(|e| !internal(e, &engine)).map(|e| e.id()).collect();
    es.sort();
    es
}

pub(crate) fn snapshot(In(_): In<Option<Value>>, world: &World) -> BrpResult {
    let seq = world.get_resource::<EntityWatch>().map_or(0, |w| w.seq);
    let entities: Vec<Value> = all_entities(world).into_iter().map(|e| info(world, e)).collect();
    Ok(json!({ "seq": seq, "entities": entities }))
}

#[derive(Clone, Copy, Debug)]
enum Event {
    Added(Entity, ComponentId),
    Removed(Entity, ComponentId),
    /// `Name` or `ChildOf` inserted (also on replace).
    Touched(Entity),
}

/// Observer state; exists only while the stream is watched.
#[derive(Resource, Default)]
pub(crate) struct EntityWatch {
    observers: Vec<Entity>,
    observed: usize,
    events: Vec<Event>,
    known: HashSet<Entity>,
    pub(crate) seq: u64,
    last_frame: u64,
    /// The message computed for `last_frame`, shared by every subscriber that frame.
    message: Option<Value>,
}

fn install(world: &mut World) {
    let ids: Vec<ComponentId> = world.components().iter_registered().map(|(id, _)| id).collect();
    let name = world.register_component::<Name>();
    let child_of = world.register_component::<ChildOf>();
    let mut w = world.remove_resource::<EntityWatch>().unwrap_or_default();
    for o in w.observers.drain(..) {
        world.despawn(o);
    }
    let added = world
        .spawn(Observer::new(|ev: On<Add<()>>, w: Option<ResMut<EntityWatch>>| {
            let Some(mut w) = w else { return };
            let e = ev.event().entity;
            for id in ev.trigger().components {
                w.events.push(Event::Added(e, *id));
            }
        }).with_components(ids.clone()))
        .id();
    let removed = world
        .spawn(Observer::new(|ev: On<Remove<()>>, w: Option<ResMut<EntityWatch>>| {
            let Some(mut w) = w else { return };
            let e = ev.event().entity;
            for id in ev.trigger().components {
                w.events.push(Event::Removed(e, *id));
            }
        }).with_components(ids.clone()))
        .id();
    let touched = world
        .spawn(Observer::new(|ev: On<Insert<()>>, w: Option<ResMut<EntityWatch>>| {
            let Some(mut w) = w else { return };
            w.events.push(Event::Touched(ev.event().entity));
        }).with_components(vec![name, child_of]))
        .id();
    if w.observers.is_empty() && w.known.is_empty() {
        w.known = all_entities(world).into_iter().collect();
    }
    w.observers = vec![added, removed, touched];
    w.observed = ids.len();
    world.insert_resource(w);
}

/// Turns a frame's events into one message, or `None` for a quiet frame.
fn delta(world: &mut World) -> Option<Value> {
    let mut w = world.remove_resource::<EntityWatch>().unwrap();
    let events = std::mem::take(&mut w.events);
    let mut touched: BTreeSet<Entity> = BTreeSet::new();
    let mut added: BTreeMap<Entity, BTreeSet<ComponentId>> = BTreeMap::new();
    let mut removed: BTreeMap<Entity, BTreeSet<ComponentId>> = BTreeMap::new();
    for ev in events {
        match ev {
            Event::Added(e, id) => {
                touched.insert(e);
                if !removed.get_mut(&e).is_some_and(|s| s.remove(&id)) {
                    added.entry(e).or_default().insert(id);
                }
            }
            Event::Removed(e, id) => {
                touched.insert(e);
                if !added.get_mut(&e).is_some_and(|s| s.remove(&id)) {
                    removed.entry(e).or_default().insert(id);
                }
            }
            Event::Touched(e) => {
                touched.insert(e);
            }
        }
    }
    let name_id = world.register_component::<Name>();
    let child_of_id = world.register_component::<ChildOf>();
    let ts = world.resource::<TsComponents>();
    let label = |id: &ComponentId| match ts.by_id(*id) {
        Some(c) => Some(c.decl.name.clone()),
        None => world.components().get_name(*id).map(|n| n.to_string()),
    };
    let engine = engine_components(world);
    let (mut spawned, mut despawned, mut reparented, mut renamed, mut add, mut rem) = (vec![], vec![], vec![], vec![], vec![], vec![]);
    for e in touched {
        // Observers are bookkeeping, not scene content.
        if world.get_entity(e).is_ok_and(|r| internal(&r, &engine)) || w.observers.contains(&e) {
            continue;
        }
        let exists = world.get_entity(e).is_ok();
        let known = w.known.contains(&e);
        match (known, exists) {
            (false, true) => {
                spawned.push(info(world, e));
                w.known.insert(e);
            }
            (true, false) => {
                despawned.push(json!(e.to_bits()));
                w.known.remove(&e);
            }
            (false, false) => {}
            (true, true) => {
                let a = added.remove(&e).unwrap_or_default();
                let r = removed.remove(&e).unwrap_or_default();
                if a.contains(&child_of_id) || r.contains(&child_of_id) || world.get::<ChildOf>(e).is_some_and(|_| a.is_empty() && r.is_empty()) {
                    reparented.push(json!({ "entity": e.to_bits(), "parent": world.get::<ChildOf>(e).map(|c| c.parent().to_bits()) }));
                }
                if a.contains(&name_id) || r.contains(&name_id) || world.get::<Name>(e).is_some_and(|_| a.is_empty() && r.is_empty()) {
                    renamed.push(json!({ "entity": e.to_bits(), "name": world.get::<Name>(e).map(|n| n.as_str().to_string()) }));
                }
                let a: Vec<String> = a.iter().filter(|id| **id != name_id && **id != child_of_id).filter_map(label).collect();
                let r: Vec<String> = r.iter().filter(|id| **id != name_id && **id != child_of_id).filter_map(label).collect();
                if !a.is_empty() {
                    add.push(json!({ "entity": e.to_bits(), "components": a }));
                }
                if !r.is_empty() {
                    rem.push(json!({ "entity": e.to_bits(), "components": r }));
                }
            }
        }
    }
    let quiet = spawned.is_empty() && despawned.is_empty() && reparented.is_empty() && renamed.is_empty() && add.is_empty() && rem.is_empty();
    let out = if quiet {
        None
    } else {
        w.seq += 1;
        let mut m = Map::new();
        m.insert("seq".into(), json!(w.seq));
        for (k, v) in [("spawned", spawned), ("despawned", despawned), ("reparented", reparented), ("renamed", renamed), ("added", add), ("removed", rem)] {
            if !v.is_empty() {
                m.insert(k.into(), Value::Array(v));
            }
        }
        Some(Value::Object(m))
    };
    world.insert_resource(w);
    out
}

pub(crate) fn watch(In(p): In<Option<Value>>, world: &mut World) -> BrpResult<Option<Value>> {
    let now = world.resource::<Subscribers>().update();
    let components = world.components().len();
    match world.get_resource::<EntityWatch>() {
        Some(w) if !w.observers.is_empty() && w.observed == components => {}
        _ => install(world),
    }
    let new = world.resource_mut::<Subscribers>().cursor("entities", &p).is_none();
    // One delta per frame, shared by every subscriber.
    if world.resource::<EntityWatch>().last_frame != now {
        let m = delta(world);
        let mut w = world.resource_mut::<EntityWatch>();
        w.last_frame = now;
        w.message = m;
    }
    if new {
        let w = world.resource::<EntityWatch>();
        let seq = w.seq;
        let entities: Vec<Value> = all_entities(world).into_iter().map(|e| info(world, e)).collect();
        return Ok(Some(json!({ "seq": seq, "snapshot": entities })));
    }
    Ok(world.resource::<EntityWatch>().message.clone())
}

/// `Last`: removes the observers once no subscriber has been served for a frame.
pub(crate) fn teardown_when_unwatched(world: &mut World) {
    let now = world.resource::<Subscribers>().update();
    let stale = world.get_resource::<EntityWatch>().is_some_and(|w| !w.observers.is_empty() && w.last_frame + 1 < now);
    if stale {
        let mut w = world.remove_resource::<EntityWatch>().unwrap();
        for o in w.observers.drain(..) {
            world.despawn(o);
        }
        let seq = w.seq;
        world.insert_resource(EntityWatch { seq, ..Default::default() });
    }
}
