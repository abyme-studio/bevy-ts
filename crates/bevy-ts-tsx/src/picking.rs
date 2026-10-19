//! Picking props: `onPointerClick`, `onPointerPress`, `onPointerRelease`, `onPointerOver` and
//! `onPointerOut` on any `<entity>` or UI element are entity-local observers of Bevy 0.20's
//! picking events (operator ruling 2026-10-20). The observer queues the event with the handler's
//! id; the queue goes to JS in the frame's one host call, in arrival order. Events do not bubble:
//! an event that reached this entity from a descendant is not this entity's.

use crate::tree::TsxTree;
use bevy::picking::events::{PointerClick, PointerOut, PointerOver, PointerPress, PointerRelease};
use bevy::picking::pointer::PointerButton;
use bevy::prelude::*;
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};

/// The handler ids of the live generation on one entity, by event name, and the events it
/// already has an observer for. Observers stay; ids change with each generation.
#[derive(Component, Default, Debug, Clone, PartialEq)]
pub struct TsxHandlers {
    pub ids: BTreeMap<String, u32>,
    observed: BTreeSet<String>,
}

fn button(b: PointerButton) -> &'static str {
    match b {
        PointerButton::Primary => "primary",
        PointerButton::Secondary => "secondary",
        PointerButton::Middle => "middle",
    }
}

fn queue(tree: &mut TsxTree, handlers: &TsxHandlers, kind: &str, entity: Entity, position: Vec2, b: Option<PointerButton>) {
    let Some(id) = handlers.ids.get(kind) else { return };
    let ev = json!({
        "kind": kind,
        "entity": entity.to_bits(),
        "position": [position.x, position.y],
        "button": b.map(button).map_or(Value::Null, Value::from),
    });
    tree.reactive.events.push((*id, ev));
}

macro_rules! observer {
    ($name:ident, $event:ty, $kind:literal, $button:expr) => {
        fn $name(on: On<$event>, handlers: Query<&TsxHandlers>, mut tree: ResMut<TsxTree>) {
            let target = on.event().entity;
            if on.original_event_target() != target {
                return;
            }
            let Ok(h) = handlers.get(target) else { return };
            let b: fn(&$event) -> Option<PointerButton> = $button;
            queue(&mut tree, h, $kind, target, on.event().pointer.position, b(on.event()));
        }
    };
}

observer!(on_click, PointerClick, "PointerClick", |e| Some(e.button));
observer!(on_press, PointerPress, "PointerPress", |e| Some(e.button));
observer!(on_release, PointerRelease, "PointerRelease", |e| Some(e.button));
observer!(on_over, PointerOver, "PointerOver", |_| None);
observer!(on_out, PointerOut, "PointerOut", |_| None);

/// Sets an entity's handler ids and attaches an observer for each event it has not seen yet.
pub fn set(world: &mut World, e: Entity, ids: &BTreeMap<String, u32>) {
    let current = world.get::<TsxHandlers>(e).cloned();
    if current.is_none() && ids.is_empty() {
        return;
    }
    let mut h = current.clone().unwrap_or_default();
    h.ids = ids.clone();
    let mut em = world.entity_mut(e);
    for kind in ids.keys() {
        if h.observed.insert(kind.clone()) {
            match kind.as_str() {
                "PointerClick" => em.observe(on_click),
                "PointerPress" => em.observe(on_press),
                "PointerRelease" => em.observe(on_release),
                "PointerOver" => em.observe(on_over),
                _ => em.observe(on_out),
            };
        }
    }
    if current.as_ref() != Some(&h) {
        em.insert(h);
    }
}
