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
                    other => warn!("viewport: unknown channel message {other}"),
                }
            }
            crate::mach::Incoming::Malformed(s) => warn!("viewport: malformed channel message {s}"),
            crate::mach::Incoming::Hello => {}
        }
    }
}
