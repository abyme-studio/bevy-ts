//! Event props, each `on` plus a Bevy 0.20 event's name and an entity-local observer of it
//! (operator ruling 2026-10-20): `onPointerClick`, `onPointerPress`, `onPointerRelease`,
//! `onPointerOver` and `onPointerOut` on any `<entity>` or UI element; `onTextEditChange` and
//! `onKeyboardInput` on an `<input>`, with `onSubmit` (Enter) and `onCancel` (Escape), which Bevy
//! lacks, read from the same `KeyboardInput` (operator ruling 2026-10-22). The observer queues the event with the handler's id; the
//! queue goes to JS in the frame's one host call, in arrival order. Picking events bubble up
//! `ChildOf` as Bevy's do (change scene-pick): a handler also runs for a hit on a descendant, a
//! glTF scene's meshes included, with `entity` its own entity and `target` the one hit. JS runs
//! after Bevy's walk, so a handler cannot stop it. An `<input>`'s keyboard events are the field's
//! own: a key that bubbled from a descendant is not.

use crate::tree::TsxTree;
use bevy::clipboard::Clipboard;
use bevy::input::ButtonState;
use bevy::input::keyboard::{Key, KeyboardInput};
use bevy::input_focus::FocusedInput;
use bevy::picking::events::{PointerClick, PointerOut, PointerOver, PointerPress, PointerRelease};
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
}

fn button(b: PointerButton) -> &'static str {
    match b {
        PointerButton::Primary => "primary",
        PointerButton::Secondary => "secondary",
        PointerButton::Middle => "middle",
    }
}

fn queue(tree: &mut TsxTree, handlers: &TsxHandlers, kind: &str, entity: Entity, target: Entity, position: Vec2, b: Option<PointerButton>) {
    let Some(id) = handlers.ids.get(kind) else { return };
    let ev = json!({
        "kind": kind,
        "entity": entity.to_bits(),
        "target": target.to_bits(),
        "position": [position.x, position.y],
        "button": b.map(button).map_or(Value::Null, Value::from),
    });
    tree.reactive.events.push((*id, ev));
}

macro_rules! observer {
    ($name:ident, $event:ty, $kind:literal, $button:expr) => {
        fn $name(on: On<$event>, handlers: Query<&TsxHandlers>, mut tree: ResMut<TsxTree>) {
            let entity = on.event().entity;
            let Ok(h) = handlers.get(entity) else { return };
            let b: fn(&$event) -> Option<PointerButton> = $button;
            queue(&mut tree, h, $kind, entity, on.original_event_target(), on.event().pointer.position, b(on.event()));
        }
    };
}

observer!(on_click, PointerClick, "PointerClick", |e| Some(e.button));
observer!(on_press, PointerPress, "PointerPress", |e| Some(e.button));
observer!(on_release, PointerRelease, "PointerRelease", |e| Some(e.button));
observer!(on_over, PointerOver, "PointerOver", |_| None);
observer!(on_out, PointerOut, "PointerOut", |_| None);

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
    let target = on.event().focused_entity;
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
                "TextEditChange" => em.observe(on_text_edit_change),
                _ => em.observe(on_keyboard_input),
            };
        }
    }
    if current.as_ref() != Some(&h) {
        em.insert(h);
    }
}
