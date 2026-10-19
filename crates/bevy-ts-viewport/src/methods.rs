//! The viewport's protocol methods, and the `First` system that applies what the viewer sent.

use crate::shared::SharedRes;
use crate::VirtualWindow;
use bevy::prelude::*;
use bevy::remote::{BrpError, BrpResult, error_codes};
use bevy::window::{PrimaryWindow, WindowEvent, WindowResized, WindowScaleFactorChanged};
use bevy_ts_remote::protocol;
use serde_json::{Value, json};
use std::sync::atomic::Ordering;

pub const CRATE: &str = "bevy-ts-viewport";

pub(crate) fn invalid(message: impl Into<String>) -> BrpError {
    BrpError { code: error_codes::INVALID_PARAMS, message: message.into(), data: None }
}

pub(crate) fn internal(message: impl Into<String>) -> BrpError {
    BrpError { code: error_codes::INTERNAL_ERROR, message: message.into(), data: None }
}

pub(crate) fn required<T: serde::de::DeserializeOwned>(p: Option<Value>) -> Result<T, BrpError> {
    serde_json::from_value(p.ok_or_else(|| invalid("params are required"))?).map_err(|e| invalid(format!("invalid params: {e}")))
}

pub(crate) fn to_value(v: impl serde::Serialize) -> BrpResult {
    serde_json::to_value(v).map_err(|e| internal(e.to_string()))
}

/// Registers the methods that only make sense with a viewer: attach, detach, resize, input.
pub(crate) fn add_viewer_methods(app: &mut App) {
    use bevy_ts_remote::add_method;
    add_method(app, CRATE, "bevy-ts/viewport_attach", attach);
    add_method(app, CRATE, "bevy-ts/viewport_detach", detach);
    add_method(app, CRATE, "bevy-ts/viewport_resize", resize);
    add_method(app, CRATE, "bevy-ts/viewport_input", input);
}

/// HTTP input joins the channel's queue, one `input` message per event.
fn input(In(p): In<Option<Value>>, world: &mut World) -> BrpResult {
    let p: protocol::ViewportInputParams = required(p)?;
    let shared = world.resource::<SharedRes>().0.clone();
    let n = p.events.len();
    for e in p.events {
        let params = serde_json::to_value(e).map_err(|e| internal(e.to_string()))?;
        shared.push(crate::mach::Incoming::Message { method: "input".into(), params, port: None });
    }
    to_value(protocol::ViewportInputResult { queued: n as u32 })
}

/// Registers the methods every mode serves.
pub(crate) fn add_common_methods(app: &mut App) {
    bevy_ts_remote::add_method(app, CRATE, "bevy-ts/viewport_stats", stats);
}

fn attach(In(p): In<Option<Value>>, world: &mut World) -> BrpResult {
    let p: protocol::ViewportAttachParams = required(p)?;
    if !(p.w > 0.0 && p.h > 0.0 && p.scale > 0.0) {
        return Err(invalid("w, h and scale must be positive"));
    }
    if let Some(hz) = p.hz {
        world.resource::<SharedRes>().0.set_hz(hz);
    }
    apply_size(world, p.w, p.h, p.scale);
    let service = crate::attach(world).map_err(internal)?;
    to_value(protocol::ViewportAttachResult { service, protocol: protocol::VERSION.into() })
}

fn detach(In(_): In<Option<Value>>, world: &mut World) -> BrpResult {
    let shared = world.resource::<SharedRes>().0.clone();
    let was = shared.service_name().is_some();
    shared.close();
    to_value(protocol::ViewportDetachResult { detached: was })
}

fn resize(In(p): In<Option<Value>>, world: &mut World) -> BrpResult {
    let p: protocol::ViewportResizeParams = required(p)?;
    if !(p.w > 0.0 && p.h > 0.0 && p.scale > 0.0) {
        return Err(invalid("w, h and scale must be positive"));
    }
    if let Some(hz) = p.hz {
        world.resource::<SharedRes>().0.set_hz(hz);
    }
    to_value(apply_size(world, p.w, p.h, p.scale))
}

fn stats(In(_): In<Option<Value>>, world: &mut World) -> BrpResult {
    let shared = world.resource::<SharedRes>().0.clone();
    let s = &shared.stats;
    Ok(json!({
        "attached": shared.attached(),
        "visible": shared.visible(),
        "mode": if shared.edit() { "edit" } else { "play" },
        "frames": s.frames.load(Ordering::Relaxed),
        "skipped": s.skipped.load(Ordering::Relaxed),
        "droppedTicks": s.dropped_ticks.load(Ordering::Relaxed),
        "gpuMs": s.gpu_ms(),
        "inputToFrame": s.latency(),
        "ring": shared.ring_info(),
        "size": window_size(world),
    }))
}

