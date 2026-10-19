//! Viewport input: events from the viewer become the Bevy 0.20 messages `bevy_winit` writes for
//! a window, applied in `First` so `bevy_input`, picking, UI and game code take one path.

use crate::keys;
use crate::shared::SharedRes;
use crate::VirtualWindow;
use bevy::input::ButtonState;
use bevy::input::gestures::{DoubleTapGesture, PinchGesture, RotationGesture};
use bevy::input::keyboard::{KeyboardFocusLost, KeyboardInput};
use bevy::input::mouse::{MouseButton, MouseButtonInput, MouseMotion, MouseScrollUnit, MouseWheel};
use bevy::input::touch::TouchPhase;
use bevy::prelude::*;
use bevy::window::{CursorEntered, CursorLeft, CursorMoved, Ime, WindowEvent, WindowFocused};
use bevy_ts_remote::protocol::{self as p, ViewportInput};
use serde_json::Value;
use std::time::Instant;

/// Events to apply this frame, with when each arrived.
#[derive(Resource, Default)]
pub struct PendingInput(pub Vec<(Instant, ViewportInput)>);

/// Parses one event and queues it; a malformed event is logged and dropped.
pub(crate) fn queue_at(world: &mut World, at: Instant, params: Value) -> bool {
    match serde_json::from_value::<ViewportInput>(params.clone()) {
        Ok(e) => {
            world.resource_mut::<PendingInput>().0.push((at, e));
            true
        }
        Err(_) => {
            warn!("viewport: malformed input event {params}");
            false
        }
    }
}

fn write(world: &mut World, e: impl Into<WindowEvent> + Message + Clone) {
    world.write_message(e.clone());
    world.write_message(e.into());
}

fn button(b: Option<p::PointerEventButton>) -> MouseButton {
    match b {
        Some(p::PointerEventButton::Right) => MouseButton::Right,
        Some(p::PointerEventButton::Middle) => MouseButton::Middle,
        Some(p::PointerEventButton::Back) => MouseButton::Back,
        Some(p::PointerEventButton::Forward) => MouseButton::Forward,
        Some(p::PointerEventButton::Left) | None => MouseButton::Left,
    }
}

/// Moves the cursor to `pos` (logical pixels, which are the viewer's points), writing
/// `CursorMoved` if it changed.
fn move_cursor(world: &mut World, window: Entity, pos: Vec2) {
    let Some(mut w) = world.get_mut::<Window>(window) else { return };
    let before = w.cursor_position();
    if before == Some(pos) {
        return;
    }
    w.set_cursor_position(Some(pos));
    let delta = before.map(|b| pos - b);
    world.resource_mut::<crate::feedback::Cursor>().set_by_viewer(pos);
    write(world, CursorMoved { window, position: pos, delta });
}

/// Applies the queued events in arrival order. Runs in `First`.
pub(crate) fn apply(world: &mut World) {
    let events = std::mem::take(&mut world.resource_mut::<PendingInput>().0);
    if events.is_empty() {
        return;
    }
    let mut q = world.query_filtered::<Entity, With<VirtualWindow>>();
    let Ok(window) = q.single(world) else { return };
    let shared = world.resource::<SharedRes>().0.clone();
    let update = world.resource::<crate::UpdateCount>().0;
    for (at, e) in events {
        shared.mark_input(update, at);
        apply_one(world, window, e);
    }
}

