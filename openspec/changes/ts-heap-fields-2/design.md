## Context

Stage 1 (`openspec/changes/archive/2026-10-26-ts-component-heap-fields`) stores a TS component's
`string` and `T[]` fields inline as Rust `String` / `Vec<T>`, after an 8-byte header holding a
`&'static HeapShape` (`heap.rs`). A component with one registers a drop fn and a deep clone fn. A TS
system reads a heap field with a host call (`reflect_codec` `READ`, a `Target` with `heap: Some`),
cached by the row's changed tick, and assigns it whole (`WRITE`). JS sends heap values after the
plain bytes of an insert (`encodeWithHeap`), and Rust builds the value (`HeapFill::build`).

What stage 1 left out, with where it bites (file:line on this branch):

- TS resources are components on a resource entity (`generation.rs:335`), so storage, migration,
  saves and holes already work. Only the refusal (`generation.rs:398`) and two details block them: a
  read-only `Res` frame has no ticks, so each read is a host call (`core.js:3536`), and
  `Condition::ResourceEquals` compares raw bytes (`conditions.rs:371`), pointers for a heap field.
- `Local`: `LocalStore` keeps a `Vec<u128>` with no drop glue, derives `Clone`, and `redeclare`
  memcpys kept fields (`memory.rs:11-61`). JS builds a `Local`'s layout itself (`cLayout`,
  `core.js:2307`), with no header and no heap sizes.
- Messages: `MessageQueue` keeps records in `Vec<u8>`s, copies on `push` and frees `older` with no
  per-record drop (`messages.rs:36-65`); `TsPayloads` derives `Clone` and the generation clones it
  for rollback (`generation.rs:455`). `MessageWriter.next()` hands a raw byte view (`core.js:2787`).
- Events: `observers::Pending.bytes` is a `Vec<u8>` given to every observer as one external buffer
  (`systems.rs:1474`); `Pending` derives `Clone` (`observers.rs:26`).
