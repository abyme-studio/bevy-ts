//! Viewport edits: a gizmo drag writes the world live and, on release, becomes one edit routed
//! by provenance: `tsx` (the editor rewrites the literal), `locked` (bound, or rejected this
//! session), `transient` (no source describes the entity), `bsn` once `levels` exists.

use crate::edit::{EditState, Mode, Selection};
use bevy::gizmos::transform_gizmo::{TransformGizmoFocus, TransformGizmoMode, TransformGizmoSettings, TransformGizmoState};
use bevy::prelude::*;
use bevy_ts_tsx::tree::{Provenance, TsxNode};
use serde_json::{Value, json};
use std::collections::HashMap;
use std::time::{Duration, Instant};

/// The type path edits name.
pub const TRANSFORM: &str = "bevy_transform::components::transform::Transform";
/// A `tsx` edit with no answer by then is `unconfirmed`.
pub const ANSWER_TIMEOUT: Duration = Duration::from_secs(10);
const FIELDS: [&str; 3] = ["translation", "rotation", "scale"];

#[derive(Clone, Debug)]
pub struct Edit {
    pub id: u64,
    pub entity: Entity,
    pub node: Option<u64>,
    pub field: &'static str,
    pub before: Transform,
    pub after: Transform,
    pub route: Value,
    pub status: &'static str,
    pub reason: Option<String>,
    pub at: Instant,
    /// The log's seq when this edit last changed, for the watch.
    pub rev: u64,
}

