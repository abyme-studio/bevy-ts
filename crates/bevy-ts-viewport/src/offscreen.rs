//! Headless input (openspec changes `mesh-picking`, `headless-keys`, `headless-wheel`):
//! `bevy-ts/viewport_input` under `bevy-ts dev --headless`, for `pointer`, `wheel`, `gesture`,
//! `key` and `ime` events. Pointer and wheel events drive one Bevy custom pointer aimed at the
//! offscreen image the cameras render into, as Bevy's `render_ui_to_texture` example drives one on
//! a texture, so `bevy_picking` hits and `onPointer*` handlers fire as in a window. Every event
//! also becomes the messages a window gets, on the primary window, through [`crate::mouse`] and
//! [`crate::keyboard`] as under `--viewport`. Built on every platform.
//!
//! The plugin spawns a window-less primary window the size of the image, for two reasons. Bevy
//! 0.20's `Location::is_in_viewport` (`bevy_picking/src/pointer.rs`) returns false without a
//! `PrimaryWindow`, even for a pointer on an image: a workaround, to drop once Bevy fixes it. And
//! `dispatch_focused_input` sends keys to `InputFocus` only with a `PrimaryWindow`: drop the window
//! only with another for keys.

use crate::CRATE;
use bevy::asset::uuid::Uuid;
use bevy::camera::{ImageRenderTarget, NormalizedRenderTarget};
use bevy::picking::PickingSystems;
use bevy::picking::pointer::{Location, PointerAction, PointerButton, PointerId, PointerInput, PointerLocation};
use bevy::prelude::*;
use bevy::remote::{BrpError, BrpResult, error_codes};
use bevy::window::PrimaryWindow;
use bevy::input::mouse::MouseWheel;
use bevy_ts_remote::protocol::{self as p, PointerEventButton, PointerEventKind, ViewportInput, ViewportInputParams, ViewportInputResult};
use serde_json::Value;
use std::collections::VecDeque;

/// The headless pointer's id.
pub const POINTER: PointerId = PointerId::Custom(Uuid::from_u128(0x6265_7679_2d74_732d_6865_6164_6c65_7373));

/// Serves `bevy-ts/viewport_input` for pointer and wheel events on `image`, at scale 1, and
/// gesture, key and IME events. Never added beside `ViewportPlugin`, which serves the method under `--viewport`.
pub struct OffscreenInputPlugin {
    pub image: Handle<Image>,
}

/// The image the pointer moves over.
#[derive(Resource)]
struct Target(NormalizedRenderTarget);

/// Events not yet applied, oldest first.
#[derive(Resource, Default)]
pub struct Pending(pub VecDeque<ViewportInput>);

impl Plugin for OffscreenInputPlugin {
    fn build(&self, app: &mut App) {
        let target = NormalizedRenderTarget::Image(ImageRenderTarget::from(self.image.clone()));
        let size = app.world().resource::<Assets<Image>>().get(&self.image).map_or(UVec2::ONE, |i| i.size());
        crate::spawn_windowless_primary(app.world_mut(), "bevy-ts headless", (size.x, size.y, 1.0), ());
        app.insert_resource(Target(target))
            .init_resource::<Pending>()
            .add_systems(Startup, |mut commands: Commands| {
                commands.spawn((POINTER, PointerLocation::default()));
            })
            .add_systems(First, apply.in_set(PickingSystems::Input));
    }

    fn finish(&self, app: &mut App) {
        // `RemotePlugin`, added in any order, has made `RemoteMethods` by now.
        if app.world().contains_resource::<bevy::remote::RemoteMethods>() {
            bevy_ts_remote::add_method(app, CRATE, "bevy-ts/viewport_input", input);
        }
    }
}

fn invalid(message: impl Into<String>) -> BrpError {
    BrpError { code: error_codes::INVALID_PARAMS, message: message.into(), data: None }
}

/// Checks every event before queueing any: one refused event refuses the request. The `type` is
/// read before the event is parsed, so a refused one is named even when malformed.
fn input(In(p): In<Option<Value>>, mut pending: ResMut<Pending>) -> BrpResult {
    let p = p.ok_or_else(|| invalid("params are required"))?;
    for e in p.get("events").and_then(Value::as_array).into_iter().flatten() {
        match e.get("type").and_then(Value::as_str) {
            Some("pointer" | "wheel" | "gesture" | "key" | "ime") => {}
            Some(t) => return Err(invalid(format!("headless takes pointer, wheel, gesture, key and ime events, not {t}"))),
            None => return Err(invalid("an event has no type")),
        }
    }
    let p: ViewportInputParams = serde_json::from_value(p).map_err(|e| invalid(format!("invalid params: {e}")))?;
    if let Some(v) = p.viewport.filter(|v| *v != 0) {
        return Err(invalid(format!("no viewport {v}: headless has only the game's, 0")));
    }
    let n = p.events.len() as u32;
    pending.0.extend(p.events);
    serde_json::to_value(ViewportInputResult { queued: n })
        .map_err(|e| BrpError { code: error_codes::INTERNAL_ERROR, message: e.to_string(), data: None })
}