- Lifecycle values: `observers.rs:152` copies a heap component's bytes, pointers included, into
  `Pending.bytes`. JS binds the value with no entity (`core.js:2861`), so a heap read goes to slot 0
  = -1, which Rust reads as "the resource" (`reflect_codec.rs:810`): it always fails. docs/ecs.md
  says it reads live; neither is what Bevy does (Bevy's `On<Remove>` sees the component still there).
- A heap `Target` can only address an entity's component (`reflect_codec.rs:482-494`).
- Lists are frozen and assigned whole (`core.js:3337`); `reflectMutGet` / `reflectFlush`
  (`core.js:3444-3474`), the copy-then-flush path of reflected values, is unused for heap holes.
- List items: number kinds, `bool`, `string` only (`layout.rs:166`, `reader.rs:1008`).

Stage-1 bench (`bench/ts-heap-fields/results/runs.txt`, ns a row, 1,000 rows): column 8, string hit
29 / miss 109 / write 381, 8-item `f32[]` hit 30 / miss 110 / write 397, despawn plain 21, heap 38.

## Goals / Non-Goals

**Goals:**

- `string` and `T[]` in TS resources, `Local`, messages and events, through every path that builds,
  reads, moves, clones or drops them, freed exactly once.
- In-place list edits on writable rows, sent as item edits, and `.field[i]` protocol edits.
- List items of `Vec2`, `Vec3`, `Vec4`, `Quat`, `Entity`, enums and plain structs.
- Lifecycle values readable after the component is gone.
- Plain types: same layout, same code path, same cost as main.

**Non-Goals:**

- Nested lists (`f32[][]`), heap members inside a struct item, struct fields outside a list
  (`pos: { x: f32 }` on a component), maps. Follow-ups, named in the coverage row.
- Heap fields in scene props (they keep their own `string` support) and in Rust messages or events
  (they cross as reflected values already).
- Raw `q.tables` arrays for heap components.
- A JS `Proxy` over lists.

## Decisions

### 1. One owner: `heap.rs`, a value anywhere

Every store of a heap value holds a `HeapBytes` or a buffer of them, built by `value.rs` or
`HeapFill`, dropped through its shape, never memcpy'd twice. `HeapShape::of` already keys by name and
layout, so a resource, a `Local`, a message type and an event type each get a shape the same way a
component does (`Local`'s name is its system identity and index). No second drop or clone path.

- `HeapFill::build` (today used by component inserts) also builds message and event payloads from
  JS's `encodeWithHeap` bytes. `encodeWithLayout` already emits that format for any class with a
  heap field; messages and events set `C.heap` the way components do.
- Containers stop deriving `Clone` where they hold heap values (`LocalStore`, `Memory`,
  `MessageQueue`, `TsPayloads`, `Pending`). Generation rollback moves the old `TsPayloads` aside instead of cloning it; `declare` on the next
  one moves queues across, and a failed reload moves them back, so unread messages survive it.
- Plain layouts keep plain `Vec<u8>` paths: a queue or store whose layout has no heap field never
  touches a shape.

### 2. Resources

Lift the refusal. A `Res<R>` frame for a resource with a heap field carries the resource's changed
tick, as `ResMut` does, so reads cache (`systems.rs:1677`). `resourceEquals(R.of({...}))` encodes the
expected value as `HeapBytes`, compares plain byte ranges as today and heap fields by value
(`heap::read` then `==`).

### 3. `Local`

`LocalStore` owns a `HeapBytes`-backed, 16-aligned buffer with a header when the layout has a heap
field; `Drop` drops through the shape. `redeclare` moves a kept heap field (`heap::take_value` from
the old, `ptr::write` into the new), as component reload does (`components.rs:416`). JS keeps
`cLayout`, which mirrors `ComponentLayout::c_layout` and so puts the header first and a heap field
at 24 bytes, aligned 8. A `Local` has no reflected target: its heap call's place is -2 and the
`Local`'s index in the system, and slot 2 is the field's index in its layout, so no Rust
resolution is needed. The runner sets `RunPlace::locals` around the run (`places.rs`). JS caches
a heap field's read until the accessor writes that field, since the system is its only writer.

### 4. A heap read names a place

`Target` today is (component, field). A heap host call's slots are `f64`s; today slot 0 is the
entity (negative: the resource) and slot 1 the target (`reflect_codec.rs:803-805`). Every heap call
(`READ`, `WRITE`, `WRITE_EDIT`) takes one layout instead: slot 0 the entity's bits or a negative
place kind, slot 1 the place id (0, unused, for an entity and a resource), slot 2 the target, then
the call's own slots (a write's byte length in slot 3). Reflected (non-heap) calls move to the same
layout, so there is one.

| Place | Slot 0 | Slot 1 | Lifetime of a cached read |
| --- | --- | --- | --- |
| entity's component | entity bits (≥ 0) | 0 | row's changed tick |
| resource | -1 | 0 | resource's changed tick |
| running system's `Local` | -2 | the `Local`'s index in the system | until the system writes it (it is the only writer) |
| message | -3 | its sequence number (< 2^53, exact in an `f64`) | the record's life (messages are read-only) |
| the running observer's value | -4 | 0 | the observer's run |

A message's queue is found by the target's owner name (the message type), then the record by its sequence number.

For `Local` and observer places, Rust resolves the place against the run in progress: the system
runner keeps the running system's identity and the observer drain keeps the `Pending` it is running
(`systems.rs:1474`), in a `RunPlace` resource. A host call gets only a shared world, so the runner sets `RunPlace` once per system or observer run, before calling JS, and clears it after. A place that is not
live (a stale sequence, no observer running) fails the call naming the place. The field is still the
`Target`; for non-component places the target is built from the layout's shape (`Target::ts_heap`,
keyed by the layout's owner name), so the codec is one.

Alternative: write every heap field's codec bytes into the frame next to the plain bytes and decode
in JS on bind. No host calls, but every observer and reader pays to decode fields it never reads,
and a 1,000-message read of an unread `string[]` decodes all of them. Rejected; lazy reads match the
component path. Revisit if the bench shows messages read every field.

### 5. Messages and events own their records

- `MessageQueue` for a heap layout keeps a `HeapFill` (its shape and defaults); `push` takes
  `HeapBytes` by value (moves, forgets); `update` drops each record of `older` through the shape
  before clearing; `Drop` drops both halves. Records stay in the byte buffer plain queues use:
  every heap slot is read and written unaligned (`heap.rs`), so it needs no 8-aligned one. Readers
  still get the raw buffers as external arrays for plain fields; heap fields are read by place
  (decision 4): -3 and the record's sequence number, its frame giving each run's first. A
  message's and an event's heap fields are `ReflectTargets::ts_heap` targets marked `Payload`, so
  JS reads them as a component's holes.
- A reload's rollback keeps `TsPayloads: Clone`: a queue's clone deep-copies each record
  (`HeapBytes::copy_of`), so the failed generation's copy and the kept one each own their
  values and each is freed once. Moving queues across and back would save that copy only on a
  reload with unread heap messages.
- `MessageWriter.next()` on a heap message: its view's heap setters keep the value in JS and the
  flush appends it as `encodeWithHeap` does. An unset heap field takes its default, built in Rust.
- `plugin_api::write_message` encodes through `value::encode_component`, so heap fields work there.
- `Pending.bytes` becomes `HeapBytes`. Every observer and hop reads the same value; the drain drops
  it once after the last observer. `On<E>.event` (a JSON copy, `core.js:2845`) reads heap fields
  through the place too.
- A message or event's heap field is read-only to JS: Bevy's `MessageReader` and `On` give `&M`.

### 6. Lifecycle values own a copy

For a heap component, `observers.rs:152` copies with `HeapBytes::copy_of` (deep) instead of raw
bytes, only when an observer for that kind and component exists (the install already filters).
Plain components keep the memcpy. The value reads at the observer place (-4). Bevy runs `On<Remove>`
while the component is still on the entity; ours run at the next stage boundary, so a copy taken at
trigger time is the closest match. docs/ecs.md's "reads live, throws once gone" goes.

### 7. In-place list edits: copy, edit, flush item edits

A list read through a writable accessor (`Mut<C>`, `ResMut<R>`, `Local`) returns a plain, mutable JS
array, made for this run from the cached value, with a snapshot of its items (the `reflectMutGet`
path). At the end of the run, `reflectFlush` diffs each touched list against its snapshot and sends
one `WRITE_EDIT` host call per changed list. With old length `n0` and new length `n1`, items compared
with `Object.is` (so `NaN` equals `NaN` and `-0` differs from `0`; struct and vector items member by
member):

- `truncate(n1)` when `n1 < n0`;
- `set(i, item)` for each `i < min(n0, n1)` whose item differs;
- `append(items[n0..n1])` when `n1 > n0`;
- in that order, applied in place on the `Vec`. When the sets cover more than half of `min(n0, n1)`
  (a `sort`, a `shift`, a `splice`), one `replace(all)` is sent instead: today's whole write.

Rust applies the ops (`heap.rs` `List::apply`), sets the change tick once, and drops the cache entry.
A list that was read but not changed sends nothing and marks nothing changed, as bevy-ts marks a row
only on a real write (docs/ecs.md), like Bevy's `set_if_neq`.

- Holes: an array with a hole or an `undefined` item at flush (`length = n` grown, `a[10] = x` past
  the end) fails the run with a `TypeError` naming the field and the first bad index, and writes
  nothing for that list.
- Assignment: a whole assignment (`tag.scores = [...]`) stays an immediate `WRITE`, and replaces the
  array and its snapshot with the assigned value, so edits before it are dropped and edits after it
  diff against it.
- When the edit is seen: by the same system at once (it holds the array); by Rust at the end of the
  system's or observer's run, before any later system. Same rule as reflected `Mut` fields today. The
  observer runner flushes as `runSystem` does. A host call in the same run that reads that field from
  Rust (another query over the same row) flushes first: `reflectFlush` runs before a heap `READ` of
  a row with a pending edit.
- An array kept past its run is a plain JS array, no longer tied to the row: editing it later writes
  nothing. The next run hands out a fresh one.
- Cost: diffing is O(items) per touched list, once per run, no allocation for an untouched one.

Alternative: a `Proxy` sending each `push` and `[i] =` as it happens. Index access through a proxy
is many times a plain array's on V8 and every op is a host call. Rejected.
Alternative: methods on a list view (`tag.scores.set(i, x)`). Not a JS array, so `for..of`,
spread, `map` break. Rejected.

### 8. `.field[i]` over the protocol

`mutate_components` with `.scores[i]` decodes one item of the list's item type and applies
`set(i, item)` through the same `List::apply`. `i` past the end fails naming the length. Struct items
take `.points[i].x`. Whole-field edits are unchanged.

### 9. Richer list items

`Item` gains `Vec2`, `Vec3`, `Vec4`, `Quat`, `Entity`, `Enum { width }` and `Struct(&'static ItemShape)`.
Storage, as Bevy keeps a `Vec<T>`:

- `Vec<Vec2>`, `Vec<Vec3>`; `Vec4` and `Quat` as `Vec<[f32; 4]>`, since glam's SIMD `Vec4`/`Quat` are
  16-aligned and a column stores them at align 4 (`layout.rs` `align`).
- `Vec<Entity>`; enums as `Vec<u8|u16|u32>` of the stored variant value.
- A struct item is a `FieldType::Struct { fields }` laid out by `c_layout` (plain fields only, no heap
  member; its size is a multiple of its alignment, so the stride is the size). It is stored as a raw
  list `heap.rs` owns: a pointer, `cap` and `len` counted in items, the buffer allocated and freed
  with `Layout::from_size_align(stride * cap, align)`. That is the memory a Rust `Vec<Point>` of a
  `#[repr(C)]` `Point` of the same layout has, so a Rust system reads it as one. Vec's field order
  is not stable Rust, so it is probed once from a real `Vec` (a `Vec<u8>` with distinct `cap` and
  `len`, read as three words) and every raw list writes its words in that order; a unit test asserts
  the probe. A raw list is built, edited, cloned and dropped only through `heap.rs` (`ItemShape`, the
  item layout, leaked per distinct layout as `HeapShape` is), never through a `Vec`'s own push or
  drop.
- A Rust mirror of a `Vec4` or `Quat` item, in a list or in a struct item, uses `[f32; 4]`, not
  glam's `Vec4`/`Quat` (16-aligned).

Codec: items reuse the reflect codec's `Lanes`, `Entity`, `Enum` and `Struct` nodes, so `core.js`'s
decoder already reads them; the encoder gains the same four. JS reads a vector item as a frozen
`ReadonlyVec3`-like value, an entity as `Entity`, a struct as a frozen plain object; on a writable
accessor items are mutable copies flushed by decision 7.

Declaration (`reader.rs` `field_ty`): `T[]` accepts the new kinds and an object type (a literal, an
alias or an interface, via `object()`) whose members are plain field types. `string` and `T[]` are
accepted on every layout (components, resources, `Local`, messages, events); refusals keep file and
line for nested lists and heap members of a struct item.

Saves: `map_entities` recurses into `List` and `Struct` (`saves.rs:617`). Migration keeps a list when
its saved value encodes as the live type (stage 1's rule; a struct item's fields must match by name
and type). Levels: an entity list cannot be set from a level, as an `Entity` field cannot
(`compose.rs:1002`).

### 10. Bench

Extend `bench/ts-heap-fields` (same method: 1,000 rows or records, median of 200 frames less an empty
system). New arms:

- plain controls built against main as well: plain `Res` read, plain `Local` read, plain message
  write + read, plain event trigger + observer read. Must match main within noise.
- heap: `Res` string read (cached), `Local` string read, message with a `string` write + read,
  event with a `string` observed, lifecycle `Remove` value read on a heap component.
- lists: 8-item and 64-item `f32[]` `push` in place against whole assignment of the same list,
  `[i] =` one item, 8-item `Vec3[]` hit and miss.
- stage-1 arms re-run, compared with the stage-1 numbers above.

3 runs per arm, 8 when two arms are within noise. Numbers go into docs/ecs.md.

## Risks / Trade-offs

- [A memcpy of a heap record somewhere missed: double free or leak] → containers lose `Clone`;
  `push` takes `HeapBytes`; every path has a live-count test (stage 1's `HeapShape::live`): message
  push, update, reader, generation rollback, `Local` redeclare and restart, event drain over several
  observers and hops, lifecycle copy, resource restart.
- [Edits flushed at end of run are not seen by a host read mid-run] → a heap `READ` of a row with a
  pending edit flushes first.
- [Diffing large lists every run] → only lists read through a writable accessor that run; an
  untouched one costs a length check.
- [Plain types regress] → plain layouts never take a shape, plain queues keep `Vec<u8>`; the bench's
  control arms run against main.
- [Place ids from a stale frame] → Rust checks the place against the run in progress and fails the
  call naming it.

## Migration Plan

Nothing loading today holds a heap field outside components or a richer list item. Each stage
(tasks.md) lands alone; rollback is reverting the stage. The lifecycle-value fix changes a read that
threw into one that returns the value.

## Open Questions

None for the build; the ask-first items below need the operator's go before stage 1 lands.

## Ask first

- New architectural pattern: heap host calls address a place (table in decision 4), with a
  `RunPlace` the runner sets per run. Alternative: decode heap fields eagerly into the frame (no
  pattern, every reader pays).
- Performance: in-place list edits flush by diffing at the end of the run (O(items) per touched
  list); message and event records with heap fields drop per record. Plain types unchanged, shown by
  control arms against main. Alternative: whole-list writes only, as stage 1.
- Protocol: `list { of }` gains vector, entity, enum and struct items, and `.field[i]` on a list now
  writes: a new protocol minor and a regenerated `protocol/bevy-ts.openrpc.json`.
- Behaviour change: lifecycle values become a copy taken at trigger time.
- No new dependency, no data visibility change, no AI prompt or tool change, nothing in production,
  nothing irreversible.
