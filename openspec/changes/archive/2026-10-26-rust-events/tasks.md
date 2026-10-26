## 1. Gate

- [x] 1.1 Confirm reflect-coverage stage 1 has landed on main (`reflect_value::read`, `write`, `ts_type`); rebase onto it. Do not start 2 before.
- [x] 1.2 Confirm the operator's go on the per-message serialization cost (proposal, Ask-first). The operator closed rust-events/go with 'go'; lead@bevy-ts's conditions: stop if 1,000 `CursorMoved` reads cost more than 1 ms a frame, bench on a quiet machine, docs say the cost is per message read.

## 2. Type data and the bench

- [x] 2.1 `crates/bevy-ts/src/rust_events.rs`: `ReflectTsMessage` (`read` from a cursor, `write` from JSON) and `ReflectTsEvent` (`observe(target)`, `trigger`), with unit tests on a test message and event
- [x] 2.2 `BevyTsNativeExt::ts_message::<M>()` and `ts_event::<E>()` in `crates/bevy-ts-native/src/lib.rs`
- [x] 2.3 Rust `MessageReader` run param: per-system cursor, one run per call (codec-encoded with `reflect`, else one JSON array), restart moves cursors to the end; `onMessage` on a Rust message
- [x] 2.4 Bench `bench/rust-events` (`rust-events-bench` process): 1,000 `CursorMoved` reads a frame; record the numbers in `docs/native.md`. Stop and report to the lead if the frame cost passes 1 ms. JSON ran 1.1-1.7 ms even quiet; lead@bevy-ts routed reads through `reflect_codec` (rust-events/go-bench): 0.05-0.17 ms over 8 runs at load 91-94

## 3. Writing and triggering

- [x] 3.1 Ops `OP_WRITE_REFLECT` and `OP_TRIGGER_REFLECT` in `commands.rs`; JS picks them for `static native` classes in both `core.js` copies
- [x] 3.2 Fallback to Bevy's `ReflectMessage`/`ReflectEvent` for unlisted types; load error for reading an unlisted message

## 4. Observers

- [x] 4.1 `Trigger::Rust` in `observers.rs` with `original`; global observers per Rust event type, replaced on reload
- [x] 4.2 `propagate(false)` drops the remaining TS hops of one Rust trigger
- [x] 4.3 `commands.entity(e).observe(fn)`: `OP_OBSERVE`, lazy `TsSystem` keyed by `file#name`, remapped on reload, for TS and Rust entity events
- [x] 4.4 `index.d.ts`: `EntityCommands.observe`, `On<E>.original`
- [x] 4.5 Stage 2: a Rust event field named `entity`, `original`, `propagate` or `toJSON` stays on `On<E>.event` under its reflected name and never hides `On`'s API; generated class documents it; test with a `PointerClick` hop and a test event with a `propagate` field
- [x] 4.5a Stage 1: `original` joins accessor-slot-names' `ON_MEMBERS` in `crates/bevy-ts-cli/src/reader.rs`, tested in `surface.rs`
- [x] 4.5b Stage 2: `event` joins `ON_MEMBERS` with `on.event`, with a test
- [x] 4.7 Stage 2: type `EntityCommands.observe` so tsc refuses an observer whose first param is not an entity event (today `On<never>` lets any function through)
- [x] 4.8 Stage 2: a failed reload keeps the attachments `observe` made since the last drain (today `NewTargeted` is dropped before the load), with a test
- [x] 4.6 Stage 2: `On<E>.event` for TS-declared events (whole value, `entity` included for an entity event) in both `core.js` copies and `index.d.ts`; test it reads as the direct fields do

## 5. Registration and types

- [x] 5.1 `register_bevy(app)` (in `crates/bevy-ts-cli/src/rust_events.rs`): window, input (incl. gamepad events and `GamepadRumbleRequest`), `RequestRedraw`, picking, focus, UI widget events, `WorldInstanceReady`, `AssetEvent<A>`/`AssetLoadFailedEvent<A>` per engine asset type
- [x] 5.2 `types.rs` emits message and event classes (generic classes for generic types); regenerate `bevy.d.ts`; determinism test still passes
- [x] 5.3 `crates/bevy-ts-cli/tests/rust_events_coverage.rs`: the hole check with its allowlist (`StateTransitionEvent<S>`)

## 6. Tests, one per coverage row

- [x] 6.1 `a_ts_reader_sees_keyboard_input_once` (two readers, expiry)
- [x] 6.2 `a_ts_writer_sends_a_rumble_request_a_rust_reader_sees` (`GamepadRumbleRequest::Add` with `Duration` and nested intensity, end to end)
- [x] 6.3 `window_messages_reach_a_ts_reader` (`WindowResized`, `CursorMoved`, `FileDragAndDrop`)
- [x] 6.4 `input_messages_reach_a_ts_reader` (mouse button, motion, wheel, touch, a gesture, `GamepadEvent`)
- [x] 6.5 `asset_event_image_reaches_a_ts_reader`; `AssetLoadFailedEvent` has no `Reflect` in Bevy 0.20 and stays out (coverage audit)
- [x] 6.6 `a_global_observer_sees_a_bubbling_pointer_click` (`entity` and `original` per hop, `propagate(false)`)
- [x] 6.7 `an_entity_observer_runs_for_its_entity_only_and_dies_with_it`, plus a TS `EntityEvent` and reload cases
- [x] 6.8 `an_unlisted_reflect_message_is_writable` and the unlisted-reader load error
- [x] 6.9 A project `ts_message::<Scored>()` read and written from TS
- [x] 6.10 Rapier's `write_message` forwarding still passes (`plugin_api.rs` is unchanged; the crate is outside the workspace, and `exported.rs` forwards through `plugin_api::write_message` in the gate)

## 7. Docs and land

- [x] 7.1 `docs/native.md`: `ts_message`/`ts_event` first, `write_message` for non-reflected types; scripting guide section on Rust messages and events
- [x] 7.2 Update the coverage audit rows to idiomatic with their tests
- [x] 7.3 If any wire change appeared, bump the protocol to the next free minor (none: ops 31 and 32, after main's 14-30, are the command buffer's, not the protocol's; it stays at main's version)
- [x] 7.4 `openspec validate rust-events --strict`; full gate green; land with the code. Gate at 89207906: build and clippy green; tests green but 2 GPU materials::drawn tests lost to load, which pass alone (rust-events-gpu)
