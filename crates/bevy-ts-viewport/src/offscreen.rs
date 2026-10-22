//! The headless pointer (openspec change `mesh-picking`): `bevy-ts/viewport_input` under
//! `bevy-ts dev --headless`, for `pointer` events. They drive one Bevy custom pointer aimed at the
//! offscreen image the cameras render into, as Bevy's `render_ui_to_texture` example drives one on
//! a texture, so `bevy_picking` hits and `onPointer*` handlers fire as in a window. Built on every
//! platform.
//!
//! Workaround, to drop once Bevy fixes it: Bevy 0.20's `Location::is_in_viewport`
//! (`bevy_picking/src/pointer.rs`) returns false without a `PrimaryWindow`, even for a pointer on an
//! image, so the plugin spawns a window-less primary window the size of the image.

use crate::CRATE;
use bevy::asset::uuid::Uuid;
use bevy::camera::{ImageRenderTarget, NormalizedRenderTarget};
use bevy::picking::PickingSystems;
use bevy::picking::pointer::{Location, PointerAction, PointerButton, PointerId, PointerInput, PointerLocation};
use bevy::prelude::*;
use bevy::remote::{BrpError, BrpResult, error_codes};
use bevy_ts_remote::protocol::{PointerEvent, PointerEventButton, PointerEventKind, ViewportInput, ViewportInputParams, ViewportInputResult};
use serde_json::Value;
use std::collections::VecDeque;

/// The headless pointer's id.
pub const POINTER: PointerId = PointerId::Custom(Uuid::from_u128(0x6265_7679_2d74_732d_6865_6164_6c65_7373));

/// Serves `bevy-ts/viewport_input` for pointer events on `image`, at scale 1. Never added beside
/// `ViewportPlugin`, which serves the method under `--viewport`.
pub struct OffscreenPointerPlugin {
    pub image: Handle<Image>,
}

/// The image the pointer moves over.
#[derive(Resource)]
struct Target(NormalizedRenderTarget);

/// Pointer events not yet applied, oldest first.
#[derive(Resource, Default)]
pub struct Pending(pub VecDeque<PointerEvent>);

impl Plugin for OffscreenPointerPlugin {
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
            Some("pointer") => {}
            Some(t) => return Err(invalid(format!("headless takes pointer events only, not {t}"))),
            None => return Err(invalid("an event has no type")),
        }
    }
    let p: ViewportInputParams = serde_json::from_value(p).map_err(|e| invalid(format!("invalid params: {e}")))?;
    if let Some(v) = p.viewport.filter(|v| *v != 0) {
        return Err(invalid(format!("no viewport {v}: headless has only the game's, 0")));
    }
    let events: Vec<PointerEvent> = p
        .events
        .into_iter()
        .filter_map(|e| match e {
            ViewportInput::PointerEvent(e) => Some(e),
            _ => None,
        })
        .collect();
    let n = events.len() as u32;
    pending.0.extend(events);
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

/// Applies one queued event a frame: picking's hover comes from the frame before, so a press in
/// the frame of the move that brought the pointer over a mesh would hit nothing. A `down` or `up`
/// away from the pointer, or with no pointer yet, first moves it there, as a viewer's does, and
/// waits a frame. `leave` cancels, and the next frame takes the pointer off the image: Bevy keeps
/// a cancelled pointer's location and would cast from it again, but sends `PointerOut` only to a
/// pointer that still has one.
fn apply(
    mut pending: ResMut<Pending>,
    target: Res<Target>,
    mut pointers: Query<(&PointerId, &mut PointerLocation)>,
    mut last: Local<Option<Vec2>>,
    mut left: Local<bool>,
    mut out: MessageWriter<PointerInput>,
) {
    if std::mem::take(&mut *left) {
        for (id, mut at) in &mut pointers {
            if *id == POINTER {
                at.location = None;
            }
        }
    }
    let Some(e) = pending.0.front() else { return };
    let position = Vec2::new(e.x as f32, e.y as f32);
    let location = Location { target: target.0.clone(), position };
    let moves = matches!(e.kind, PointerEventKind::Move | PointerEventKind::Enter);
    if !moves && e.kind != PointerEventKind::Leave && *last != Some(position) {
        out.write(PointerInput::new(POINTER, location, PointerAction::Move { delta: position - last.unwrap_or(position) }));
        *last = Some(position);
        return;
    }
    let e = pending.0.pop_front().expect("front is some");
    let action = match e.kind {
        PointerEventKind::Move | PointerEventKind::Enter => PointerAction::Move { delta: position - last.unwrap_or(position) },
        PointerEventKind::Down => match button(e.button) {
            Some(b) => PointerAction::Press(b),
            None => return,
        },
        PointerEventKind::Up => match button(e.button) {
            Some(b) => PointerAction::Release(b),
            None => return,
        },
        PointerEventKind::Leave => {
            *left = true;
            *last = None;
            out.write(PointerInput::new(POINTER, location, PointerAction::Cancel));
            return;
        }
    };
    *last = Some(position);
    out.write(PointerInput::new(POINTER, location, action));
}
