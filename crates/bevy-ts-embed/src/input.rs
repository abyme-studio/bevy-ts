//! SURFACE.md input (`ViewportInput`): events from a viewer or an embedding host become the Bevy
//! 0.20 messages `bevy_winit` writes for a window, applied in `First` so `bevy_input`, picking, UI
//! and game code take one path. The viewport and the embedding apply them the same way; the
//! viewport adds what only its viewer has (focus and visibility it reports back, the cursor).

use bevy_ts::keys;
use bevy::input::ButtonState;
use bevy::input::gestures::{DoubleTapGesture, PinchGesture, RotationGesture};
use bevy::input::keyboard::{KeyboardFocusLost, KeyboardInput};
use bevy::input::mouse::{MouseButton, MouseButtonInput, MouseMotion, MouseScrollUnit, MouseWheel};
use bevy::input::gamepad::{
    GamepadAxis, GamepadButton, GamepadConnection, GamepadConnectionEvent, RawGamepadAxisChangedEvent, RawGamepadButtonChangedEvent,
    RawGamepadEvent,
};
use bevy::input::touch::{ForceTouch, TouchInput, TouchPhase};
use bevy::prelude::*;
use bevy::window::{CursorEntered, CursorLeft, CursorMoved, Ime, WindowEvent, WindowFocused};
use bevy_ts_remote::protocol::{self as p, ViewportInput};
use serde_json::Value;
use std::collections::BTreeMap;
use std::time::Instant;

/// The window events apply to: the embedding's window, or the viewport's virtual one.
#[derive(Component, Default)]
pub struct HostWindow;

/// The `input` types the engine reads, which a viewer's `hello` is answered with and an
/// embedding's `started` names.
pub const INPUTS: &[&str] = &["pointer", "wheel", "gesture", "key", "ime", "touch", "gamepad", "focus", "visible"];

/// Events to apply this frame, with when each arrived.
#[derive(Resource, Default)]
pub struct PendingInput(pub Vec<(Instant, ViewportInput)>);

/// What the viewer last said its touches and gamepads were: it sends each as a whole, and Bevy
/// wants what changed.
#[derive(Resource, Default)]
pub struct Remote {
    touches: BTreeMap<u64, Vec2>,
    pads: BTreeMap<u32, Pad>,
}

/// A viewer's gamepad: its own entity, as gilrs gives each pad, and its last buttons and axes.
struct Pad {
    entity: Entity,
    connected: bool,
    buttons: Vec<f32>,
    axes: Vec<f32>,
}

/// The modifiers on the viewer's last pointer, wheel or key event: what an `NSEvent` says is
/// held, which the edit tools read alongside `ButtonInput<KeyCode>`.
#[derive(Resource, Default, Clone, Copy, Debug)]
pub struct Modifiers {
    pub shift: bool,
    pub ctrl: bool,
    pub alt: bool,
    pub meta: bool,
}

impl Modifiers {
    fn from(m: &Option<p::Modifiers>) -> Modifiers {
        let m = m.clone().unwrap_or_default();
        Modifiers { shift: m.shift.unwrap_or(false), ctrl: m.ctrl.unwrap_or(false), alt: m.alt.unwrap_or(false), meta: m.meta.unwrap_or(false) }
    }
}

