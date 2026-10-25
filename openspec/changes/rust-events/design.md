## Context

TS messages live in bevy-ts's own double-buffered queues (`messages.rs`), TS events in
`ObserverQueue` (`observers.rs`). Rust messages live in Bevy's `Messages<M>`, whose buffers are
`pub(crate)`; Bevy's `ReflectMessage` can only write, and `ReflectEvent` can trigger and build an
observer (`create_observer`). `bevy_remote` already writes messages and observes events by type
path (`builtin_methods.rs`). Ports forward Rust messages by hand through
`plugin_api::write_message` (Rapier's `CollisionEvent`). Research: `docs/research/rust-events.md`.

reflect-coverage stage 1 owns `reflect_value::read`/`write` (the JSON shape) and `ts_type` in
`types.rs`. This change calls them and builds on nothing else of that change.

## Goals / Non-Goals

**Goals:** read and write any registered Rust message, observe and trigger any registered Rust
event, `entity(e).observe` for every entity event, all by type path, with one registration line
per type and a check that keeps Bevy's set complete.

**Non-Goals:** input state params (`ButtonInput`, `Touches`, `Gamepad`, cursor position), input
conditions and rumble helpers (script-input-devices); TSX `onPointer*` and focus props
(bevy-ui-coverage); `Res` of Rust resources (rust-resources); `Discard`/`Despawn` lifecycle and
hooks (ecs-queries); message update under fixed timestep (ecs-schedules); asset load state
(script-assets); a binary fast path for plain-data messages (a later change if the bench asks).

## Decisions

1. Name by reflect type path, payload by `reflect_value`. One mechanism with reflect-tier
   components and rust-resources. Alternative, a layout per message: most messages hold enums
   and entities, so it would cover few types and need a second shape.
2. Our own type data, `ReflectTsMessage` and `ReflectTsEvent`. Bevy has no reflected read of
   `Messages<M>`, so a monomorphic function per type is needed. `app.ts_message::<M>()` builds
   `read(world, cursor) -> (Vec<Value>, next)` over `Messages<M>` with a `MessageCursor`-style
   `usize` and `write(world, &Value)`; `app.ts_event::<E>()` (global) and
   `app.ts_entity_event::<E>()` (entity, as Bevy splits them) build `observe(target:
   Option<Entity>, decl) -> Observer` and `trigger(world, &Value)`. The type data hands over
   reflected values (`&dyn PartialReflect`); the caller turns them into JSON with
   `reflect_value`, so the type data stays one function per type and knows no JSON. The observer pushes
   `Pending { trigger: Trigger::Rust(type_id), json, path: [current], original }` into
   `ObserverQueue`. Alternative, reflect `Messages<M>`'s fields: private, fragile, still per type.
3. Fallback for unlisted types: Bevy's `ReflectMessage::write_message` and
   `ReflectEvent::{trigger, create_observer}`. Reading needs ours; the load error says so.
4. Readers. A Rust `MessageReader` is `RunParam::RustMessageReader { type, cursor }`, cursor kept
   per system identity in `TsSystems` beside the TS cursors. Each run Rust calls `read` and
   hands JS a count then each message encoded along its type's codec (`reflect_codec`, compiled
   once per type; the resolution's `rustMessages` carries it to JS); JS decodes the run once on
   first use. JSON ran 1.1-1.7 ms for 1,000 `CursorMoved`, over the 1 ms budget, so lead@bevy-ts
   chose the codec (rust-events/go-bench). Without `reflect` the run is one JSON array. A system
   skipped by its condition makes no call. Restart sets each cursor to `Messages<M>::len` end.
5. Writers and triggers. Three ops, applied in order with the buffer's other ops:
   `OP_WRITE_REFLECT { type, json }`, `OP_TRIGGER_REFLECT { type, json }`,
   `OP_OBSERVE { entity, decl, event }`. JS picks the reflect op when the class has
   `static native`, the existing op otherwise, so `write`/`trigger` keep one TS name.
6. Global observers. One Bevy observer per Rust event type a generation's observers name,
   spawned at install and despawned on reload, as `observers::install` does for lifecycle ones.
7. `entity(e).observe`. On `OP_OBSERVE`, Rust looks up or builds the `TsSystem` for `decl`
   (`file#name`) in a table of lazy observers, then spawns an observer entity watching `e`
   (Rust event: `ReflectTsEvent::observe(Some(e))`; TS event: a filter on `Pending.path`'s
   current hop). The observer entity is Bevy's, so it dies with `e`. On reload the table maps
   each `decl` to the new generation's function; a `decl` that no longer exists drops its
   observers with a warning. Alternative, `app.addEntityObserver(fn)` at load: an extra name
   Bevy does not have.
8. Generics. `AssetEvent<Image>` is one generic TS class; the reader resolves the full path
   from the type argument in the manifest. Alternative, a class per instance
   (`AssetEventImage`): not Bevy's name.
9. `plugin_api::write_message` stays. `CollisionEvent` has no `Reflect`, and a port may map any
   non-reflected type onto a TS message. It is not superseded; docs list `ts_message` first.
10. Registration list. The prebuilt engine's list sits in one function,
    `bevy_ts_cli::rust_events::register_bevy(app)`, in the crate whose Bevy has the input,
    window, picking and focus features (`bevy-ts` itself builds with a minimal Bevy). `rust_events_coverage.rs` walks the registry:
    any `ReflectMessage`/`ReflectEvent` without ours fails, plus a named list of input, window,
    picking, focus and asset types that must be present. Allowlist with reasons:
    `StateTransitionEvent<S>`, `AssetLoadFailedEvent<A>` and `bevy_scene::Ready` (no `Reflect`
    in 0.20), and any type whose trigger is not `Default`. The check also requires
    `AssetEvent<A>` for every type with `ReflectAsset`.

## Risks / Trade-offs

- Serialization per message → bench first (tasks 2); only named types pay; fallback is a later
  binary path, and cursor position from script-input-devices meanwhile.
- `propagate(false)` cannot stop Rust observers that already ran → documented in the spec and
  the guide.
- Lazy observers across reload are designed, not tried → tests for reload with a live entity
  observer and for a removed declaration.
- Depends on reflect-coverage stage 1 → task 1 is a gate; nothing builds before it lands.
- Bevy upgrades add messages → the coverage test fails until they are registered or allowlisted.

## Migration Plan

No breaking change. Rapier keeps `write_message`. Scripts that read port-forwarded TS messages
keep working. Rollback is reverting the change; no data or wire format persists.

## Open Questions

None blocking. The bench result decides whether the binary path change is opened.