fn apply_one(world: &mut World, window: Entity, e: ViewportInput) {
    match e {
        ViewportInput::PointerEvent(e) => {
            let pos = Vec2::new(e.x as f32, e.y as f32);
            match e.kind {
                p::PointerEventKind::Enter => {
                    write(world, CursorEntered { window });
                    move_cursor(world, window, pos);
                }
                p::PointerEventKind::Leave => {
                    if let Some(mut w) = world.get_mut::<Window>(window) {
                        w.set_cursor_position(None);
                    }
                    write(world, CursorLeft { window });
                }
                p::PointerEventKind::Move => move_cursor(world, window, pos),
                p::PointerEventKind::Down | p::PointerEventKind::Up => {
                    move_cursor(world, window, pos);
                    let state = if e.kind == p::PointerEventKind::Down { ButtonState::Pressed } else { ButtonState::Released };
                    write(world, MouseButtonInput { button: button(e.button), state, window });
                }
            }
            if let (Some(dx), Some(dy)) = (e.dx, e.dy)
                && (dx != 0.0 || dy != 0.0)
            {
                write(world, MouseMotion { delta: Vec2::new(dx as f32, dy as f32) });
            }
        }
        ViewportInput::WheelEvent(e) => {
            if let (Some(x), Some(y)) = (e.x, e.y) {
                move_cursor(world, window, Vec2::new(x as f32, y as f32));
            }
            let unit = match e.unit {
                p::WheelEventUnit::Line => MouseScrollUnit::Line,
                p::WheelEventUnit::Pixel => MouseScrollUnit::Pixel,
            };
            let phase = match e.phase {
                Some(p::WheelEventPhase::Started) => TouchPhase::Started,
                Some(p::WheelEventPhase::Ended) => TouchPhase::Ended,
                Some(p::WheelEventPhase::Cancelled) => TouchPhase::Canceled,
                Some(p::WheelEventPhase::Moved) | None => TouchPhase::Moved,
            };
            write(world, MouseWheel { unit, x: e.dx as f32, y: e.dy as f32, window, phase });
        }
        ViewportInput::GestureEvent(e) => {
            let d = e.delta.unwrap_or(0.0) as f32;
            match e.kind {
                p::GestureEventKind::Pinch => write(world, PinchGesture(d)),
                p::GestureEventKind::Rotate => write(world, RotationGesture(d)),
                p::GestureEventKind::DoubleTap => write(world, DoubleTapGesture),
            }
        }
        ViewportInput::KeyEvent(e) => {
            let Some(key_code) = keys::key_code(&e.code) else {
                warn!("viewport: unknown key code {}", e.code);
                return;
            };
            let logical_key = keys::logical_key(e.key.as_deref().unwrap_or(""));
            let state = if e.state == p::KeyEventState::Down { ButtonState::Pressed } else { ButtonState::Released };
            let text = e.text.filter(|t| !t.is_empty() && state == ButtonState::Pressed).map(Into::into);
            write(world, KeyboardInput { key_code, logical_key, state, text, repeat: e.repeat.unwrap_or(false), window });
        }
        ViewportInput::ImeEvent(e) => {
            let value = e.text.unwrap_or_default();
            let ime = match e.kind {
                p::ImeEventKind::Enabled => Ime::Enabled { window },
                p::ImeEventKind::Disabled => Ime::Disabled { window },
                p::ImeEventKind::Commit => Ime::Commit { window, value },
                p::ImeEventKind::Preedit => {
                    let cursor = e.cursor.and_then(|c| (c.len() == 2).then(|| (c[0] as usize, c[1] as usize)));
                    Ime::Preedit { window, value, cursor }
                }
            };
            write(world, ime);
        }
        ViewportInput::FocusEvent(e) => {
            if let Some(mut w) = world.get_mut::<Window>(window) {
                w.focused = e.on;
            }
            world.resource::<SharedRes>().0.set_focused(e.on);
            if !e.on {
                // As winit does: the game sees every held key released.
                world.write_message(KeyboardFocusLost);
                world.write_message(WindowEvent::KeyboardFocusLost(KeyboardFocusLost));
            }
            write(world, WindowFocused { window, focused: e.on });
        }
        ViewportInput::VisibleEvent(e) => world.resource::<SharedRes>().0.set_visible(e.on),
    }
}

/// Gamepads are read in the engine (gilrs), not forwarded; while the hole has no focus their
/// buttons and axes do not reach the game unless `viewport_mode` set `gamepadUnfocused`.
/// Connections always pass, so a pad plugged in meanwhile is known when focus returns.
pub(crate) fn gate_gamepads(shared: Res<SharedRes>, mut raw: ResMut<Messages<bevy::input::gamepad::RawGamepadEvent>>) {
    use bevy::input::gamepad::RawGamepadEvent;
    if shared.0.focused() || shared.0.gamepad_unfocused() {
        return;
    }
    let kept: Vec<_> = raw.drain().filter(|e| matches!(e, RawGamepadEvent::Connection(_))).collect();
    raw.write_batch(kept);
}
