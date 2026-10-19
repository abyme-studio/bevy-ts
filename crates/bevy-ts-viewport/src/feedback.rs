//! What the engine tells the viewer, which owns the real cursor and IME: `cursor`,
//! `cursor_grab`, `cursor_warp`, `ime_area` and `ime_allowed`, each sent when the virtual
//! window's field changes.

use crate::shared::SharedRes;
use crate::VirtualWindow;
use bevy::prelude::*;
use bevy::window::{CursorGrabMode, CursorIcon, CursorOptions};
use serde_json::json;

/// The last state sent, and where the viewer last put the cursor (not a warp).
#[derive(Resource, Default)]
pub(crate) struct Cursor {
    icon: Option<String>,
    grab: Option<CursorGrabMode>,
    ime_area: Option<Vec2>,
    ime_allowed: Option<bool>,
    viewer_pos: Option<Vec2>,
}

impl Cursor {
    pub(crate) fn set_by_viewer(&mut self, pos: Vec2) {
        self.viewer_pos = Some(pos);
    }
}

/// A CSS cursor name for an icon: winit's names, kebab-cased (`EResize` is `e-resize`).
pub(crate) fn css_name(icon: &CursorIcon) -> String {
    let debug = match icon {
        CursorIcon::System(s) => format!("{s:?}"),
        #[allow(unreachable_patterns)]
        _ => return "default".into(),
    };
    let mut out = String::new();
    for (i, c) in debug.chars().enumerate() {
        if c.is_ascii_uppercase() {
            if i > 0 {
                out.push('-');
            }
            out.push(c.to_ascii_lowercase());
        } else {
            out.push(c);
        }
    }
    out
}

/// Runs in `Last`: sends what changed this frame.
pub(crate) fn send_changes(
    shared: Res<SharedRes>,
    mut state: ResMut<Cursor>,
    windows: Query<(&Window, &CursorOptions, Option<&CursorIcon>), With<VirtualWindow>>,
) {
    let Ok((window, options, icon)) = windows.single() else { return };
    let shared = &shared.0;
    if !shared.attached() {
        *state = Cursor { viewer_pos: state.viewer_pos, ..default() };
        return;
    }
    let name = if options.visible { icon.map_or_else(|| "default".to_string(), css_name) } else { "hidden".to_string() };
    if state.icon.as_ref() != Some(&name) {
        shared.send("cursor", json!({ "icon": name }));
        state.icon = Some(name);
    }
    if state.grab != Some(options.grab_mode) {
        let mode = match options.grab_mode {
            CursorGrabMode::None => "none",
            CursorGrabMode::Confined => "confined",
            CursorGrabMode::Locked => "locked",
        };
        shared.send("cursor_grab", json!({ "mode": mode }));
        state.grab = Some(options.grab_mode);
    }
    // Game code moved the cursor if it is somewhere the viewer did not put it.
    if let Some(pos) = window.cursor_position()
        && state.viewer_pos != Some(pos)
    {
        if state.viewer_pos.is_some() {
            shared.send("cursor_warp", json!({ "x": pos.x, "y": pos.y }));
        }
        state.viewer_pos = Some(pos);
    }
    if state.ime_allowed != Some(window.ime_enabled) {
        shared.send("ime_allowed", json!({ "on": window.ime_enabled }));
        state.ime_allowed = Some(window.ime_enabled);
    }
    if window.ime_enabled && state.ime_area != Some(window.ime_position) {
        let p = window.ime_position;
        shared.send("ime_area", json!({ "x": p.x, "y": p.y, "w": 1, "h": 20 }));
        state.ime_area = Some(p);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::window::SystemCursorIcon;

    #[test]
    fn cursor_names_are_css() {
        assert_eq!(css_name(&CursorIcon::System(SystemCursorIcon::EResize)), "e-resize");
        assert_eq!(css_name(&CursorIcon::System(SystemCursorIcon::ContextMenu)), "context-menu");
        assert_eq!(css_name(&CursorIcon::System(SystemCursorIcon::Pointer)), "pointer");
    }
}
