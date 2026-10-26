# Research: heap fields on TS components

Change `ts-component-heap-fields`: a TS component (`class X extends Component`) carries a `string`,
then a small list, as a Bevy component carries `String` and `Vec`. Today every TS field is plain
bytes at a fixed offset. Read-only research on main at `d1c86e31`.

## 1. The codec: what is reusable

A TS component has no Rust type: it is a dynamic component (`components.rs:152`,
`ComponentDescriptor::new_with_layout`, no `TypeId`, nothing in the `TypeRegistry`). Everything in
`reflect_codec.rs` and `reflect_value.rs` walks `dyn PartialReflect` found through
`ReflectComponent`, so none of it reaches a TS component as it stands.

What carries over as-is:

- The wire format and its ops. `Node::Str` (`reflect_codec.rs:58`, a `u32` byte length then
  UTF-8, `put_str` at `:287`) and `Node::List { of }` (`:70`, a `u32` count then items, `:427`).
  The JS decoder is generated from the same op tree (`crates/bevy-ts-v8/js/core.js` `reflectDecoder`, `:2961`; every `core.js` line below is that file) and
  already decodes both. A heap field's codec is a `Node` built from its `FieldType`, not from
  `TypeInfo`: `Str`, or `List { of: <scalar op> }` (`Num`, `Lanes`, `Entity` all exist). No
  second codec.
- The read path's shape: `Target`, `ReflectTargets` (`:478`, `:493`), the host calls `READ`/`WRITE` (`:695`), the per-target tick cache in JS (`reflectGet`, `core.js:3145`), and the
  `Mut` copy written back at run end (`reflectMutGet`, `:3201`). A layout-tier hole is the exact
  precedent: a column for the plain fields, a host call for the field the column cannot hold,
  keyed by the row's changed tick (`systems.rs:1577`, `c.holes`).
- What is not reusable as-is: `read`/`write` (`:572`, `:592`) take the value through
  `ReflectComponent`. A TS heap field needs its own `Target` kind (component id, field name,
  where the heap value lives) and a `put` from that value, not from reflection. `write` goes
  through `reflect_value::write` over a default (`:609`): for a `String` or a `Vec<f32>` that is a
  JSON string or array, which `value.rs` can decode without reflection.
- `type_data.rs` is about Bevy's own types lacking `ReflectDefault`/serde. Nothing here applies.
- `layout_reflect.rs` reads Rust layouts by reflection. It shows the rule this change must keep:
  a field with drop glue makes a type non-insertable from bytes (`layout_reflect.rs:333`,
  `layout.rs:283` `insertable`, `manifest.rs:646` `reflected`, test `:560`).

## 2. Who already owns what

| Concept | Owner | Covers strings and lists? |
|---|---|---|
| JSON shape per kind | `reflect_value` (docs/reflection.md matrix) | yes: `String` is its serde string, a list an array written whole |
| Binary wire format | `reflect_codec` (reflect-coverage stage 3, decision 7) | yes: `Str`, `List` |
| Insert from a script with a value bytes cannot carry | stage 2, `OP_INSERT_REFLECT`, layout `.of` with holes (archive `2026-10-26-reflect-coverage` tasks 2.3, 2.4) | Rust types only |
| TS component bytes to and from JSON | `value.rs` (`encode_component :171`, `decode_component :156`) | no: fixed `FieldType` only |
| Registration, identity, reload migration | `components.rs` (`register :152`, `commit :287`, `plan :182`) | no |
| Saves of TS components | `saves` (`TsSaved`, `saves.rs:46`, docs/saves.md) | JSON in, JSON out, so the file holds strings and arrays already; migration by shape (`saves.rs:588`) knows `Text` and `List` |
| TS field types | `layout.rs` `FieldType` (`:11`), read from TS by `bevy-ts-cli/src/reader.rs:986` | no; `reader.rs` already parses `string` for scene props only (`strings`) |
| Protocol field types | `bevy-ts-remote/src/protocol.rs:1195` (generated) | no |

So: the JSON shape and the wire format are owned and need nothing new. The new concept is "a TS
field whose value lives off the column", owned by `components.rs` + `layout.rs`, reusing
`reflect_codec`'s ops and `reflect_value`'s JSON shape.

## 3. Where fixed layout is assumed, and what breaks

A TS component is `layout.size` bytes, registered with no drop and copy-clone
(`components.rs:153`, `:171`: "plain bytes: no drop, any bit pattern is valid"). Every step below
leans on that.