/// Every edit of the session, and fields locked by a rejection.
#[derive(Resource, Default)]
pub struct EditLog {
    pub edits: Vec<Edit>,
    pub seq: u64,
    next_id: u64,
    /// (node or entity bits, field) to the reason it is locked.
    locks: HashMap<(u64, &'static str), String>,
    drag: Option<(Entity, Transform, &'static str)>,
    /// Overrides [`ANSWER_TIMEOUT`] (tests).
    pub timeout: Option<Duration>,
}

impl EditLog {
    fn touch(&mut self, i: usize) {
        self.seq += 1;
        self.edits[i].rev = self.seq;
    }
}

fn transform_json(t: &Transform) -> Value {
    json!({ "translation": t.translation.to_array(), "rotation": t.rotation.to_array(), "scale": t.scale.to_array() })
}

fn lock_key(world: &World, e: Entity) -> u64 {
    world.get::<TsxNode>(e).map_or(e.to_bits(), |n| n.id)
}

/// The route for one `Transform` field of `e`.
pub fn route(world: &World, e: Entity, field: &'static str) -> Value {
    if let Some(reason) = world.get_resource::<EditLog>().and_then(|l| l.locks.get(&(lock_key(world, e), field))) {
        return json!({ "kind": "locked", "reason": reason });
    }
    let Some(node) = world.get::<TsxNode>(e) else {
        return json!({ "kind": "transient" });
    };
    let owned = node.components.get("Transform");
    let provenance = owned.and_then(|c| c.fields.get(field)).map(|f| f.provenance);
    let element = owned.and_then(|c| c.source.clone()).or_else(|| node.source.first().cloned());
    match provenance {
        Some(Provenance::Bound) => json!({ "kind": "locked", "reason": "bound", "element": element, "prop": field }),
        _ => json!({ "kind": "tsx", "source": node.source, "element": element, "prop": field }),
    }
}

pub fn routes_json(world: &World, e: Entity) -> Value {
    json!({
        "translation": route(world, e, "translation"),
        "rotation": route(world, e, "rotation"),
        "scale": route(world, e, "scale"),
    })
}

fn mode_field(mode: TransformGizmoMode) -> &'static str {
    match mode {
        TransformGizmoMode::Translate => "translation",
        TransformGizmoMode::Rotate => "rotation",
        TransformGizmoMode::Scale => "scale",
    }
}

pub(crate) fn build(app: &mut App) {
    app.init_resource::<EditLog>().add_systems(
        PostUpdate,
        (lock_inert_fields.before(bevy::gizmos::transform_gizmo::TransformGizmoSystems), track_drags.after(bevy::gizmos::transform_gizmo::TransformGizmoSystems), expire)
            .run_if(|s: Res<EditState>| s.mode == Mode::Edit),
    );
}

/// A locked field's handles do nothing: the gizmo loses its focus while that field's mode is on.
fn lock_inert_fields(world: &mut World) {
    let Some(primary) = world.resource::<Selection>().primary() else { return };
    if world.resource::<TransformGizmoState>().active {
        return;
    }
    let field = mode_field(world.resource::<TransformGizmoSettings>().mode);
    let locked = route(world, primary, field)["kind"] == "locked";
    let Ok(mut e) = world.get_entity_mut(primary) else { return };
    if locked {
        e.remove::<TransformGizmoFocus>();
    } else if !e.contains::<TransformGizmoFocus>() {
        e.insert(TransformGizmoFocus);
    }
}

/// Drag start and release, from the gizmo's state; `Escape` mid-drag restores and drops it.
fn track_drags(world: &mut World) {
    let (active, entity, start) = {
        let g = world.resource::<TransformGizmoState>();
        (g.active, g.entity, g.start_transform)
    };
    let escape = world.resource::<ButtonInput<KeyCode>>().just_pressed(KeyCode::Escape);
    let field = mode_field(world.resource::<TransformGizmoSettings>().mode);
    let drag = world.resource::<EditLog>().drag;
    match (drag, active) {
        (None, true) => {
            if let Some(e) = entity {
                world.resource_mut::<EditLog>().drag = Some((e, start, field));
            }
        }
        (Some((e, before, _)), true) if escape => {
            if let Some(mut t) = world.get_mut::<Transform>(e) {
                *t = before;
            }
            world.resource_mut::<TransformGizmoState>().active = false;
            world.resource_mut::<EditLog>().drag = None;
        }
        (Some((e, before, field)), false) => {
            world.resource_mut::<EditLog>().drag = None;
            let Some(after) = world.get::<Transform>(e).copied() else { return };
            if after == before {
                return;
            }
            record(world, e, field, before, after);
        }
        _ => {}
    }
}

/// Adds an edit and answers its id.
pub fn record(world: &mut World, e: Entity, field: &'static str, before: Transform, after: Transform) -> u64 {
    let route = route(world, e, field);
    let status = if route["kind"] == "transient" { "transient" } else { "pending" };
    let node = world.get::<TsxNode>(e).map(|n| n.id);
    let mut log = world.resource_mut::<EditLog>();
    log.next_id += 1;
    let id = log.next_id;
    log.edits.push(Edit { id, entity: e, node, field, before, after, route, status, reason: None, at: Instant::now(), rev: 0 });
    let i = log.edits.len() - 1;
    log.touch(i);
    id
}

/// `tsx` edits with no answer in 10 s stay applied and become `unconfirmed`.
fn expire(mut log: ResMut<EditLog>) {
    let timeout = log.timeout.unwrap_or(ANSWER_TIMEOUT);
    let late: Vec<usize> = log.edits.iter().enumerate().filter(|(_, e)| e.status == "pending" && e.at.elapsed() >= timeout).map(|(i, _)| i).collect();
    for i in late {
        log.edits[i].status = "unconfirmed";
        log.touch(i);
    }
}

pub fn edit_json(e: &Edit) -> Value {
    json!({
        "id": e.id,
        "entity": e.entity.to_bits(),
        "component": TRANSFORM,
        "field": e.field,
        "before": transform_json(&e.before),
        "after": transform_json(&e.after),
        "route": e.route,
        "status": e.status,
        "reason": e.reason,
    })
}

fn find(world: &World, id: u64) -> Option<usize> {
    world.resource::<EditLog>().edits.iter().position(|e| e.id == id)
}

/// The entity an edit applies to now: its node's entity if a reload respawned it.
fn live_entity(world: &World, e: &Edit) -> Option<Entity> {
    if world.get_entity(e.entity).is_ok() {
        return Some(e.entity);
    }
    let node = e.node?;
    world.get_resource::<bevy_ts_tsx::tree::TsxTree>()?.entity(node)
}

/// Puts a field of `before` back.
fn restore(world: &mut World, i: usize) {
    let edit = world.resource::<EditLog>().edits[i].clone();
    let Some(e) = live_entity(world, &edit) else { return };
    if let Some(mut t) = world.get_mut::<Transform>(e) {
        match edit.field {
            "translation" => t.translation = edit.before.translation,
            "rotation" => t.rotation = edit.before.rotation,
            _ => t.scale = edit.before.scale,
        }
    }
}

/// The editor's answer to a `tsx` edit.
pub fn answer(world: &mut World, id: u64, written: bool, reason: Option<String>) -> Result<Value, String> {
    let i = find(world, id).ok_or_else(|| format!("no edit {id}"))?;
    let edit = world.resource::<EditLog>().edits[i].clone();
    if edit.route["kind"] != "tsx" {
        return Err(format!("edit {id} has a {} route; only tsx edits are answered", edit.route["kind"]));
    }
    if written {
        let mut log = world.resource_mut::<EditLog>();
        log.edits[i].status = "written";
        log.edits[i].reason = None;
        log.touch(i);
    } else {
        restore(world, i);
        let key = edit.node.unwrap_or(edit.entity.to_bits());
        let reason = reason.unwrap_or_else(|| "rejected".into());
        let mut log = world.resource_mut::<EditLog>();
        let field = FIELDS.into_iter().find(|f| *f == edit.field).unwrap_or("translation");
        log.locks.insert((key, field), reason.clone());
        log.edits[i].status = "rejected";
        log.edits[i].reason = Some(reason);
        log.touch(i);
        // The selection stream carries the new lock.
        world.resource_mut::<Selection>().seq += 1;
    }
    Ok(edit_json(&world.resource::<EditLog>().edits[i]))
}

/// Restores an edit's `before`.
pub fn revert(world: &mut World, id: u64) -> Result<Value, String> {
    let i = find(world, id).ok_or_else(|| format!("no edit {id}"))?;
    restore(world, i);
    let mut log = world.resource_mut::<EditLog>();
    log.edits[i].status = "reverted";
    log.touch(i);
    Ok(edit_json(&log.edits[i]))
}
