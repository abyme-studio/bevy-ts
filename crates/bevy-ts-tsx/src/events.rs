//! Event props, each `on` plus a Bevy 0.20 event's name and an entity-local observer of it
//! (operator ruling 2026-10-20): every picking event (`onPointerClick` ... `onPointerScroll`) on
//! any `<entity>` or UI element; `onActivate` on a `<button>`; `onFocusGained` and `onFocusLost`
//! on any UI element, its own focus only; `onValueChange` on the value widgets, each its own
//! `ValueChange<T>` (design 5); `onTextEditChange` and
//! `onKeyboardInput` on an `<input>`, with `onSubmit` (Enter) and `onCancel` (Escape), which Bevy
//! lacks, read from the same `KeyboardInput` (operator ruling 2026-10-22). The observer queues the event with the handler's id; the
//! queue goes to JS in the frame's one host call, in arrival order. Picking events bubble up
//! `ChildOf` as Bevy's do (change scene-pick): a handler also runs for a hit on a descendant, a
//! glTF scene's meshes included, with `entity` its own entity and `target` the one hit. An entity
//! with a pointer handler gets a default `Pickable`, which Bevy's sprite backend needs, until its
//! last pointer handler goes. JS runs
//! after Bevy's walk, so a handler cannot stop it. An `<input>`'s keyboard events are the field's
//! own: a key that bubbled from a descendant is not.

use crate::tree::TsxTree;
use bevy::clipboard::Clipboard;
use bevy::input::ButtonState;
use bevy::input::keyboard::{Key, KeyboardInput};
use bevy::input_focus::{FocusCause, FocusGained, FocusLost, FocusedInput};
use bevy::input::mouse::MouseScrollUnit;
use bevy::picking::events::{
    PointerCancel, PointerClick, PointerDrag, PointerDragDrop, PointerDragEnd, PointerDragEnter, PointerDragLeave, PointerDragOver, PointerDragStart, PointerEnter,
    PointerLeave, PointerMove, PointerOut, PointerOver, PointerPress, PointerRelease, PointerScroll,
};
use bevy::ui_widgets::{Activate, ValueChange};
use crate::tree::TsxNode;
use bevy::picking::pointer::PointerButton;
use bevy::prelude::*;
use bevy::text::{EditableText, EditableTextFilter, FontCx, LayoutCx, TextEditChange};
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};

/// The handler ids of the live generation on one entity, by event name, and the events it
/// already has an observer for. Observers stay; ids change with each generation.
#[derive(Component, Default, Debug, Clone, PartialEq)]
pub struct TsxHandlers {
    pub ids: BTreeMap<String, u32>,
    observed: BTreeSet<String>,
    /// tsx inserted the entity's `Pickable`, so it removes it with the last pointer handler.
    pickable: bool,
}

fn button(b: PointerButton) -> &'static str {
    match b {
        PointerButton::Primary => "primary",
        PointerButton::Secondary => "secondary",
        PointerButton::Middle => "middle",
    }
}

#[allow(clippy::too_many_arguments)]
fn queue(tree: &mut TsxTree, handlers: &TsxHandlers, kind: &str, entity: Entity, target: Entity, position: Vec2, b: Option<PointerButton>, extra: Value) {
    let Some(id) = handlers.ids.get(kind) else { return };
    let mut ev = json!({
        "kind": kind,
        "entity": entity.to_bits(),
        "target": target.to_bits(),
        "position": [position.x, position.y],
        "button": b.map(button).map_or(Value::Null, Value::from),
    });
    if let (Some(o), Value::Object(more)) = (ev.as_object_mut(), extra) {
        o.extend(more);
    }
    tree.reactive.events.push((*id, ev));
}

fn vec2(v: Vec2) -> Value {
    json!([v.x, v.y])
}

/// One observer per picking event: its button, if it has one, and the fields Bevy adds to it
/// (drag `delta` and `distance`, the `dragged` or `dropped` entity, scroll `unit`, `x`, `y`).
macro_rules! observer {
    ($name:ident, $event:ty, $kind:literal, $button:expr, $extra:expr) => {
        fn $name(on: On<$event>, handlers: Query<&TsxHandlers>, mut tree: ResMut<TsxTree>) {
            let entity = on.event().entity;
            let Ok(h) = handlers.get(entity) else { return };
            let b: fn(&$event) -> Option<PointerButton> = $button;
            let x: fn(&$event) -> Value = $extra;
            queue(&mut tree, h, $kind, entity, on.original_event_target(), on.event().pointer.position, b(on.event()), x(on.event()));
        }
    };
}

