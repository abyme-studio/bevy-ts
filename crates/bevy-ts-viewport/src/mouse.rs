//! `pointer`, `wheel` and `gesture` events to the Bevy 0.20 messages `bevy_winit` writes for a
//! window: one owner for `--viewport` ([`crate::input`]) and `--headless` ([`crate::offscreen`]).
//! Built on every platform.

use crate::keyboard::write;
use bevy::input::ButtonState;
use bevy::input::gestures::{DoubleTapGesture, PinchGesture, RotationGesture};
use bevy::input::mouse::{MouseButton, MouseButtonInput, MouseMotion, MouseScrollUnit, MouseWheel};
use bevy::input::touch::TouchPhase;
use bevy::prelude::*;
use bevy::window::{CursorEntered, CursorLeft, CursorMoved};
use bevy_ts_remote::protocol as p;

fn button(b: Option<p::PointerEventButton>) -> MouseButton {
    match b {
        Some(p::PointerEventButton::Right) => MouseButton::Right,
        Some(p::PointerEventButton::Middle) => MouseButton::Middle,
        Some(p::PointerEventButton::Back) => MouseButton::Back,
        Some(p::PointerEventButton::Forward) => MouseButton::Forward,
        Some(p::PointerEventButton::Left) | None => MouseButton::Left,
    }
}

/// Moves `window`'s cursor to `pos` (logical pixels), writing `CursorMoved` if it changed; where
/// it moved to, if it did.
pub fn move_cursor(world: &mut World, window: Entity, pos: Vec2) -> Option<Vec2> {
    let mut w = world.get_mut::<Window>(window)?;
    let before = w.cursor_position();
    if before == Some(pos) {
        return None;
    }
    w.set_cursor_position(Some(pos));
    write(world, CursorMoved { window, position: pos, delta: before.map(|b| pos - b) });
    Some(pos)
}

/// A `pointer` event on `window`: the cursor moves, enters or leaves, and a button is pressed or
/// released where it is. Where the cursor moved to, if it did.
pub fn pointer(world: &mut World, window: Entity, e: &p::PointerEvent) -> Option<Vec2> {
    let pos = Vec2::new(e.x as f32, e.y as f32);
    let moved = match e.kind {
        p::PointerEventKind::Enter => {
            write(world, CursorEntered { window });
            move_cursor(world, window, pos)
        }
        p::PointerEventKind::Leave => {
            if let Some(mut w) = world.get_mut::<Window>(window) {
                w.set_cursor_position(None);
            }
            write(world, CursorLeft { window });
            None
        }
        p::PointerEventKind::Move => move_cursor(world, window, pos),
        p::PointerEventKind::Down | p::PointerEventKind::Up => {
            let moved = move_cursor(world, window, pos);
            let state = if e.kind == p::PointerEventKind::Down { ButtonState::Pressed } else { ButtonState::Released };
            write(world, MouseButtonInput { button: button(e.button), state, window });
            moved
        }
    };
    if let (Some(dx), Some(dy)) = (e.dx, e.dy)
        && (dx != 0.0 || dy != 0.0)
    {
        write(world, MouseMotion { delta: Vec2::new(dx as f32, dy as f32) });
    }
    moved
}

/// A `wheel` event as `MouseWheel` on `window`, the cursor first moved to its `x`, `y` when it has
/// them. Where the cursor moved to, if it did.
pub fn wheel(world: &mut World, window: Entity, e: &p::WheelEvent) -> Option<Vec2> {
    let moved = match (e.x, e.y) {
        (Some(x), Some(y)) => move_cursor(world, window, Vec2::new(x as f32, y as f32)),
        _ => None,
    };
    write(world, mouse_wheel(window, e));
    moved
}

/// The `MouseWheel` a `wheel` event is on `window`.
pub fn mouse_wheel(window: Entity, e: &p::WheelEvent) -> MouseWheel {
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
    MouseWheel { unit, x: e.dx as f32, y: e.dy as f32, window, phase }
}

/// A `gesture` event as `PinchGesture`, `RotationGesture` or `DoubleTapGesture`.
pub fn gesture(world: &mut World, e: &p::GestureEvent) {
    let d = e.delta.unwrap_or(0.0) as f32;
    match e.kind {
        p::GestureEventKind::Pinch => write(world, PinchGesture(d)),
        p::GestureEventKind::Rotate => write(world, RotationGesture(d)),
        p::GestureEventKind::DoubleTap => write(world, DoubleTapGesture),
    }
}
