# Rust messages and events from TS

Research for the `rust-events` change: how a TS script reads and writes any Rust `Message` and
observes any Rust `Event`, without a hand-written port per type. Bevy 0.20.0 sources, bevy-ts at
`70880c6`, and the `reflect-coverage` proposal (in the `reflect-enums-color` worktree).
research@rust-events, 2026-10-23.

## Answer

Name every Rust message and event by its reflect type path, carry payloads as JSON through
`reflect_value::read`/`write` (reflect-coverage's one shape), and add two bevy-ts type data,
`ReflectTsMessage` and `ReflectTsEvent`, registered by one generic call per type. Bevy's own
reflection cannot read messages, so a per-type generic registration is unavoidable; it is one
line, not a port. Should become a change: yes.

## What Bevy gives

- `ReflectMessage` (`bevy_ecs/src/reflect/message.rs`): `write_message` only. No read.
- `ReflectEvent` (`bevy_ecs/src/reflect/event.rs`): `trigger`, `create_observer(callback)` (a
  callback gets `&dyn Reflect` and a `DeferredWorld`), `register_event_key`. Requires
  `E::Trigger: Default`, which holds for global, entity and propagating triggers
  (`PropagateEntityTrigger` has `Default`, `event/trigger.rs:244`).
- `bevy_remote` already does all three by type path: `world.trigger_event`, `world.write_message`
  and an observe-watch that spawns `create_observer(..).with_entity(target)`
  (`bevy_remote/src/builtin_methods.rs:1480-1660`). That is the pattern to copy.
- `Messages<M>` derives `Reflect`, but no generic `Messages<M>` is registered and its buffers are
  `pub(crate)`. Reading it by reflection is fragile and still needs per-type registration.
- Who carries the type data today: window messages (`WindowResized`, `CursorMoved`,
  `FileDragAndDrop`…) and most input messages (`KeyboardInput`, mouse, gamepad, touch, gestures)
  have `#[reflect(Message)]`. Picking events (`PointerClick`, `PointerOver`…) and `AssetEvent<A>`
  derive `Reflect` with no `Message`/`Event` type data. Focus and UI-widget events have
  `#[reflect(Event)]`.

## Recommended design

1. Naming and payload (Q1). A Rust message or event is its reflect type path, as reflect-tier
   components already are (`plugin_api::read_reflected`, `TypeRegistry::get_with_type_path`).
   Payloads are JSON in reflect-coverage's shape: `reflect_value::read` out, `write` in. One
   mechanism with reflect-coverage and rust-resources: registry by path, `reflect_value` for the
   shape. We need its stage 1 (the reader), not stage 3.
2. Registration. `bevy_ts_native::BevyTsNativeExt` gains `ts_message::<M>()` and `ts_event::<E>()`
   (beside `ts_layout`, `ts_param`). Each registers bevy-ts type data on `M`/`E`:
   - `ReflectTsMessage { read(world, cursor) -> (Vec<Value>, next_cursor), write(world, json) }`,
     monomorphic over `Messages<M>` and a `usize` cursor.
   - `ReflectTsEvent { observe(target: Option<Entity>) -> Observer, trigger(world, json) }`; its
     observer pushes `(type, json, current target, original target)` into `ObserverQueue`.
   The prebuilt engine registers Bevy's set (input, window, picking, focus, UI widgets, asset
   events for its asset types). A test in the style of reflect-coverage's hole check fails when
   a registration carries `ReflectMessage`/`ReflectEvent` but not ours, so the list cannot rot.
   Unlisted third-party types fall back to Bevy's `ReflectMessage` (write only) and
   `ReflectEvent` (observe, trigger; no original target).
3. Reading (Q2). `MessageReader<KeyboardInput>` keeps a cursor per system identity, as TS readers
   do now (`messages.rs`). Each run, Rust calls `read` from the cursor and hands JS an array of
   JSON values. Bevy's `message_update_system` owns expiry: two updates, the same rule TS queues
   follow. `onMessage(M)` works on a Rust message by the same cursor. Restart resets cursors to
   the current end, so a reload does not replay old input.
4. Writing (Q2). `MessageWriter<RequestRedraw>.write({...})` and `commands.write(...)` emit a new
   op `OP_WRITE_REFLECT { type, json }`, applied with the system's commands (the same flush as
   `OP_INSERT_REFLECT` in reflect-coverage stage 2). A Rust reader later in the frame sees it.
   `commands.trigger(FocusGained.of({...}))` emits `OP_TRIGGER_REFLECT` the same way.