observer!(on_click, PointerClick, "PointerClick", |e| Some(e.button), |e| json!({ "count": e.count }));
observer!(on_press, PointerPress, "PointerPress", |e| Some(e.button), |e| json!({ "count": e.count }));
observer!(on_release, PointerRelease, "PointerRelease", |e| Some(e.button), |_| Value::Null);
observer!(on_over, PointerOver, "PointerOver", |_| None, |_| Value::Null);
observer!(on_out, PointerOut, "PointerOut", |_| None, |_| Value::Null);
observer!(on_enter, PointerEnter, "PointerEnter", |_| None, |_| Value::Null);
observer!(on_leave, PointerLeave, "PointerLeave", |_| None, |_| Value::Null);
observer!(on_move, PointerMove, "PointerMove", |_| None, |e| json!({ "delta": vec2(e.delta) }));
observer!(on_cancel, PointerCancel, "PointerCancel", |_| None, |_| Value::Null);
observer!(on_drag_start, PointerDragStart, "PointerDragStart", |e| Some(e.button), |_| Value::Null);
observer!(on_drag, PointerDrag, "PointerDrag", |e| Some(e.button), |e| json!({ "delta": vec2(e.delta), "distance": vec2(e.distance) }));
observer!(on_drag_end, PointerDragEnd, "PointerDragEnd", |e| Some(e.button), |e| json!({ "distance": vec2(e.distance) }));
observer!(on_drag_enter, PointerDragEnter, "PointerDragEnter", |e| Some(e.button), |e| json!({ "dragged": e.dragged.to_bits() }));
observer!(on_drag_over, PointerDragOver, "PointerDragOver", |e| Some(e.button), |e| json!({ "dragged": e.dragged.to_bits() }));
observer!(on_drag_leave, PointerDragLeave, "PointerDragLeave", |e| Some(e.button), |e| json!({ "dragged": e.dragged.to_bits() }));
observer!(on_drag_drop, PointerDragDrop, "PointerDragDrop", |e| Some(e.button), |e| json!({ "dropped": e.dropped.to_bits() }));
observer!(on_scroll, PointerScroll, "PointerScroll", |_| None, |e| json!({
    "unit": match e.unit { MouseScrollUnit::Line => "line", MouseScrollUnit::Pixel => "pixel" },
    "x": e.x,
    "y": e.y,
}));

/// `Activate` on a `<button>` (click, or Enter or Space while focused): Bevy triggers it on the
/// widget and it does not propagate.
fn on_activate(on: On<Activate>, handlers: Query<&TsxHandlers>, mut tree: ResMut<TsxTree>) {
    let entity = on.event().entity;
    let Ok(h) = handlers.get(entity) else { return };
    let Some(id) = h.ids.get("Activate") else { return };
    tree.reactive.events.push((*id, json!({ "kind": "Activate", "entity": entity.to_bits() })));
}

/// `ValueChange<T>` on the widget it is triggered on (Bevy does not propagate it), as
/// `{ value, isFinal }`; `value` is the JSON of `T`, and an entity value adds the chosen child's
/// TSX `key` (design 8).
fn value_change(tree: &mut TsxTree, handlers: &TsxHandlers, entity: Entity, value: Value, key: Option<Value>, is_final: bool) {
    let Some(id) = handlers.ids.get("ValueChange") else { return };
    let mut ev = json!({ "kind": "ValueChange", "entity": entity.to_bits(), "value": value, "isFinal": is_final });
    if let Some(k) = key {
        ev["key"] = k;
    }
    tree.reactive.events.push((*id, ev));
}

/// The TSX key of an entity tsx made (its explicit key or `type#n`), or `null`.
fn key(nodes: &Query<&TsxNode>, e: Entity) -> Value {
    nodes.get(e).ok().and_then(|n| n.key_path.last().cloned()).map_or(Value::Null, Value::from)
}

fn on_value_bool(on: On<ValueChange<bool>>, handlers: Query<&TsxHandlers>, mut tree: ResMut<TsxTree>) {
    let e = on.event_target();
    let Ok(h) = handlers.get(e) else { return };
    value_change(&mut tree, h, e, json!(on.event().value), None, on.event().is_final);
}

fn on_value_f32(on: On<ValueChange<f32>>, handlers: Query<&TsxHandlers>, mut tree: ResMut<TsxTree>) {
    let e = on.event_target();
    let Ok(h) = handlers.get(e) else { return };
    value_change(&mut tree, h, e, json!(on.event().value), None, on.event().is_final);
}

