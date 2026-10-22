## ADDED Requirements

### Requirement: Rust messages and events are named by their reflect type path
A Rust message or event SHALL be known to TS by its reflect type path, as a reflect-tier
component is (`bevy_input::keyboard::KeyboardInput`). Its TS class SHALL carry that path as
`static path` and `static native = true`. Payloads SHALL cross as JSON in the shape of
`reflect_value::read` (Rust to TS) and `reflect_value::write` (TS to Rust), the one shape TSX and
the protocol use. A generic type SHALL be named by its full path with its arguments
(`bevy_asset::event::AssetEvent<bevy_image::image::Image>`), resolved by the reader from the TS
type argument.

#### Scenario: A generic message by its argument
- **WHEN** a TS system takes `MessageReader<AssetEvent<Image>>`
- **THEN** it reads Bevy's `Messages<AssetEvent<Image>>`, and a `MessageReader<AssetEvent<Mesh>>` in another system reads a different queue

### Requirement: ts_message and ts_event register a Rust type for TS
`BevyTsNativeExt` SHALL offer `app.ts_message::<M>()` for any `M: Message + Reflect` and
`app.ts_event::<E>()` for any `E: Event + Reflect` whose trigger is `Default`. Each SHALL
register the type for reflection and add bevy-ts type data, `ReflectTsMessage` (read from a
cursor, write from JSON) or `ReflectTsEvent` (build an observer for an optional target, trigger
from JSON). The prebuilt engine SHALL register Bevy's window messages, input messages (keyboard,
mouse button, motion, wheel, `CursorMoved`, touch, gestures, `GamepadEvent` and the
`Gamepad*Event` family, `GamepadRumbleRequest`), `RequestRedraw`, picking events, focus events,
UI widget events, `WorldInstanceReady`, and `AssetEvent<A>` and `AssetLoadFailedEvent<A>` for
every asset type it registers.

#### Scenario: A project's own message
- **WHEN** the native crate calls `app.ts_message::<Scored>()` for `#[derive(Message, Reflect)] struct Scored { points: u32 }`
- **THEN** after the project engine rebuilds, a TS system can take `MessageReader<Scored>` and `MessageWriter<Scored>` with no other Rust code

### Requirement: Unregistered reflected types fall back to Bevy's type data
A type path with Bevy's `ReflectMessage` but no `ReflectTsMessage` SHALL be writable from TS and
SHALL fail the load as a `MessageReader` with an error naming the type and `ts_message`. A type
path with Bevy's `ReflectEvent` but no `ReflectTsEvent` SHALL be observable and triggerable;
its observers SHALL get the current target and no original target.

#### Scenario: Reading an unlisted message
- **WHEN** a TS system takes `MessageReader<ThirdPartyPing>` and `ThirdPartyPing` has only `#[reflect(Message)]`
- **THEN** the load fails with an error naming `ThirdPartyPing` and saying to register it with `app.ts_message`

### Requirement: Reading a Rust message
`MessageReader<M>` of a Rust message SHALL offer `each`, iteration and `len` over messages the
system has not read yet, each a plain JSON value, as for a TS message. The cursor SHALL be kept
per system identity. Bevy's `message_update_system` SHALL own expiry, so a message is readable
for two updates. `onMessage(M)` SHALL be true when the system's cursor has unread messages of
`M`. A restart SHALL move every cursor to the queue's end, so old messages are not replayed. A
system that does not run SHALL pay no serialization.

#### Scenario: Keyboard input read once
- **WHEN** Bevy writes two `KeyboardInput` messages in a frame and two TS systems take `MessageReader<KeyboardInput>`
- **THEN** each system reads both messages once, with `key_code` and `state` in reflect-coverage's enum shape, and neither sees them the next frame

### Requirement: Writing a Rust message
`MessageWriter<M>.write(value)` and `commands.write(M.of(value))` of a Rust message SHALL queue
a command that writes the message into Bevy's `Messages<M>` when the system's commands are
applied, in order with its other commands. A Rust reader that runs later in the frame SHALL see
it. A value that does not deserialize SHALL fail that run's commands with an error naming the
type and the field.

#### Scenario: Rumble request
- **WHEN** a TS system writes `GamepadRumbleRequest.of({ Add: { gamepad: pad, duration: 0.5, intensity: { strong_motor: 1, weak_motor: 0.25 } } })` and a Rust system after it reads `MessageReader<GamepadRumbleRequest>`
- **THEN** the Rust system reads one `Add` with a 500 ms `Duration`, that gamepad, and that intensity

### Requirement: Observing and triggering a Rust event
An exported function whose first param is `On<E>` for a Rust event, registered with
`app.addObserver(fn)`, SHALL observe every trigger of `E`. `On<E>` SHALL give the event's fields
as for a TS event, `entity` (the current target, for an entity event), `original` (the first
target of a bubbling event) and `propagate(on)`. The Bevy observer SHALL only serialize and
queue; the TS observer SHALL run at the next drain of the observer queue, as for any event
triggered from Rust. Each bubbling hop SHALL be its own call. `propagate(false)` SHALL drop the
remaining TS hops of that trigger; Rust observers have already run. `commands.trigger(E.of(v))`
SHALL trigger a Rust event when the system's commands are applied. Observers registered by
`addObserver` SHALL be replaced on reload.

#### Scenario: A bubbling click
- **WHEN** a TS observer takes `On<PointerClick>` and the user clicks a child entity whose parent is also pickable
- **THEN** the observer runs twice at the next bevy-ts stage start, first with `entity` the child, then the parent, both with `original` the child

### Requirement: Generated types for Rust messages and events
`bevy-ts types` SHALL emit every type with `ReflectTsMessage` or `ReflectTsEvent` as a class
extending `Message`, `Event` or `EntityEvent`, with `static readonly path`, `static readonly
native: true` and fields typed by reflect-coverage's `ts_type`. A generic type SHALL emit one
generic class (`AssetEvent<A extends Asset>`). Output SHALL stay deterministic.

#### Scenario: KeyboardInput in bevy.d.ts
- **WHEN** `bevy-ts types` runs on the prebuilt engine
- **THEN** `bevy.d.ts` declares `class KeyboardInput extends Message` with `path` `"bevy_input::keyboard::KeyboardInput"` and a typed `key_code`

### Requirement: No Bevy message or event is left unregistered
A test SHALL build the prebuilt engine and fail for any registered type that carries Bevy's
`ReflectMessage` or `ReflectEvent` but not `ReflectTsMessage` or `ReflectTsEvent`, and for any
input, window, picking, focus or asset message or event listed in the test that is not
registered, unless the type is on an allowlist with a reason.

#### Scenario: A new Bevy message
- **WHEN** a Bevy upgrade adds a message with `#[reflect(Message)]` and nobody calls `ts_message` for it
- **THEN** the coverage test fails naming the type

### Requirement: The cost of a Rust message read is measured
A bench SHALL read 1,000 `CursorMoved` messages a frame through one TS system and report the
per-message and per-frame cost. Its result SHALL be recorded in the docs before a shipped
example relies on reading cursor or pointer messages.

#### Scenario: Running the bench
- **WHEN** the bench runs in release
- **THEN** it prints the per-message cost of serialization and parse, and the frame cost of 1,000 reads