| Step | Today | What breaks with a heap field |
|---|---|---|
| Declare | `reader.rs:986` maps a TS type to a `FieldType`; `components::validate` (`:119`) | `string`/`T[]` refused. Needs `FieldType::Str`, `FieldType::List { of }` with `size()` (`layout.rs:66`), `validate`, `ts_name`, protocol enum |
| Register | no drop fn, `ComponentCloneBehavior::Custom(clone_bytes)` | a heap field needs a drop fn and a deep clone, or the column holds a handle into a store that owns the heap value |
| Insert | `commands.rs:479` `insert_bytes` memcpys bytes; JS `encodeWithLayout` builds them; TSX (`bevy-ts-tsx/src/host.rs`, `resolve.rs:339`), levels (`bevy-ts-levels/src/engine.rs:396`), plugin API (`plugin_api.rs:221`), saves (`saves.rs:547`) all call `encode_component` | bytes cannot carry a string. Every one of these callers must create the heap value. Stage 2's split (bytes for the column, a patch for the rest, one command) is the pattern |
| Query read | JS typed-array views over the table column (`core.js:2045` `viewsFor`, `scalarCode :2088`), zero-copy | a heap value is not in the column. Read it as a hole: host call on a tick-cache miss, decoded by the `Str`/`List` op |
| Raw `q.tables` | typed arrays for every field | a native type with holes offers no raw writes (`layout.rs:279`); the same rule fits |
| Write | JS writes the column, sets the tick (`setterTail :2076`) | a heap write is a host call, as `reflectSet` (`core.js:3162`); `Mut` in-place edits of a list need `reflectMutGet`'s copy-and-flush |
| Change detection | per-row changed tick column; `Changed<C>` checked in Rust (`systems.rs:243`) | unchanged: the host write sets the tick, as `reflect_codec::write` does (`:616`) |
| Reload | `plan` keeps a field if its `FieldType` is equal; `commit` memcpys kept fields into new bytes, then `remove_by_id(old)` (`components.rs:306`) | a memcpy of an owning pointer then dropping the old component frees it twice (or, with a store, two rows share a handle). Kept heap fields must move, not copy |
| Undeclared / retired | data stays on entities | fine if the drop fn or store outlives the declaration |
| Clone (`cloneAndSpawn`) | `clone_bytes` (`:171`) | shallow copy = double free; needs a deep clone |
| Saves | `read_ts` (`saves.rs:227`) → `decode_component`; load: `migrate` then `encode` | decode/encode must read and build heap values. `same_shape` compares list lengths (`saves.rs:576`): a saved list of 3 vs a live default of `[]` resets the field. Variable lists need "any length, same item shape" |
| BRP and protocol reads | `bevy-ts-remote/src/values.rs:139` `decode_component` over raw bytes; edits via `resolve_path`/`encode_field` (`:14`) | same as saves: decode/encode must reach the heap value. Stock `bevy_remote` never sees TS components (not reflected) |
| Locals, TS resources | `memory.rs` `LocalStore` and TS resources use the same `FieldDecl`/layout | decide whether heap fields are allowed there too, or refused for now |

Two ways to hold the value, both inside one column of fixed size:

- Inline Rust value. The field is a `String` (24 bytes, align 8) or `Vec<T>` in the column; the
  descriptor gets a drop fn walking the heap fields; clone is deep. Bevy-like, no lookup. JS
  still cannot read Rust heap memory, so reads are host calls either way.
- Handle into a store. The field is a `u32` index into a per-world store of strings and lists;
  drop frees the slot. One indirection more, but bytes stay copyable for anything that only moves
  them.

Either way `value.rs` gains heap-aware encode/decode that every caller in the table goes through,
so the callers change in one place.

## 4. Where a per-query cost bench lives

docs/testing.md rule 8: a new bench lives in `bench/`, which the workspace excludes
(`Cargo.toml:4`). The fitting precedent is `bench/rust-events/` (a crate depending on
`bevy-ts`, `bevy-ts-cli`, `bevy-ts-v8` by path; `run.sh [runs]` builds release into the
workspace target and appends each run with `uptime` to `results/runs.txt`) and
`bench/world-saves/run.sh` (runs inside a check slot via `scripts/slot.sh`). Run it as a process
in `.abyme/processes.ts`, as `rust-events-bench` does (`gate-slot run … bench/rust-events/run.sh
${RUNS:-3}`). Arms per docs/reflection.md's bench (1,000 rows, median of 200 frames less an empty
system): a column read, a string field hit, a miss, a write. Its existing numbers are the
baseline: column 10 ns/row, hole hit 45, miss 176, write 3236 (docs/reflection.md:194). The
existing reflected bench is still in `tests/it/reflect_bench.rs`; the new one goes to `bench/`.

## 5. Ask-first list

- Performance: yes. Reads of a heap field are host calls (45–176 ns a row vs 10), writes cross as
  JSON (about 3 µs). A component with no heap field must cost what it costs today; the bench
  shows both. A drop fn on TS components with heap fields changes despawn cost for those only.
- New dependency: none needed.
- Data visibility: none. Same entities, same protocol readers.
- AI prompts or tools: none.
- Protocol: new `FieldType` kinds in `protocol.rs`/`protocol.d.ts` are a schema change (a minor
  bump, as reflect-coverage stage 1 did).

## Open questions

1. Inline `String`/`Vec` with a drop fn, or a handle into a store? Inline is Bevy's shape and
   cheaper to read; a store keeps every byte path (`insert_bytes`, migration memcpy,
   `clone_bytes`) safe without auditing each.
2. Does `Mut<C>` in-place list editing (`c.items.push(x)`) get the `reflectMutGet` copy-and-flush,
   or only whole assignment (`c.items = [...]`) at first?
3. Heap fields in `Local<{}>`, TS resources and scene props: in scope, or refused with a message?
4. Is "small arrays" a variable `T[]` (a `Vec`), or a fixed `[T; N]` that already exists as
   `FieldType::Array`? Only the first is a heap field.
5. List items: scalars and vectors only, or strings and nested lists too?
