## Why

A TS script cannot read a Rust message (`KeyboardInput`, `WindowResized`, `AssetEvent<Image>`),
write one (`GamepadRumbleRequest`, `RequestRedraw`), or observe a Rust event (`PointerClick`,
`FocusGained`) from code. Each port forwards by hand today (`plugin_api::write_message`), and
`entity(e).observe` does not exist. Bevy's reflection already names these types; one generic
registration per type closes the gap for good.

## What Changes

- `MessageReader<M>` and `MessageWriter<M>` take any registered Rust message, named by its
  reflect type path, payloads as JSON in reflect-coverage's one shape. `commands.write(...)` and
  `onMessage(M)` work on them too. Bevy's `Messages<M>` stays the store; Bevy's update owns
  expiry.
- `app.addObserver(fn)` with `On<E>` takes any registered Rust `Event` or `EntityEvent`
  (picking, focus, UI widgets, `WorldInstanceReady`), with Bevy's bubbling. `commands.trigger`
  triggers one.
- `commands.entity(e).observe(fn)`, Bevy's `EntityCommands::observe`, for Rust and TS entity
  events. The observer dies with `e`.
- Two bevy-ts type data, `ReflectTsMessage` and `ReflectTsEvent`, registered by
  `app.ts_message::<M>()` and `app.ts_event::<E>()` on `BevyTsNativeExt`. The prebuilt engine
  registers Bevy's set: window, input (keyboard, mouse, cursor, touch, gestures, gamepad events,
  `GamepadRumbleRequest`), picking, focus, UI widgets, and `AssetEvent<A>` and
  `AssetLoadFailedEvent<A>` for its asset types. A hole check fails CI when a registered type
  carries Bevy's `ReflectMessage`/`ReflectEvent` but not ours.
- `bevy-ts types` emits each registered message and event into `bevy.d.ts`/`native.d.ts` as a
  class extending `Message`, `Event` or `EntityEvent`. Generic types stay generic:
  `MessageReader<AssetEvent<Image>>`.
- New command ops: write a reflected message, trigger a reflected event, observe on an entity.
- `plugin_api::write_message` is kept and specified: the seam for a port that maps a Rust type
  without `Reflect` (Rapier's `CollisionEvent`) onto a TS-declared message. `ts_message` is the
  documented first choice.

## Coverage

| Audit row | After this change | Test |
|---|---|---|
| Reading Rust messages (`KeyboardInput`, `AssetEvent`, `WindowResized`) | idiomatic | `rust_events.rs` `a_ts_reader_sees_keyboard_input_once` |
| Writing Rust messages (`GamepadRumbleRequest`, `RequestRedraw`) | idiomatic | `rust_events.rs` `a_ts_writer_sends_a_rumble_request_a_rust_reader_sees` |
| Window messages (`WindowResized`, `CursorMoved`, `FileDragAndDrop`) | idiomatic | `rust_events.rs` `window_messages_reach_a_ts_reader` |
| Input messages (mouse, cursor, touch, gestures, gamepad events) | idiomatic (state params stay with script-input-devices) | `rust_events.rs` `input_messages_reach_a_ts_reader` |
| `AssetEvent`, `AssetLoadFailedEvent` | idiomatic | `rust_events.rs` `asset_event_image_reaches_a_ts_reader` |
| `entity(e).observe` | idiomatic, Rust and TS events | `rust_events.rs` `an_entity_observer_runs_for_its_entity_only_and_dies_with_it` |
| Observers on any Rust event, from code | idiomatic, with bubbling | `rust_events.rs` `a_global_observer_sees_a_bubbling_pointer_click` |
| Every Bevy message and event registered | CI hole check | `rust_events_coverage.rs` |
| Unregistered third-party messages and events | write and observe through Bevy's own type data | `rust_events.rs` `an_unlisted_reflect_message_is_writable` |

`StateTransitionEvent<S>` has no `Reflect` in Bevy 0.20 and stays out; the check lists it with
that reason.

## Capabilities

### New Capabilities

- `rust-events`: Rust messages read and written and Rust events observed and triggered from TS,
  by type path; their registration, type data, generated types and hole check.

### Modified Capabilities

- `ts-systems`: `Observers` gains `commands.entity(e).observe(fn)` for every entity event.
- `plugin-api`: `write_message` is specified as the seam for non-reflected port messages.

## Impact

- Code: `crates/bevy-ts/src/rust_events.rs` (new: type data, cursors, observers),
  `messages.rs`, `observers.rs`, `commands.rs` (three ops), `systems.rs` and `manifest.rs`
  (reader, writer and `On<E>` of a Rust type), `crates/bevy-ts-native/src/lib.rs`
  (`ts_message`, `ts_event`), `crates/bevy-ts-cli/src/types.rs` (message and event emission),
  the prebuilt engine's registration list, `crates/bevy-ts-v8/js/core.js` and
  `packages/bevy-ts/runtime/core.js`, `packages/bevy-ts/index.d.ts`, `bevy.d.ts` regenerated.
- Tests: new `crates/bevy-ts-cli/tests/rust_events.rs`, `rust_events_coverage.rs`, and a bench
  `rust_events_bench.rs`.
- Docs: `docs/native.md` (registration, `write_message` second), a new section in the scripting
  guide on Rust messages and events.
- Depends on reflect-coverage stage 1 (`reflect_value::read`, `ts_type`). Nothing here builds
  until it lands.
- Protocol: no wire change expected. If one comes, it takes the next free minor at landing.
- No new dependency, service or infrastructure.

## Ask-first

Performance needs the operator's go. Every Rust message a TS system reads and every Rust event
a TS observer sees costs one reflect-to-JSON serialization in Rust and one `JSON.parse` in JS.
`CursorMoved` and `PointerMove` can arrive dozens of times a frame. Only types a live TS
system or observer names pay; a skipped system pays nothing; nothing else in the frame changes.
Expected cost: a few microseconds per message, so about 0.1 to 0.3 ms a frame at 50 cursor
messages, measured by the bench (1,000 `CursorMoved` reads a frame) before anything relies on
it. Fallback if the bench is too slow: a binary fast path for plain-data messages through the
existing layout encoder, as a later change; until then scripts read `cursor position` state
from script-input-devices instead of raw cursor messages.

Also new, mild: two bevy-ts type data and a registration list, and observers built from a
command at run time (today every TS observer is declared at load). Both follow `bevy_remote`'s
by-path pattern and extend `BevyTsNativeExt`.