/// The primary window's size as the protocol reports it.
pub(crate) fn window_size(world: &mut World) -> Value {
    let mut q = world.query_filtered::<&Window, With<PrimaryWindow>>();
    match q.single(world) {
        Ok(w) => json!({
            "w": w.resolution.width(), "h": w.resolution.height(), "scale": w.resolution.scale_factor(),
            "pixelW": w.resolution.physical_width(), "pixelH": w.resolution.physical_height(),
        }),
        Err(_) => json!({ "w": 0, "h": 0, "scale": 1, "pixelW": 0, "pixelH": 0 }),
    }
}

/// Resizes the virtual window to `w` by `h` points at `scale`, writing the messages winit would.
pub(crate) fn apply_size(world: &mut World, w: f64, h: f64, scale: f64) -> Value {
    let mut q = world.query_filtered::<(Entity, &mut Window), With<VirtualWindow>>();
    let Ok((entity, mut window)) = q.single_mut(world) else { return window_size(world) };
    let (pw, ph) = ((w * scale).round().max(1.0) as u32, (h * scale).round().max(1.0) as u32);
    let scale_changed = (window.resolution.scale_factor() as f64 - scale).abs() > f64::EPSILON;
    let size_changed = window.resolution.physical_width() != pw || window.resolution.physical_height() != ph;
    if scale_changed {
        window.resolution.set_scale_factor(scale as f32);
    }
    if size_changed || scale_changed {
        window.resolution.set_physical_resolution(pw, ph);
    }
    let (lw, lh) = (window.resolution.width(), window.resolution.height());
    if scale_changed {
        let m = WindowScaleFactorChanged { window: entity, scale_factor: scale };
        world.write_message(m.clone());
        world.write_message(WindowEvent::WindowScaleFactorChanged(m));
    }
    if size_changed || scale_changed {
        let m = WindowResized { window: entity, width: lw, height: lh };
        world.write_message(m.clone());
        world.write_message(WindowEvent::WindowResized(m));
    }
    window_size(world)
}

/// Applies what arrived on the channel since the last frame, in order.
pub(crate) fn drain(world: &mut World) {
    let shared = world.resource::<SharedRes>().0.clone();
    for (at, m) in shared.drain() {
        match m {
            crate::mach::Incoming::Gone => shared.close(),
            crate::mach::Incoming::Message { method, params, port } => {
                if let Some(port) = port {
                    crate::mach::release_port(port);
                }
                match method.as_str() {
                    "resize" => match serde_json::from_value::<protocol::ViewportResizeParams>(params) {
                        Ok(p) if p.w > 0.0 && p.h > 0.0 && p.scale > 0.0 => {
                            if let Some(hz) = p.hz {
                                shared.set_hz(hz);
                            }
                            apply_size(world, p.w, p.h, p.scale);
                        }
                        _ => warn!("viewport: malformed resize"),
                    },
                    "input" => {
                        // One event per message: an array or anything else is malformed.
                        crate::input::queue_at(world, at, params);
                    }
                    "focus" => {
                        crate::input::queue_at(world, at, json!({ "type": "focus", "on": params["on"] }));
                    }
                    "visible" | "hello" | "bye" => {}
                    // A surface view's own messages (abyme's `command`, `menu`) and any later
                    // ones: not the engine's to read.
                    other => debug!("viewport: ignoring channel message {other}"),
                }
            }
            crate::mach::Incoming::Malformed(s) => warn!("viewport: malformed channel message {s}"),
            crate::mach::Incoming::Hello => {}
        }
    }
}

// ---- edit mode ----

use crate::edit::{self, EditState, Mode, Origin, Selection};
use bevy_ts_tsx::tree::TsxNode;

pub(crate) fn add_edit_methods(app: &mut App) {
    use bevy_ts_remote::{add_method, add_watching_method};
    add_method(app, CRATE, "bevy-ts/viewport_mode", mode);
    add_method(app, CRATE, "bevy-ts/viewport_camera", camera);
    add_method(app, CRATE, "bevy-ts/viewport_pick", pick);
    add_method(app, CRATE, "bevy-ts/viewport_select", select);
    add_watching_method(app, CRATE, "bevy-ts/viewport_selection+watch", selection_watch);
    app.init_resource::<Watchers>();
}

fn optional<T: serde::de::DeserializeOwned + Default>(p: Option<Value>) -> Result<T, BrpError> {
    match p {
        None | Some(Value::Null) => Ok(T::default()),
        Some(v) => serde_json::from_value(v).map_err(|e| invalid(format!("invalid params: {e}"))),
    }
}

