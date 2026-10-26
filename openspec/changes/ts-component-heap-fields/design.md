## Context

A TS component is a dynamic component: `ComponentDescriptor::new_with_layout`, no `TypeId`, no
drop fn, copy clone (`components.rs:152`). Every path that touches it (insert, reload, clone, saves,
protocol) copies bytes and leans on "plain bytes". Research, with file and line for each path:
`openspec/research/ts-component-heap-fields.md`.

The lead set five decisions; this design keeps all five:

1. The value is inline in the component bytes (`String`, `Vec<T>`), with a drop fn and a deep
   clone fn per component, as Bevy. Not a handle store.
2. Stage one assigns whole fields only. No in-place `Mut` list edits.
3. Components only. `Local`, TS resources, messages, events and scene props are follow-ups.
4. "Small arrays" are variable `T[]` (`Vec<T>`), not the fixed `FieldType::Array`.
5. List items: number kinds (`f32`, `f64`, `i8`..`u32`), `bool`, `string`. Vectors, `Entity` and
   nested structs later.

Stages: strings first (stage 1), then lists (stage 2). Each stage lands on its own.

## Goals / Non-Goals

**Goals:**

- `name: string` and `tags: string[]` on a TS component, through every path that builds, reads,
  moves or drops one.
- A component with no heap field costs what it costs today, bit for bit the same layout.
- No double free and no leak: every byte path either moves ownership or deep-copies.

**Non-Goals:**

- In-place list edits (`c.tags.push(x)`), a later stage with `reflectMutGet`'s copy-and-flush.
- Heap fields in `Local`, TS resources, messages, events.
- List items that are vectors, `Entity`, nested lists or structs.
- Raw `q.tables` access to heap fields.
- `.field[i]` edits of a list over the protocol (whole-field edits only).

## Decisions

### 1. Inline values, found through a header word

`new_with_layout`'s drop is `unsafe fn(OwningPtr)`: a bare fn pointer, no component id, no closure
(bevy_ecs 0.20 `component/info.rs:296`). One shared drop fn cannot know which offsets hold a
`String`. So a component with a heap field starts with a header: 8 bytes holding a
`&'static HeapShape` (each heap field's offset and kind). The drop fn reads the header and drops
each field in place. The clone fn reads the same header and deep-copies. The shape is leaked once
per registered layout; registrations are bounded (a spare id is reused, `components.rs:236`).

- The header belongs to `ComponentLayout`: Rust puts it at offset 0 and hands the other offsets to
  JS as today. JS never sees or writes it. `size()` and `align()` include it; a heap field's own
  storage is 24 bytes, align 8 (`String`, `Vec<T>`; checked by a `const` assert against
  `size_of`).
- A component with no heap field has no header, no drop fn and `clone_bytes`, as today.
- Undeclared and retired components keep working: the drop reads the header, not the registry.

Alternative: a `u32` handle into a per-world store. Its drop fn has the same problem (which offsets
are handles?), so it needs the header or component hooks anyway, and adds a lookup per read. Hooks
(`on_remove` gets the component id) were the other way to find the shape, but hooks do not run when
the world drops, so values would leak. The header is exact and costs 8 bytes a row on heap
components only. Decision 1 stands.

### 2. One owning type for heap bytes

`heap.rs` owns the concept. `HeapBytes` is a component's bytes that own their heap values: built
only by `value.rs`, dropped through the shape, never `Clone`. `insert_bytes` gains a heap form that
takes `HeapBytes` by value and forgets it after the copy, so ownership moves into the column once.
Plain bytes keep today's `&[u8]` path.

- `value::encode_component` returns `HeapBytes` (plain components: a `HeapBytes` with no shape,
  same bytes). `decode_component` reads heap fields through the header. Every caller in the
  research's table (commands, TSX, levels, plugin API, saves, protocol) goes through these two, so
  they change in one place.
- JS-built bytes (`encodeWithLayout`) can never carry a `String`. For a heap component, Rust takes
  JS's bytes for the plain fields and writes every heap slot itself (`ptr::write`), from the insert
  command's heap patch or the default. Whatever JS put at a heap offset is ignored, never read.
- A default with a non-empty string is built fresh per insert. A shared default buffer is never
  memcpy'd twice (today's `defaults.clone()` in `commit` becomes an encode per entity for heap
  components).

### 3. Inserts carry heap values after the bytes

Spawn and insert from JS: the column bytes as today, then, in the same component entry (its `len`
covers both), the heap fields' values: a `u16` count, then each field's index and its value in the
codec's bytes (decision 4). Rust builds the value whole (`heap::HeapFill`): JS's plain fields, the
header, and every heap slot from the entry or the default. One command: a bad value fails the
decode naming the component and field, and nothing applies.

Built: first planned as a new `OP_INSERT_REFLECT` mode (`TS_HEAP_PATCH`), as reflect-coverage
stage 2 split bytes and holes. Inline values need no new op, and reach every op that carries
component entries (spawn, insert, the batches, `insertIfNew`, `tryInsert`, `insertRecursive`)
with no pairing of ops to check.

### 4. Reads and writes in TS systems: the hole path

A heap field is a hole on the TS component's accessor, as a layout-tier hole is
(`core.js:3284` `addHoles`):

- Read: a host call `READ(entity, target)` on a cache miss, cached by the row's changed tick
  (`reflectGet`, `core.js:3145`). The term's frame carries changed ticks for heap components
  (`Access.holes`, `systems.rs:881`, extended to TS components with a shape).