fn on_value_entity(on: On<ValueChange<Entity>>, handlers: Query<&TsxHandlers>, nodes: Query<&TsxNode>, mut tree: ResMut<TsxTree>) {
    let e = on.event_target();
    let Ok(h) = handlers.get(e) else { return };
    let v = on.event().value;
    value_change(&mut tree, h, e, json!(v.to_bits()), Some(key(&nodes, v)), on.event().is_final);
}

fn on_value_option(on: On<ValueChange<Option<Entity>>>, handlers: Query<&TsxHandlers>, nodes: Query<&TsxNode>, mut tree: ResMut<TsxTree>) {
    let e = on.event_target();
    let Ok(h) = handlers.get(e) else { return };
    let v = on.event().value;
    let (value, k) = v.map_or((Value::Null, Value::Null), |v| (json!(v.to_bits()), key(&nodes, v)));
    value_change(&mut tree, h, e, value, Some(k), on.event().is_final);
}

/// `FocusGained` on the entity itself: Bevy bubbles it to ancestors, and a handler hears only its
/// own entity's (design 5).
fn on_focus_gained(on: On<FocusGained>, handlers: Query<&TsxHandlers>, mut tree: ResMut<TsxTree>) {
    let entity = on.event_target();
    if on.original_event_target() != entity {
        return;
    }
    let Ok(h) = handlers.get(entity) else { return };
    let Some(id) = h.ids.get("FocusGained") else { return };
    let cause = match on.event().cause {
        FocusCause::Navigated => "navigated",
        FocusCause::Pressed => "pressed",
        FocusCause::Auto => "auto",
    };
    tree.reactive.events.push((*id, json!({ "kind": "FocusGained", "entity": entity.to_bits(), "cause": cause })));
}

/// `FocusLost` on the entity itself, as `on_focus_gained`.
fn on_focus_lost(on: On<FocusLost>, handlers: Query<&TsxHandlers>, mut tree: ResMut<TsxTree>) {
    let entity = on.event_target();
    if on.original_event_target() != entity {
        return;
    }
    let Ok(h) = handlers.get(entity) else { return };
    let Some(id) = h.ids.get("FocusLost") else { return };
    tree.reactive.events.push((*id, json!({ "kind": "FocusLost", "entity": entity.to_bits() })));
}

/// `TextEditChange`: Bevy applied a field's edits (cursor moves included).
fn on_text_edit_change(on: On<TextEditChange>, fields: Query<(&TsxHandlers, &EditableText)>, mut tree: ResMut<TsxTree>) {
    let target = on.event_target();
    let Ok((h, text)) = fields.get(target) else { return };
    let Some(id) = h.ids.get("TextEditChange") else { return };
    let ev = json!({ "kind": "TextEditChange", "entity": target.to_bits(), "value": text.value().to_string() });
    tree.reactive.events.push((*id, ev));
}

/// The event names [`on_keyboard_input`] serves.
const KEY_EVENTS: [&str; 3] = ["KeyboardInput", "Submit", "Cancel"];

/// A key the focused field got: `KeyboardInput`, then `Submit` on Enter or `Cancel` on Escape. Bevy's `TextInput` observer is global, so it has already queued
/// this key's edit; applying the queue here, with what `apply_text_edits` uses, makes `value` the
/// text with this key and every key before it, though Bevy applies edits later in the frame.
/// Bevy's editor stays the text's only owner.
#[allow(clippy::too_many_arguments)]
fn on_keyboard_input(
    on: On<FocusedInput<KeyboardInput>>,
    mut fields: Query<(&TsxHandlers, &mut EditableText, Has<EditableTextFilter>)>,
    mut font: ResMut<FontCx>,
    mut layout: ResMut<LayoutCx>,
    mut clipboard: ResMut<Clipboard>,
    mut tree: ResMut<TsxTree>,
) {
    let target = on.event_target();
    if on.original_event_target() != target {
        return;
    }
    let Ok((h, mut text, filtered)) = fields.get_mut(target) else { return };
    let ids = KEY_EVENTS.map(|k| h.ids.get(k).copied());
    if ids.iter().all(Option::is_none) {
        return;
    }
    // A filter's function is private to Bevy; such a field's edits wait for Bevy's own pass.
    if !filtered && (!text.pending_edits.is_empty() || text.pending_paste.is_some()) {
        text.apply_pending_edits(&mut font, &mut layout.0, &mut clipboard, |_| true);
    }
    let input = &on.event().input;
    let entity = target.to_bits();
    let value = text.value().to_string();
    let [key, submit, cancel] = ids;
    if let Some(id) = key {
        let ev = json!({
            "kind": "KeyboardInput",
            "entity": entity,
            "key": bevy_ts::keys::key_name(&input.logical_key),
            "code": bevy_ts::keys::code_name(input.key_code),
            "text": input.text.as_deref(),
            "state": if input.state == ButtonState::Pressed { "down" } else { "up" },
            "repeat": input.repeat,
            "value": value,
        });
        tree.reactive.events.push((id, ev));
    }
    if input.state != ButtonState::Pressed || input.repeat {
        return;
    }
    match (&input.logical_key, submit, cancel) {
        (Key::Enter, Some(id), _) => tree.reactive.events.push((id, json!({ "kind": "Submit", "entity": entity, "value": value }))),
        (Key::Escape, _, Some(id)) => tree.reactive.events.push((id, json!({ "kind": "Cancel", "entity": entity }))),
        _ => {}
    }
}