fn mode(In(p): In<Option<Value>>, world: &mut World) -> BrpResult {
    let p: protocol::ViewportModeParams = optional(p)?;
    let want = match p.mode {
        Some(protocol::ViewportMode::Edit) => Mode::Edit,
        Some(protocol::ViewportMode::Play) => Mode::Play,
        None => world.resource::<EditState>().mode,
    };
    Ok(edit::set_mode(world, want, p.ui, p.gamepad_unfocused))
}

fn camera(In(p): In<Option<Value>>, world: &mut World) -> BrpResult {
    let p: protocol::ViewportCameraParams = optional(p)?;
    let Some(cam) = world.resource::<EditState>().camera else {
        return Err(invalid("no editor camera yet: enter edit mode first"));
    };
    if p.frame == Some(true) {
        edit::frame_selection(world);
    }
    if let Some(mut t) = world.get_mut::<Transform>(cam) {
        if let Some(v) = &p.translation {
            t.translation = Vec3::new(v[0] as f32, v[1] as f32, v[2] as f32);
        }
        if let Some(q) = &p.rotation {
            t.rotation = Quat::from_xyzw(q[0] as f32, q[1] as f32, q[2] as f32, q[3] as f32).normalize();
        }
    }
    if let Some(mut proj) = world.get_mut::<Projection>(cam) {
        match &mut *proj {
            Projection::Perspective(pp) => {
                if let Some(f) = p.fov {
                    pp.fov = f as f32;
                }
            }
            Projection::Orthographic(o) => {
                if let Some(s) = p.scale {
                    o.scale = s as f32;
                }
            }
            _ => {}
        }
    }
    if p.translation.is_some() || p.rotation.is_some() {
        let t = *world.get::<Transform>(cam).unwrap();
        let (yaw, pitch, _) = t.rotation.to_euler(EulerRot::YXZ);
        if let Some(mut c) = world.get_mut::<edit::EditorCamera>(cam) {
            let distance = (c.focus - t.translation).length().max(0.5);
            c.focus = t.translation + t.forward().as_vec3() * distance;
            c.yaw = yaw;
            c.pitch = pitch;
        }
    }
    let t = *world.get::<Transform>(cam).unwrap();
    let (fov, scale) = match world.get::<Projection>(cam) {
        Some(Projection::Perspective(p)) => (Some(p.fov), None),
        Some(Projection::Orthographic(o)) => (None, Some(o.scale)),
        _ => (None, None),
    };
    Ok(json!({
        "translation": t.translation.to_array(),
        "rotation": t.rotation.to_array(),
        "fov": fov,
        "scale": scale,
    }))
}

fn pick(In(p): In<Option<Value>>, world: &mut World) -> BrpResult {
    let p: protocol::ViewportPickParams = required(p)?;
    let Some(ray) = edit::editor_ray(world, Vec2::new(p.x as f32, p.y as f32)) else {
        return Err(invalid("no editor camera yet: enter edit mode first"));
    };
    let hits: Vec<Value> = edit::hits(world, ray, false)
        .into_iter()
        .map(|h| json!({ "entity": h.entity.to_bits(), "depth": h.depth, "selects": h.selects.to_bits() }))
        .collect();
    Ok(json!({ "hits": hits }))
}

fn select(In(p): In<Option<Value>>, world: &mut World) -> BrpResult {
    let p: protocol::ViewportSelectParams = optional(p)?;
    let entities: Vec<Entity> = match (p.entities, p.file) {
        (list, _) if !list.is_empty() => list
            .iter()
            .map(|b| Entity::try_from_bits(b.0).filter(|e| world.get_entity(*e).is_ok()).ok_or_else(|| invalid(format!("no entity {}", b.0))))
            .collect::<Result<_, _>>()?,
        (_, Some(file)) => {
            let line = p.line.ok_or_else(|| invalid("file needs line"))?;
            let found = call_tsx_find(world, &file, line, p.column)?;
            // The location's instance roots, one per instance, in tsx's order.
            let mut out: Vec<Entity> = vec![];
            for e in found {
                let r = edit::instance_root(world, e);
                if !out.contains(&r) {
                    out.push(r);
                }
            }
            out
        }
        _ => vec![],
    };
    let items = entities.into_iter().map(|e| (e, world.get::<TsxNode>(e).map(|n| n.id))).collect();
    world.resource_mut::<Selection>().set(items, Origin::Editor);
    Ok(selection_json(world))
}