/// Parses one event and queues it; a malformed event is logged and dropped.
pub fn queue_at(world: &mut World, at: Instant, params: Value) -> bool {
    match serde_json::from_value::<ViewportInput>(params.clone()) {
        Ok(e) => {
            world.resource_mut::<PendingInput>().0.push((at, e));
            true
        }
        Err(_) => {
            warn!("input: malformed input event {params}");
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

/// What an event changed that only its carrier acts on: the viewport tells its viewer, an
/// embedding has nothing to tell.
#[derive(Default, Debug, PartialEq)]
pub struct Applied {
    /// The cursor moved here, in logical pixels.
    pub cursor: Option<Vec2>,
    /// A `focus` event's `on`.
    pub focused: Option<bool>,
    /// A `visible` event's `on`.
    pub visible: Option<bool>,
}

/// Moves the cursor to `pos` (logical pixels, which are the viewer's points), writing
/// `CursorMoved` if it changed.
fn move_cursor(world: &mut World, window: Entity, pos: Vec2, applied: &mut Applied) {
    let Some(mut w) = world.get_mut::<Window>(window) else { return };
    let before = w.cursor_position();
    if before == Some(pos) {
        return;
    }
    w.set_cursor_position(Some(pos));
    let delta = before.map(|b| pos - b);
    applied.cursor = Some(pos);
    write(world, CursorMoved { window, position: pos, delta });
}

/// Applies the queued events in arrival order to the [`HostWindow`]. Runs in `First`.
pub fn apply(world: &mut World) {
    let events = std::mem::take(&mut world.resource_mut::<PendingInput>().0);
    if events.is_empty() {
        return;
    }
    let mut q = world.query_filtered::<Entity, With<HostWindow>>();
    let Ok(window) = q.single(world) else { return };
    for (_, e) in events {
        apply_one(world, window, e);
    }
}

/// Applies one event to `window`.
pub fn apply_one(world: &mut World, window: Entity, e: ViewportInput) -> Applied {
    let mut applied = Applied::default();
    let mods = match &e {
        ViewportInput::PointerEvent(e) => Some(Modifiers::from(&e.mods)),
        ViewportInput::WheelEvent(e) => Some(Modifiers::from(&e.mods)),
        ViewportInput::KeyEvent(e) => Some(Modifiers::from(&e.mods)),
        ViewportInput::TouchEvent(e) => Some(Modifiers::from(&e.mods)),
        _ => None,
    };
    if let Some(m) = mods {
        world.insert_resource(m);
    }
    match e {
        ViewportInput::PointerEvent(e) => {
            let pos = Vec2::new(e.x as f32, e.y as f32);
            match e.kind {
                p::PointerEventKind::Enter => {
                    write(world, CursorEntered { window });
                    move_cursor(world, window, pos, &mut applied);
                }
                p::PointerEventKind::Leave => {
                    if let Some(mut w) = world.get_mut::<Window>(window) {
                        w.set_cursor_position(None);
                    }
                    write(world, CursorLeft { window });
                }
                p::PointerEventKind::Move => move_cursor(world, window, pos, &mut applied),
                p::PointerEventKind::Down | p::PointerEventKind::Up => {
                    move_cursor(world, window, pos, &mut applied);
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
                move_cursor(world, window, Vec2::new(x as f32, y as f32), &mut applied);
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
                warn!("input: unknown key code {}", e.code);
                return applied;
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
            applied.focused = Some(e.on);
            if !e.on {
                // As winit does: the game sees every held key released.
                world.write_message(KeyboardFocusLost);
                world.write_message(WindowEvent::KeyboardFocusLost(KeyboardFocusLost));
            }
            write(world, WindowFocused { window, focused: e.on });
        }
        ViewportInput::VisibleEvent(e) => applied.visible = Some(e.on),
        ViewportInput::TouchEvent(e) => touch(world, window, e),
        ViewportInput::GamepadEvent(e) => gamepad(world, e),
    }
    applied
}

/// `touches` is every touch down after the event: one new is `Started`, one that moved `Moved`,
/// and one gone `Ended` (`Canceled` on `cancel`) where it was last.
fn touch(world: &mut World, window: Entity, e: p::TouchEvent) {
    let gone = if e.kind == p::TouchEventKind::Cancel { TouchPhase::Canceled } else { TouchPhase::Ended };
    let mut was = std::mem::take(&mut world.resource_mut::<Remote>().touches);
    let mut now = BTreeMap::new();
    let mut out = vec![];
    for t in &e.touches {
        let position = Vec2::new(t.x as f32, t.y as f32);
        let phase = match was.remove(&t.id) {
            None => Some(TouchPhase::Started),
            Some(before) if before != position => Some(TouchPhase::Moved),
            Some(_) => None,
        };
        if let Some(phase) = phase {
            out.push(TouchInput { phase, position, window, force: t.force.map(ForceTouch::Normalized), id: t.id });
        }
        now.insert(t.id, position);
    }
    for (id, position) in was {
        out.push(TouchInput { phase: gone, position, window, force: None, id });
    }
    world.resource_mut::<Remote>().touches = now;
    for t in out {
        write(world, t);
    }
}

/// A button of the W3C standard mapping.
fn pad_button(i: usize) -> GamepadButton {
    use GamepadButton::*;
    match i {
        0 => South,
        1 => East,
        2 => West,
        3 => North,
        4 => LeftTrigger,
        5 => RightTrigger,
        6 => LeftTrigger2,
        7 => RightTrigger2,
        8 => Select,
        9 => Start,
        10 => LeftThumb,
        11 => RightThumb,
        12 => DPadUp,
        13 => DPadDown,
        14 => DPadLeft,
        15 => DPadRight,
        16 => Mode,
        n => Other(n as u32),
    }
}

/// An axis of the W3C standard mapping, and its sign: the W3C's y is down, Bevy's up.
fn pad_axis(i: usize) -> (GamepadAxis, f32) {
    match i {
        0 => (GamepadAxis::LeftStickX, 1.0),
        1 => (GamepadAxis::LeftStickY, -1.0),
        2 => (GamepadAxis::RightStickX, 1.0),
        3 => (GamepadAxis::RightStickY, -1.0),
        n => (GamepadAxis::Other(n as u32), 1.0),
    }
}

/// A viewer's gamepad becomes the raw events gilrs writes for a native one, so the focus gate,
/// `GamepadSettings` and game code treat both alike. A `state` writes what changed.
fn gamepad(world: &mut World, e: p::GamepadEvent) {
    let entity = match world.resource::<Remote>().pads.get(&e.index) {
        Some(pad) => pad.entity,
        None => {
            let entity = world.spawn_empty().id();
            let pad = Pad { entity, connected: false, buttons: vec![], axes: vec![] };
            world.resource_mut::<Remote>().pads.insert(e.index, pad);
            entity
        }
    };
    let connected = world.resource::<Remote>().pads[&e.index].connected;
    let connection = match e.kind {
        p::GamepadEventKind::Disconnected if connected => Some(GamepadConnection::Disconnected),
        p::GamepadEventKind::Connected | p::GamepadEventKind::State if !connected => {
            let name = e.id.clone().unwrap_or_else(|| format!("gamepad {}", e.index));
            Some(GamepadConnection::Connected { name, vendor_id: None, product_id: None })
        }
        _ => None,
    };
    if let Some(connection) = connection {
        let on = matches!(connection, GamepadConnection::Connected { .. });
        let m = GamepadConnectionEvent::new(entity, connection);
        world.write_message(RawGamepadEvent::Connection(m.clone()));
        world.write_message(m);
        let mut remote = world.resource_mut::<Remote>();
        let pad = remote.pads.get_mut(&e.index).unwrap();
        pad.connected = on;
        pad.buttons.clear();
        pad.axes.clear();
    }
    if e.kind == p::GamepadEventKind::Disconnected {
        return;
    }
    let mut out = vec![];
    let mut remote = world.resource_mut::<Remote>();
    let pad = remote.pads.get_mut(&e.index).unwrap();
    pad.buttons.resize(pad.buttons.len().max(e.buttons.len()), 0.0);
    for (i, b) in e.buttons.iter().enumerate() {
        // A digital button some browsers report pressed at 0.
        let v = if b.pressed && b.value == 0.0 { 1.0 } else { b.value as f32 };
        if pad.buttons[i] != v {
            pad.buttons[i] = v;
            out.push(RawGamepadEvent::Button(RawGamepadButtonChangedEvent::new(entity, pad_button(i), v)));
        }
    }
    pad.axes.resize(pad.axes.len().max(e.axes.len()), 0.0);
    for (i, a) in e.axes.iter().enumerate() {
        let (axis, sign) = pad_axis(i);
        let v = *a as f32 * sign;
        if pad.axes[i] != v {
            pad.axes[i] = v;
            out.push(RawGamepadEvent::Axis(RawGamepadAxisChangedEvent::new(entity, axis, v)));
        }
    }
    for m in out {
        world.write_message(m);
    }
}

#[cfg(test)]
mod surface_shapes {
    use super::ViewportInput;
    use serde_json::json;

    /// The surface protocol's events, shared with abyme's browser host (protocol 1.9): the engine
    /// accepts the fields it may ignore instead of refusing the event as malformed.
    #[test]
    fn accepts_the_surface_protocols_optional_fields() {
        for e in [
            json!({ "type": "pointer", "kind": "move", "x": 10.0, "y": 20.0, "buttons": ["left", "back"],
                    "mods": { "shift": true, "ctrl": false, "alt": false, "meta": false, "caps": true } }),
            json!({ "type": "gesture", "kind": "pinch", "delta": 0.1, "x": 5.0, "y": 6.0, "phase": "moved" }),
            json!({ "type": "wheel", "dx": 0.0, "dy": -3.0, "unit": "pixel", "phase": "started", "momentum": false,
                    "mods": { "caps": false } }),
        ] {
            serde_json::from_value::<ViewportInput>(e.clone()).unwrap_or_else(|err| panic!("{e}: {err}"));
        }
    }
}

#[cfg(test)]
mod stream_input {
    use super::*;
    use bevy::ecs::message::MessageCursor;
    use serde_json::json;

    fn world() -> (World, Entity) {
        let mut world = World::new();
        world.init_resource::<Remote>();
        world.init_resource::<Messages<TouchInput>>();
        world.init_resource::<Messages<WindowEvent>>();
        world.init_resource::<Messages<RawGamepadEvent>>();
        world.init_resource::<Messages<GamepadConnectionEvent>>();
        let window = world.spawn_empty().id();
        (world, window)
    }

    fn send(world: &mut World, window: Entity, e: Value) {
        apply_one(world, window, serde_json::from_value(e.clone()).unwrap_or_else(|err| panic!("{e}: {err}")));
    }

    fn read<M: Message + Clone>(world: &World, cursor: &mut MessageCursor<M>) -> Vec<M> {
        cursor.read(world.resource::<Messages<M>>()).cloned().collect()
    }

    #[test]
    fn parses_touch_gamepad_and_hello() {
        for e in [
            json!({ "type": "touch", "kind": "start", "touches": [{ "id": 3_000_000_000u64, "x": 1.0, "y": 2.0, "rx": 8.0, "ry": 9.0, "force": 0.5 }],
                    "mods": { "shift": false } }),
            json!({ "type": "touch", "kind": "end", "touches": [] }),
            json!({ "type": "gamepad", "kind": "connected", "index": 0, "id": "Xbox Wireless Controller",
                    "buttons": [{ "pressed": true, "value": 1.0 }], "axes": [0.0, -1.0] }),
            json!({ "type": "gamepad", "kind": "disconnected", "index": 0 }),
        ] {
            serde_json::from_value::<ViewportInput>(e.clone()).unwrap_or_else(|err| panic!("{e}: {err}"));
        }
        let hello: p::HelloMessage = serde_json::from_value(json!({ "viewer": { "kind": "stream", "device": "iPhone", "mobile": true } })).unwrap();
        assert_eq!(hello.viewer.unwrap().kind, p::ViewerInfoKind::Stream);
        serde_json::from_value::<p::HelloMessage>(json!({})).unwrap();
        for t in INPUTS {
            serde_json::from_value::<p::InputType>(json!(t)).unwrap();
        }
    }

    #[test]
    fn a_touch_sequence_is_bevy_touch_input() {
        let (mut world, window) = world();
        let mut cursor = world.resource::<Messages<TouchInput>>().get_cursor();
        let t = |id: u64, x: f64, y: f64| json!({ "id": id, "x": x, "y": y, "force": 0.5 });
        // abyme's stream sends `t` (1.20), ms on the viewer's clock; a touch without it is accepted too.
        send(&mut world, window, json!({ "type": "touch", "kind": "start", "touches": [t(1, 10.0, 20.0)], "t": 1234.5 }));
        send(&mut world, window, json!({ "type": "touch", "kind": "start", "touches": [t(1, 10.0, 20.0), t(2, 50.0, 60.0)] }));
        send(&mut world, window, json!({ "type": "touch", "kind": "move", "touches": [t(1, 12.0, 21.0), t(2, 50.0, 60.0)] }));
        send(&mut world, window, json!({ "type": "touch", "kind": "end", "touches": [t(2, 50.0, 60.0)] }));
        send(&mut world, window, json!({ "type": "touch", "kind": "cancel", "touches": [] }));
        let got: Vec<_> = read(&world, &mut cursor).into_iter().map(|t| (t.id, t.phase, t.position, t.window)).collect();
        assert_eq!(got, vec![
            (1, TouchPhase::Started, Vec2::new(10.0, 20.0), window),
            (2, TouchPhase::Started, Vec2::new(50.0, 60.0), window),
            (1, TouchPhase::Moved, Vec2::new(12.0, 21.0), window),
            (1, TouchPhase::Ended, Vec2::new(12.0, 21.0), window),
            (2, TouchPhase::Canceled, Vec2::new(50.0, 60.0), window),
        ]);
        let mut w = world.resource::<Messages<WindowEvent>>().get_cursor();
        assert_eq!(w.read(world.resource::<Messages<WindowEvent>>()).filter(|e| matches!(e, WindowEvent::TouchInput(_))).count(), 5);
    }

    #[test]
    fn a_gamepad_state_is_bevy_raw_gamepad_events_by_the_standard_mapping() {
        let (mut world, window) = world();
        let mut raw = world.resource::<Messages<RawGamepadEvent>>().get_cursor();
        let mut conn = world.resource::<Messages<GamepadConnectionEvent>>().get_cursor();
        let b = |v: f64| json!({ "pressed": v > 0.5, "value": v });
        let mut buttons = vec![b(0.0); 17];
        buttons[0] = b(1.0);
        buttons[7] = b(0.4);
        buttons[12] = json!({ "pressed": true, "value": 0.0 });
        send(&mut world, window, json!({ "type": "gamepad", "kind": "connected", "index": 2, "id": "pad", "buttons": buttons, "axes": [0.5, 0.25, 0.0, -1.0] }));
        let connected = read(&world, &mut conn);
        assert_eq!(connected.len(), 1);
        let pad = connected[0].gamepad;
        assert!(matches!(&connected[0].connection, GamepadConnection::Connected { name, .. } if name == "pad"));
        let got: Vec<_> = read(&world, &mut raw)
            .into_iter()
            .filter_map(|e| match e {
                RawGamepadEvent::Button(b) => Some(format!("{:?} {}", b.button, b.value)),
                RawGamepadEvent::Axis(a) => Some(format!("{:?} {}", a.axis, a.value)),
                RawGamepadEvent::Connection(c) => {
                    assert_eq!(c.gamepad, pad);
                    None
                }
            })
            .collect();
        assert_eq!(got, ["South 1", "RightTrigger2 0.4", "DPadUp 1", "LeftStickX 0.5", "LeftStickY -0.25", "RightStickY 1"]);
        // A state writes only what changed.
        buttons[0] = b(0.0);
        send(&mut world, window, json!({ "type": "gamepad", "kind": "state", "index": 2, "buttons": buttons, "axes": [0.5, 0.25, 0.0, -1.0] }));
        let got: Vec<_> = read(&world, &mut raw).into_iter().map(|e| format!("{e:?}")).collect();
        assert_eq!(got.len(), 1, "{got:?}");
        assert!(got[0].contains("South") && got[0].contains(&format!("{pad:?}")), "{got:?}");
        send(&mut world, window, json!({ "type": "gamepad", "kind": "disconnected", "index": 2 }));
        assert!(matches!(read(&world, &mut conn)[..], [GamepadConnectionEvent { connection: GamepadConnection::Disconnected, gamepad }] if gamepad == pad));
    }
}
