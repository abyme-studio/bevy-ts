//! `key` and `ime` events to the Bevy 0.20 messages `bevy_winit` writes for a window: one owner for
//! `--viewport` ([`crate::input`]) and `--headless` ([`crate::offscreen`]). Built on every
//! platform. `bevy_ts::keys` names codes and keys.

use bevy::input::ButtonState;
use bevy::input::keyboard::KeyboardInput;
use bevy::prelude::*;
use bevy::window::{Ime, WindowEvent};
use bevy_ts::keys;
use bevy_ts_remote::protocol as p;

/// Writes `e` and its `WindowEvent`, as `bevy_winit` does.
pub(crate) fn write(world: &mut World, e: impl Into<WindowEvent> + Message + Clone) {
    world.write_message(e.clone());
    world.write_message(e.into());
}

/// A `key` event as `KeyboardInput` on `window`; text only on a press. An unknown `code` is
/// dropped with a warning.
pub fn key(world: &mut World, window: Entity, e: p::KeyEvent) {
    let Some(key_code) = keys::key_code(&e.code) else {
        warn!("viewport: unknown key code {}", e.code);
        return;
    };
    let logical_key = keys::logical_key(e.key.as_deref().unwrap_or(""));
    let state = if e.state == p::KeyEventState::Down { ButtonState::Pressed } else { ButtonState::Released };
    let text = e.text.filter(|t| !t.is_empty() && state == ButtonState::Pressed).map(Into::into);
    write(world, KeyboardInput { key_code, logical_key, state, text, repeat: e.repeat.unwrap_or(false), window });
}

/// An `ime` event as `Ime` on `window`.
pub fn ime(world: &mut World, window: Entity, e: p::ImeEvent) {
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
