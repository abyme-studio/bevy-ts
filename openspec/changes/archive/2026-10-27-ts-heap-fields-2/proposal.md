## Why

Stage 1 (ts-component-heap-fields) put `String` and `Vec<T>` on TS components only. A Bevy resource,
`Local`, message or event holds them too, a Bevy `Vec` is edited in place, and its items may be
vectors, entities or structs. Games still hand-build string tables and rewrite whole lists to push one
item. Stage 1 also left a bug: a heap field read on an `On<Remove>` value fails (it is routed as a
resource read).

## What Changes

- Heap fields (`string`, `T[]`) in TS resources, `Local`, messages and events, with the same
  declaration, defaults and storage as components: Rust `String` / `Vec<T>` in the value's own bytes,
  through the one `heap.rs` (`HeapShape`, `HeapBytes`, drop and deep clone) and the one codec.
- A heap read names a place, not only an entity: an entity's component, a resource, the running
  system's `Local`, a message, or the running observer's value. Messages and events are read-only
  once written, so their reads are cached for the record's life.
- Lifecycle values: `On<Remove<C>>`, `On<Despawn<C>>` and `On<Discard<C>>` carry an owned copy of a
  heap component's value, so its heap fields read as they were when the event fired.
- In-place list edits on `Mut`, `ResMut` and `Local`: `tag.scores.push(4)`, `tag.scores[2] = 1`,
  `pop`, `length = n`. Edits flush at the end of the run as item edits (set, append, truncate), not
  a whole-list rewrite. A read-only list stays frozen.
- Protocol: `mutate_components` takes `.field[i]` on a list and writes that item in place.
- Richer list items: `Vec2[]`, `Vec3[]`, `Vec4[]`, `Quat[]`, `Entity[]`, string-literal unions, and
  structs of plain fields (`Point[]` with `type Point = { x: f32; y: f32 }`). Entities in lists are
  remapped on load. Nested lists and heap members inside an item stay refused.
- `resourceEquals` compares a heap field by value.
- Plain types (no heap field) are unchanged: same layout, same paths, no host call; the bench shows it
  against main.
- Protocol schema: `list { of }` gains vector, entity, enum and struct item kinds. A new protocol minor.
- **BREAKING** for nothing that loads today, except the lifecycle-value fix (a read that threw now
  returns the value).

## Capabilities

### New Capabilities

None.

### Modified Capabilities

- `ts-components`: list items widen; heap reads and in-place edits on `Mut`; "components only"
  replaced by heap fields everywhere a layout is declared; lifecycle values own a copy.
- `ts-systems`: resources, `Local`, messages and events hold heap fields; `resourceEquals` compares
  them by value.
- `remote-protocol`: `.field[i]` edits of a list; schema item kinds.
- `world-saves`: entities in lists remap on load; list item types migrate by item shape.

## Impact

- `crates/bevy-ts`: `heap.rs` (item kinds, struct items, place-free value ops, list edits),
  `layout.rs`, `value.rs`, `reflect_codec.rs` (heap places, edit op), `memory.rs` (`LocalStore`
  owns its values), `messages.rs` (queues own records), `observers.rs` and `commands.rs` (events and
  lifecycle values own their bytes), `systems.rs`, `generation.rs`, `conditions.rs`, `saves.rs`,
  `plugin_api.rs`.
- `crates/bevy-ts-cli/src/reader.rs` (heap types on every layout, item types), `types.rs`.
- `crates/bevy-ts-v8/js/core.js` (places, list edits, item decoders and encoders, writer views),
  `crates/bevy-ts-v8/src/lib.rs` if a frame gains a slot; `packages/bevy-ts/index.d.ts`.
- `crates/bevy-ts-remote` (`protocol.rs`, `values.rs`), `protocol/bevy-ts.openrpc.json`.
- `bench/ts-heap-fields/` (new arms), `.abyme/processes.ts`.
- Docs: `docs/ecs.md`, `docs/scripts.md`, `docs/reflection.md`, `docs/saves.md`,
  `docs/research/bevy-coverage.md`.
- No new dependency.