/// Sets an entity's handler ids and attaches an observer for each event it has not seen yet.
pub fn set(world: &mut World, e: Entity, ids: &BTreeMap<String, u32>) {
    let current = world.get::<TsxHandlers>(e).cloned();
    if current.is_none() && ids.is_empty() {
        return;
    }
    let mut h = current.clone().unwrap_or_default();
    h.ids = ids.clone();
    let mut em = world.entity_mut(e);
    // Bevy 0.20's sprite backend picks only entities with `Pickable`; its default is what the
    // mesh and UI backends assume without one, so a pointer handler needs no other setup.
    let pointer = ids.keys().any(|k| k.starts_with("Pointer"));
    if pointer && !em.contains::<Pickable>() {
        em.insert(Pickable::default());
        h.pickable = true;
    } else if !pointer && h.pickable {
        em.remove::<Pickable>();
        h.pickable = false;
    }
    for kind in ids.keys() {
        // One observer serves every key event.
        let group = if KEY_EVENTS.contains(&kind.as_str()) { "KeyboardInput" } else { kind.as_str() };
        if h.observed.insert(group.to_string()) {
            match group {
                "PointerClick" => em.observe(on_click),
                "PointerPress" => em.observe(on_press),
                "PointerRelease" => em.observe(on_release),
                "PointerOver" => em.observe(on_over),
                "PointerOut" => em.observe(on_out),
                "PointerEnter" => em.observe(on_enter),
                "PointerLeave" => em.observe(on_leave),
                "PointerMove" => em.observe(on_move),
                "PointerCancel" => em.observe(on_cancel),
                "PointerDragStart" => em.observe(on_drag_start),
                "PointerDrag" => em.observe(on_drag),
                "PointerDragEnd" => em.observe(on_drag_end),
                "PointerDragEnter" => em.observe(on_drag_enter),
                "PointerDragOver" => em.observe(on_drag_over),
                "PointerDragLeave" => em.observe(on_drag_leave),
                "PointerDragDrop" => em.observe(on_drag_drop),
                "PointerScroll" => em.observe(on_scroll),
                "Activate" => em.observe(on_activate),
                "FocusGained" => em.observe(on_focus_gained),
                "FocusLost" => em.observe(on_focus_lost),
                "TextEditChange" => em.observe(on_text_edit_change),
                // One per `T`: only the one the widget triggers ever runs.
                "ValueChange" => em.observe(on_value_bool).observe(on_value_f32).observe(on_value_entity).observe(on_value_option),
                _ => em.observe(on_keyboard_input),
            };
        }
    }
    if current.as_ref() != Some(&h) {
        em.insert(h);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_pointer_handler_brings_pickable_and_takes_back_only_its_own() {
        let mut world = World::new();
        let click = BTreeMap::from([("PointerClick".to_string(), 1)]);
        let e = world.spawn_empty().id();
        set(&mut world, e, &click);
        assert!(world.get::<Pickable>(e).is_some(), "a sprite needs Pickable to be picked");
        set(&mut world, e, &BTreeMap::new());
        assert!(world.get::<Pickable>(e).is_none(), "gone with the last pointer handler");

        // A Pickable the entity already had is not tsx's to take.
        let own = world.spawn(Pickable::IGNORE).id();
        set(&mut world, own, &click);
        set(&mut world, own, &BTreeMap::new());
        assert_eq!(world.get::<Pickable>(own), Some(&Pickable::IGNORE));
    }
}