5. Observers (Q3).
   - `app.addObserver(fn)` with `On<PointerClick>` spawns one global Bevy observer per Rust event
     type per generation, replaced on reload as `observers::install` replaces lifecycle ones.
   - `commands.entity(e).observe(fn)` (Bevy's `EntityCommands::observe`) emits `OP_OBSERVE
     { entity, observer id }`; Rust spawns `observe(Some(e))` on `e`, so it dies with `e`. It
     also works for TS `EntityEvent`s, which have no entity scope today.
   - Delivery keeps the deferred rule: the Bevy callback only serializes and queues
     (`observers::Trigger::Rust(type)`); TS runs at the next drain (after a TS system's commands,
     or the next stage start). Bubbling is Bevy's: each hop queues its own entry, `entity` the
     hop, `original` the first target.
   - `On<E>.propagate(false)` on a Rust event drops the remaining queued TS hops of that trigger
     only; Rust observers already ran. Documented, not hidden.
6. Typing (Q4). `bevy-ts types` emits each registered type into `bevy.d.ts` as
   `export declare class KeyboardInput extends Message { static readonly path: "bevy_input::keyboard::KeyboardInput"; declare key_code: KeyCode; … }`
   (or `Event`/`EntityEvent`), fields typed by reflect-coverage's `ts_type`. Names follow Bevy:
   `Message`, `MessageReader`, `MessageWriter`, `On<E>`, `addObserver`, `observe`, `trigger`,
   `write`. No new TS names; a Rust class differs from a TS one only by `static native: true`.

## Owners (Q5)

| Concept | Owner |
|---|---|
| Rust messages read and written, Rust events observed and triggered, `entity(e).observe` (TS and Rust events), `ReflectTs*` type data and their check, window, input and asset messages as messages | rust-events |
| `reflect_value::read`/`write`, the JSON shape, `ts_type` in `types.rs`, `OP_INSERT_REFLECT` | reflect-coverage (we call them; we add message and event emission to `types.rs`) |
| TSX `onPointer*` and focus props (`bevy-ts-tsx/src/picking.rs`) | bevy-ui-coverage; may later sit on `ReflectTsEvent`, not required |
| `Res`/`ResMut` of Rust resources | rust-resources (shares only registry-by-path and `reflect_value`) |
| `Discard`/`Despawn` lifecycle, hooks, a test for TS bubbling | ecs-queries |
| Message update under fixed timestep (`ShouldUpdateMessages`), conditions beyond `onMessage` | ecs-schedules |
| `ButtonInput`-style state params, gamepads, touch | script-input-devices, text-input |
| Asset load state, `Assets<T>` | script-assets |

## Ask-first (Q6)

- Performance: yes. Every Rust message read and every Rust event observed pays one reflect
  serialization to JSON and a JSON parse in JS. `CursorMoved` and `PointerMove` can come dozens
  of times a frame. Only types a TS system or observer names pay; a skipped system pays nothing.
  A bench (1,000 `CursorMoved` reads per frame) should state the cost; a binary fast path for
  plain-data messages is a later change if it is too slow.
- New pattern: mild. Two bevy-ts type data and a registration list are new; they extend
  `BevyTsNativeExt` and copy `bevy_remote`'s by-path pattern. Observers built from a command at
  run time are new for bevy-ts (today every TS observer is declared at load).
- No new dependency, service or infrastructure.

## Open choices

1. `entity(e).observe(fn)` with an `fn` not passed to `addObserver`: build its `TsSystem` lazily on
   the first op, keyed by the function's decl id (`file#name`) and remapped on reload
   (recommended), or require declaring it at load with a new `app.addEntityObserver(fn)`.
2. Generic messages (`AssetEvent<Image>`): TS writes `MessageReader<AssetEvent<Image>>` and the
   reader resolves the full path (recommended), or `bevy.d.ts` emits one class per instance
   (`AssetEventImage`).
3. Retire `plugin_api::write_message` once ports can register their types, or keep it for ports
   that map a Rust message onto a TS-declared one. Recommend keep, document `ts_message` first.

Confidence: high on Bevy's reflect API and the gaps (read in source); medium on the cost (not
measured) and on the lazy-observer reload mapping (designed, not tried).
