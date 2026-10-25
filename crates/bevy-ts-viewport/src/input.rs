//! Viewport input: the viewer's events, applied by `bevy_ts_embed::input` to the virtual window,
//! plus what only the viewport has: the viewer's focus and visibility, the cursor it is shown, the
//! input-to-frame marks, and gamepads gated on focus.

use crate::shared::SharedRes;
use crate::VirtualWindow;
use bevy::prelude::*;
pub use bevy_ts_embed::input::{INPUTS, Modifiers, PendingInput, Remote, queue_at};

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
        let applied = bevy_ts_embed::input::apply_one(world, window, e);
        if let Some(pos) = applied.cursor {
            world.resource_mut::<crate::feedback::Cursor>().set_by_viewer(pos);
        }
        if let Some(on) = applied.focused {
            shared.set_focused(on);
        }
        if let Some(on) = applied.visible {
            shared.set_visible(on);
        }
    }
}

/// Gamepads are read in the engine (gilrs), and a viewer's arrive as `gamepad` input written as
/// gilrs writes them; while the hole has no focus their
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