/// `bevy-ts/tsx_find` as served, so the lookup is tsx's own.
fn call_tsx_find(world: &mut World, file: &str, line: u32, column: Option<u32>) -> Result<Vec<Entity>, BrpError> {
    let id = match world.resource::<bevy::remote::RemoteMethods>().get("bevy-ts/tsx_find") {
        Some(bevy::remote::RemoteMethodSystemId::Instant(id)) => *id,
        _ => return Err(invalid("selecting by location needs bevy-ts-tsx's methods")),
    };
    let mut params = json!({ "file": file, "line": line });
    if let Some(c) = column {
        params["column"] = json!(c);
    }
    let r = world.run_system_with(id, Some(params)).map_err(|e| internal(e.to_string()))??;
    let mut matches: Vec<(u64, Entity)> = r["matches"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|m| Some((m["depth"].as_u64()?, Entity::try_from_bits(m["entity"].as_u64()?)?)))
        .collect();
    // The shallowest depth first: an entity the location makes, before ones it is a call site of.
    matches.sort_by_key(|(d, _)| *d);
    Ok(matches.into_iter().map(|(_, e)| e).collect())
}

pub(crate) fn selection_json(world: &World) -> Value {
    let sel = world.resource::<Selection>();
    let items: Vec<Value> = sel
        .items
        .iter()
        .enumerate()
        .filter(|(_, (e, _))| world.get_entity(*e).is_ok())
        .map(|(i, (e, _))| {
            let node = world.get::<TsxNode>(*e);
            let mut v = json!({
                "entity": e.to_bits(),
                "name": world.get::<Name>(*e).map(|n| n.as_str().to_string()),
                "node": node.map(|n| bevy_ts_tsx::protocol::id_str(n.id)),
                "source": node.map(|n| n.source.clone()).unwrap_or_default(),
                "level": null,
                "instance": null,
                "primary": i == 0,
            });
            if i == 0 {
                v["routes"] = crate::edits::routes_json(world, *e);
            }
            v
        })
        .collect();
    json!({ "origin": sel.origin.name(), "items": items })
}

/// Selection watchers: params to (update last served, selection seq last sent).
#[derive(Resource, Default)]
struct Watchers(std::collections::HashMap<String, (u64, u64)>);

fn selection_watch(In(p): In<Option<Value>>, world: &mut World) -> BrpResult<Option<Value>> {
    let frame = world.resource::<crate::UpdateCount>().0;
    let key = p.as_ref().map(Value::to_string).unwrap_or_default();
    let seq = world.resource::<Selection>().seq;
    let known = world.resource::<Watchers>().0.get(&key).copied();
    world.resource_mut::<Watchers>().0.insert(key, (frame, seq));
    match known {
        Some((last, sent)) if last + 1 >= frame && sent == seq => Ok(None),
        _ => Ok(Some(selection_json(world))),
    }
}

// ---- edits ----

pub(crate) fn add_edit_result_methods(app: &mut App) {
    use bevy_ts_remote::{add_method, add_watching_method};
    add_watching_method(app, CRATE, "bevy-ts/viewport_edits+watch", edits_watch);
    add_method(app, CRATE, "bevy-ts/viewport_edit_result", edit_result);
    add_method(app, CRATE, "bevy-ts/viewport_revert", revert);
    app.init_resource::<EditWatchers>();
}

#[derive(Resource, Default)]
struct EditWatchers(std::collections::HashMap<String, (u64, u64)>);

fn edits_watch(In(p): In<Option<Value>>, world: &mut World) -> BrpResult<Option<Value>> {
    let frame = world.resource::<crate::UpdateCount>().0;
    let key = p.as_ref().map(Value::to_string).unwrap_or_default();
    let seq = world.resource::<crate::edits::EditLog>().seq;
    let known = world.resource::<EditWatchers>().0.get(&key).copied();
    world.resource_mut::<EditWatchers>().0.insert(key, (frame, seq));
    let since = match known {
        Some((last, sent)) if last + 1 >= frame => {
            if sent == seq {
                return Ok(None);
            }
            sent
        }
        // A new subscriber gets every edit of the session.
        _ => 0,
    };
    let edits: Vec<Value> = world.resource::<crate::edits::EditLog>().edits.iter().filter(|e| e.rev > since).map(crate::edits::edit_json).collect();
    if edits.is_empty() && since > 0 {
        return Ok(None);
    }
    Ok(Some(json!({ "edits": edits })))
}

fn edit_result(In(p): In<Option<Value>>, world: &mut World) -> BrpResult {
    let p: protocol::ViewportEditResultParams = required(p)?;
    let written = p.status == protocol::ViewportEditResultParamsStatus::Written;
    crate::edits::answer(world, p.id, written, p.reason).map_err(invalid)
}

fn revert(In(p): In<Option<Value>>, world: &mut World) -> BrpResult {
    let p: protocol::ViewportRevertParams = required(p)?;
    crate::edits::revert(world, p.id).map_err(invalid)
}