fn button(b: Option<PointerEventButton>) -> Option<PointerButton> {
    match b.unwrap_or(PointerEventButton::Left) {
        PointerEventButton::Left => Some(PointerButton::Primary),
        PointerEventButton::Right => Some(PointerButton::Secondary),
        PointerEventButton::Middle => Some(PointerButton::Middle),
        PointerEventButton::Back | PointerEventButton::Forward => None,
    }
}

/// Applies one queued event a frame, in arrival order, so a key queued after a click lands after
/// it. Each goes to the primary window through [`crate::mouse`] and [`crate::keyboard`], as under
/// `--viewport`, so the window's cursor follows the pointer; pointer and wheel events also drive
/// [`POINTER`], a wheel as its `Scroll` where it is.
///
/// Picking's hover comes from the frame before, so a press in the frame of the move that brought
/// the pointer over a mesh would hit nothing. A `down`, `up` or placed `wheel` away from the
/// pointer, or with no pointer yet, first moves it there, as a viewer's does, and waits a frame.
/// `leave` cancels, and the next frame takes the pointer off the image: Bevy keeps a cancelled
/// pointer's location and would cast from it again, but sends `PointerOut` only to a pointer that
/// still has one.
fn apply(world: &mut World, mut last: Local<Option<Vec2>>, mut left: Local<bool>) {
    if std::mem::take(&mut *left) {
        let mut pointers = world.query::<(&PointerId, &mut PointerLocation)>();
        for (id, mut at) in pointers.iter_mut(world) {
            if *id == POINTER {
                at.location = None;
            }
        }
    }
    let Some(front) = world.resource::<Pending>().0.front() else { return };
    let placed = match front {
        ViewportInput::PointerEvent(e) if matches!(e.kind, PointerEventKind::Down | PointerEventKind::Up) => Some(Vec2::new(e.x as f32, e.y as f32)),
        ViewportInput::WheelEvent(p::WheelEvent { x: Some(x), y: Some(y), .. }) => Some(Vec2::new(*x as f32, *y as f32)),
        _ => None,
    };
    let mut windows = world.query_filtered::<Entity, With<PrimaryWindow>>();
    let Ok(window) = windows.single(world) else { return };
    let target = world.resource::<Target>().0.clone();
    let at = |position| Location { target: target.clone(), position };
    if let Some(position) = placed
        && *last != Some(position)
    {
        world.write_message(PointerInput::new(POINTER, at(position), PointerAction::Move { delta: position - last.unwrap_or(position) }));
        crate::mouse::move_cursor(world, window, position);
        *last = Some(position);
        return;
    }
    let e = world.resource_mut::<Pending>().0.pop_front().expect("front is some");
    let (position, action) = match e {
        ViewportInput::PointerEvent(e) => {
            crate::mouse::pointer(world, window, &e);
            let position = Vec2::new(e.x as f32, e.y as f32);
            let action = match e.kind {
                PointerEventKind::Move | PointerEventKind::Enter => PointerAction::Move { delta: position - last.unwrap_or(position) },
                PointerEventKind::Down => match self::button(e.button) {
                    Some(b) => PointerAction::Press(b),
                    None => return,
                },
                PointerEventKind::Up => match self::button(e.button) {
                    Some(b) => PointerAction::Release(b),
                    None => return,
                },
                PointerEventKind::Leave => {
                    *left = true;
                    *last = None;
                    world.write_message(PointerInput::new(POINTER, at(position), PointerAction::Cancel));
                    return;
                }
            };
            *last = Some(position);
            (position, action)
        }
        ViewportInput::WheelEvent(e) => {
            crate::mouse::wheel(world, window, &e);
            let Some(position) = *last else { return };
            let MouseWheel { unit, x, y, phase, .. } = crate::mouse::mouse_wheel(window, &e);
            (position, PointerAction::Scroll { unit, x, y, phase })
        }
        ViewportInput::GestureEvent(e) => return crate::mouse::gesture(world, &e),
        ViewportInput::KeyEvent(e) => return crate::keyboard::key(world, window, e),
        ViewportInput::ImeEvent(e) => return crate::keyboard::ime(world, window, e),
        _ => return,
    };
    world.write_message(PointerInput::new(POINTER, at(position), action));
}