- `Target` (`reflect_codec.rs:478`) gains a kind: reflected (today's fields) or TS heap (component
  id, field offset, `FieldType`). The codec is `Node::Str` or `Node::List { of }` built from the
  `FieldType`, not from `TypeInfo`. The JS decoder already decodes both. No second codec.
- Write: a host call `WRITE(entity, target, bytes)`, applied at once, change tick set, cache entry
  dropped (as `reflect_codec::write`, `:616`). Unlike reflected writes, the value crosses as the
  codec's bytes, not JSON: both ends are ours and no reflection is involved. `core.js` gains the
  encoder for `Str` and `List` of scalars beside `reflectDecoder`.
- A list reads as a frozen array, typed `readonly T[]`; a push throws in strict mode instead of being
  lost. `Mut<C>` assigns a whole `readonly T[]`.
- `q.tables` offers no raw arrays for a component with a heap field (the rule for native types with
  holes, `layout.rs:279`). Plain fields of the same component stay columns.

Alternative: JSON writes through `reflect_value`, as reflected writes do. Rejected: a JSON round
trip for a string costs more (about 3 µs a write today) and needs reflection we do not have.

### 5. Reload moves kept heap fields

`commit` copies kept fields into new bytes, then removes the old component (`components.rs:306`).
For a kept heap field: copy its 24 bytes into the new value, then overwrite the old slot with an
empty value (`ptr::write(String::new())`) before `remove_by_id`, so the old drop frees nothing it
gave away. A reset or removed heap field is freed by the old drop. Added fields take a fresh default.

### 6. Saves and protocol through `value.rs`

- Saves: `read_ts` and load call `decode_component`/`encode_component`, so heap fields save as JSON
  strings and arrays with no new code. Migration: a heap field is kept when its saved value encodes
  as the live type (any length); `same_shape`'s length check does not apply to lists.
- Protocol: `get_components` decodes through `value.rs`. `mutate_components` takes `.field` with a
  whole string or array for a heap field; `.field[i]` on a list is refused with an error naming the
  whole-field form. Schemas: `FieldType` gains `string` and `list { of }` in `protocol.rs`; the
  layout in `x-bevy-ts` shows the header's size in `size`. A new protocol minor version.

### 7. Declaration

`reader.rs` `field_ty` gains a `heap` flag, set for component fields only: `string` becomes
`{ kind: "string" }`, `T[]` becomes `{ kind: "list", of }` (stage 2). For other declarations the
error names this change's follow-up: "string fields are allowed on components only". `FieldType`
gains `Str` and `List { of }`; `validate` refuses `List` of anything but a number kind, `bool` or
`Str`, and `Array { of: Str }`. A backstop in Rust (`FieldType::is_heap`) refuses heap fields in
messages, events, `Local` and TS resources at load, for decls that do not come from the reader.

### 8. Bench

`bench/ts-heap-fields/`, a crate like `bench/rust-events/` (path deps, `run.sh [runs]`, release, runs
appended with `uptime` to `results/runs.txt`), a process `ts-heap-fields-bench` in
`.abyme/processes.ts` through `gate-slot`. 1,000 rows, median of 200 frames less an empty system,
per docs/reflection.md's method. Arms: a plain component's column read (must match main's 10 ns a
row), a heap component's plain field (column), a string hit, a string miss, a string write, an
8-item `f32[]` hit, miss and write (stage 2), and despawn of 1,000 heap rows against plain rows.
3 runs per arm, 8 when two arms are within noise. Numbers go into docs/ecs.md.

## Risks / Trade-offs

- [A byte path missed: a memcpy of `HeapBytes` used twice is a double free] → `HeapBytes` is not
  `Clone`, `insert_bytes` takes it by value; each `HeapShape` counts the values it has built and
  dropped (one atomic per shape, so parallel tests do not share it); a test per path (insert, clone,
  reload, save, load, protocol edit, despawn, world drop) asserts the count returns to zero.
- [JS writes a heap offset through a typed view] → accessors never write heap offsets; no raw
  tables for heap components. Scripts are the game's own code (same trust as native holes today).
- [Header changes layout offsets] → only for components that gain a heap field, and those failed to
  load before. Rust hands JS the offsets; JS computes nothing.
- [Leaked shapes] → one small allocation per distinct registered layout; reuse of spares bounds it.

## Migration Plan

Nothing loads today with a heap field, so no data migrates. Each stage lands alone: stage 1
(`string`) is usable without stage 2. Rollback is reverting the stage.

## Open Questions

None for the build. The ask-first items below need the operator's go before stage 1 lands.

## Ask first

- Performance. A heap field read in a TS system is a host call: about 45 ns a row on a cache hit and
  176 ns on a miss, against 10 ns for a column (docs/reflection.md bench). A write is a host call,
  bytes not JSON, expected well under today's 3 µs reflected write; the bench measures it. A heap
  component's despawn runs a drop fn. Components with no heap field are unchanged, and the bench
  shows that. Alternative: no heap fields; games keep a string table in a resource or a
  `Map<Entity, string>` in a module, at no engine cost but no saves, protocol or Bevy parity.
- Protocol schema bump. `FieldType` gains `string` and `list`: a new protocol minor version and a
  regenerated `protocol/bevy-ts.openrpc.json`. Readers that switch on `kind` must handle two new
  kinds. Alternative: report heap fields as `json` in the schema, with no bump, at the cost of
  editors that cannot type them.
- No new dependency, no data visibility change, no AI prompt or tool change.
