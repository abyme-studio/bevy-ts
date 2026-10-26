Order: stage 1 (places, resources, lifecycle copy) first, as stages 2, 3 and 5 address heap values
through its places. Stage 4 (item kinds) before stage 5 (in-place edits), so edits cover every item
kind once. Each stage lands alone. Tests follow docs/testing.md: a module of the crate's one test
binary, lowest layer, no fixed waits. Every cargo job through `~/dev/abyme/scripts/gate-slot`.

## 0. Before starting

- [ ] 0.1 Operator's go on design.md "Ask first" (heap places, end-of-run list diff, protocol minor, lifecycle copy). Stage 1 does not land without it

## 1. Stage 1: places, resources, lifecycle values

- [ ] 1.1 `crates/bevy-ts/src/reflect_codec.rs`: one slot layout for every reflect and heap host call (design decision 4): slot 0 entity bits or a place kind (resource -1, `Local` -2, message -3, observer value -4), slot 1 place id, slot 2 target, then the call's own; a `RunPlace` resource the runner sets once per system or observer run; a non-live place fails naming it. `Target::ts_heap` keyed by the layout's owner name, so non-component layouts get targets
- [ ] 1.2 `crates/bevy-ts-v8/js/core.js`: each accessor kind binds its place (`rowEntity` replaced by a per-bind place kind and id); `reflectFetch`, `heapSet` and reflected writes put slots in the new layout. Regenerate `packages/bevy-ts/runtime/core.js` the way the repo does
- [ ] 1.3 `crates/bevy-ts/src/generation.rs`: drop the resource refusal (`:398`); `crates/bevy-ts/src/systems.rs` `RunParam::Res`: a heap resource's frame carries its changed tick, as `ResMut` does
- [ ] 1.4 `crates/bevy-ts/src/conditions.rs` `ResourceEquals`: heap fields compared by value (`heap::read`), plain ranges as today; the expected value built as `HeapBytes`
- [ ] 1.5 `crates/bevy-ts/src/observers.rs`: `Pending.bytes` a `HeapBytes`; a heap component's `Remove`/`Despawn`/`Discard` value copied with `HeapBytes::copy_of`; `Pending` not `Clone`; `crates/bevy-ts/src/systems.rs` drain sets the observer place and drops the value once after the last observer
- [ ] 1.6 `crates/bevy-ts-cli/src/reader.rs` `field_ty`: `string` and `T[]` allowed on resources
- [ ] 1.7 Tests: `crates/bevy-ts/tests/it/heap_fields.rs` a resource's string freed once on restart and world drop; a lifecycle copy freed once after the drain; `crates/bevy-ts-cli/tests/it/heap_fields.rs` `ResMut` write seen by `Res`, second `Res` read makes no host call (`reflect_codec::CALLS`), `On<Despawn<Tag>>` reads `label`; `crates/bevy-ts-cli/tests/it/conditions.rs` `resourceEquals` on a string; `crates/bevy-ts/tests/it/saves.rs` a resource string round-trips
- [ ] 1.8 Bench (`bench/ts-heap-fields/src/main.rs`): plain `Res` read control (built against main too), heap `Res` string read, `On<Remove>` value read on a heap component
- [ ] 1.9 Docs: `docs/ecs.md` (resources, lifecycle copy replaces "reads live, throws once gone"), `docs/scripts.md` field-type rules
- [ ] 1.10 `cargo build`, tests of `bevy-ts` and `bevy-ts-cli` through `gate-slot`; `openspec validate ts-heap-fields-2 --strict`

## 2. Stage 2: `Local`

- [ ] 2.1 `crates/bevy-ts/src/memory.rs`: `LocalStore` owns a 16-aligned buffer with a header and shape when its layout has a heap field, `Drop` through the shape; `LocalStore` and `Memory` not `Clone`; `redeclare` moves kept heap fields (`heap::take_value`), resets changed ones; `new` builds defaults through `value::encode_component`
- [ ] 2.2 `crates/bevy-ts/src/systems.rs`: drop the `Local` refusal (`:1106`); the run sets the `Local` place; the resolution hands JS a `Local`'s Rust layout and heap holes
- [ ] 2.3 `core.js`: a `Local` with a heap field takes Rust's layout, not `cLayout`; its reads cached until its own write
- [ ] 2.4 `reader.rs`: heap types allowed in `Local<{...}>`
- [ ] 2.5 Tests: `crates/bevy-ts/tests/it/heap_fields.rs` a `Local`'s string freed once on redeclare-with-change, restart and world drop, kept on reload; `crates/bevy-ts-cli/tests/it/heap_fields.rs` a `Local<{ log: string; hits: u32[] }>` keeps its values across runs and a reload that adds a field (the spec's `Entity[]` scenario runs once stage 4 lands, in 4.10)
- [ ] 2.6 Bench: plain `Local` read control, `Local` string read
- [ ] 2.7 Docs: `docs/ecs.md` `Local` line
- [ ] 2.8 `cargo build`, tests of `bevy-ts` and `bevy-ts-cli` through `gate-slot`

## 3. Stage 3: messages and events

- [ ] 3.1 `crates/bevy-ts/src/messages.rs`: `MessageQueue` for a heap layout keeps an 8-aligned buffer and a shape; `push` takes `HeapBytes` by value; `update` drops each `older` record through the shape; `Drop` both halves; plain queues unchanged; `MessageQueue` and `TsPayloads` not `Clone`
- [ ] 3.2 `crates/bevy-ts/src/generation.rs`: rollback moves the old `TsPayloads` aside instead of `clone` (`:455`); `declare` moves queues across and a failed reload moves them back; drop the message and event refusal (`:393`)
- [ ] 3.3 `crates/bevy-ts/src/commands.rs`: `Op::WriteMessage` and `Op::Trigger` build heap payloads with `HeapFill::build` from `encodeWithHeap` bytes
- [ ] 3.4 `crates/bevy-ts/src/plugin_api.rs` `write_message`: encode through `value::encode_component`
- [ ] 3.5 `crates/bevy-ts/src/systems.rs`: `MessageReader` frames give each record's sequence for the message place; events read at the observer place (stage 1)
- [ ] 3.6 `core.js`: messages and events set `C.heap` and holes (read-only, cached per record); `MessageWriter.next()` keeps heap values set on its view and sends them with the record; `On<E>.event` reads heap fields through the place
- [ ] 3.7 `reader.rs`: heap types allowed on `Message`, `Event`, `EntityEvent` fields
- [ ] 3.8 Tests: `crates/bevy-ts/src/messages.rs` unit: a heap queue frees each record once on `update` and on drop; `crates/bevy-ts/tests/it/heap_fields.rs` a failed reload keeps unread heap messages and frees nothing twice; an event over two observers and two hops freed once; `crates/bevy-ts-cli/tests/it/heap_fields.rs` `Chat { text }` read by two readers, `next()` with a string, an event with a list seen by two observers, a plain message reader makes no host call
- [ ] 3.9 Bench: plain message write + read and plain event trigger + observer read (controls against main), message with a string, event with a string
- [ ] 3.10 Docs: `docs/ecs.md` messages and events
- [ ] 3.11 `cargo build`, tests of `bevy-ts` and `bevy-ts-cli` through `gate-slot`

## 4. Stage 4: richer list items

- [ ] 4.1 `crates/bevy-ts/src/layout.rs`: `FieldType::Struct { fields }` (allowed only as a list item, `c_layout` of plain members); `validate` allows `List` of vectors, `Entity`, `Enum`, `Struct`; refuses nested lists and heap members of a struct
- [ ] 4.2 `crates/bevy-ts/src/heap.rs`: `Item` gains `Vec2`, `Vec3`, `Vec4`, `Quat`, `Entity`, `Enum { width }`, `Struct(&'static ItemShape)`; storage `Vec<Vec2>`, `Vec<Vec3>`, `Vec<[f32; 4]>`, `Vec<Entity>`, `Vec<u8|u16|u32>`, struct items as a raw list with `len` and `cap` in items, allocated with the item's layout, `Vec`'s field order probed once (design decision 9); `ItemShape` leaked per layout; drop, clone and codec per kind
- [ ] 4.3 `crates/bevy-ts/src/value.rs`: JSON encode and decode of the new items (vectors as arrays, entities as numbers, enums as names, structs as objects)
- [ ] 4.4 `crates/bevy-ts/src/reflect_codec.rs`: heap target codec items `Lanes`, `Entity`, `Enum`, `Struct`
- [ ] 4.5 `core.js`: encoders for the four item nodes; read items frozen (vector values, plain objects); `packages/bevy-ts/index.d.ts`: `readonly ReadonlyVec3[]` etc. on `Read`
- [ ] 4.6 `reader.rs` `field_ty`: list items of vectors, `Entity`, unions and object types (via `object()`), with file and line for refusals; `crates/bevy-ts-cli/src/types.rs` if it names field types
- [ ] 4.7 `crates/bevy-ts/src/saves.rs`: `map_entities` into lists and struct items; `migrate` keeps a struct list only when item members match
- [ ] 4.8 `crates/bevy-ts-remote/src/protocol.rs`, `values.rs`: item kinds and JSON Schema for them; regenerate `protocol/bevy-ts.openrpc.json` (`scripts/gen-protocol.sh`), next free minor
- [ ] 4.9 `crates/bevy-ts-levels`: an entity list from a level fails as an `Entity` field does
- [ ] 4.10 Tests: `crates/bevy-ts/src/layout.rs` unit: struct item layout and stride; `crates/bevy-ts/tests/it/heap_fields.rs` each item kind freed once on every path, a Rust system reads `Vec<Vec3>` and a `#[repr(C)]` struct `Vec` from the bytes; a Rust system pushes onto a struct list, TS reads every item, and it is freed once; `crates/bevy-ts/src/heap.rs` unit: the `Vec` field-order probe; `saves.rs` an entity list remapped, a struct item member added resets; `crates/bevy-ts-cli/tests/it/heap_fields.rs` read each kind, refusals of `f32[][]` and `{ name: string }[]` with file and line; `protocol.rs` schema of a struct list validated against the protocol schema
- [ ] 4.11 Bench: 8-item `Vec3[]` hit and miss
- [ ] 4.12 Docs: `docs/ecs.md`, `docs/scripts.md`, `docs/saves.md`
- [ ] 4.13 `cargo build`, tests of `bevy-ts`, `bevy-ts-cli`, `bevy-ts-remote`, `bevy-ts-levels` through `gate-slot`

## 5. Stage 5: in-place list edits and `.field[i]`

- [ ] 5.1 `crates/bevy-ts/src/heap.rs`: `List::apply(ops)`, ops set(i, item), append(items), truncate(len), replace(all), each checked against the item kind and length
- [ ] 5.2 `crates/bevy-ts/src/reflect_codec.rs`: `WRITE_EDIT` host call at any writable place (entity, resource, `Local`): ops in codec bytes, applied in place, change tick set once, cache entry dropped
- [ ] 5.3 `core.js`: a list on a writable accessor reads as a mutable array with a snapshot (the `reflectMutGet` path); `reflectFlush` diffs touched lists into ops, nothing for an unchanged one; a heap `READ` of a row with pending edits flushes first; whole assignment stays an immediate `WRITE` and replaces the array and its snapshot; a hole at flush throws a `TypeError` naming the field and index and writes nothing for that list; the observer runner flushes as `runSystem` does. `index.d.ts`: `T[]` on `Mut`, `ResMut` and `Local`
- [ ] 5.4 `crates/bevy-ts-remote/src/values.rs`: `.field[i]` and `.field[i].member` on a list write through `List::apply`; past the end fails naming the length; same protocol minor as stage 4 if unreleased
- [ ] 5.5 Tests: `crates/bevy-ts/src/heap.rs` unit: each op and its bounds; `crates/bevy-ts-cli/tests/it/heap_fields.rs` `push` and `[0] =` reach Rust as one set and one append (an op counter beside `reflect_codec::CALLS`), seen by `Changed<Tag>`; a read-and-untouched list marks nothing; `sort` falls back to a whole write; an assignment after a push sends only the assignment; `a[10] = x` past the end throws naming the field and index and leaves the list unchanged; `NaN` items unchanged write nothing; an in-place edit in an observer with `Mut<C>` is seen after it; edits on `ResMut` and `Local`; `protocol.rs` `.scores[0]` edit and the past-the-end error; `packages/bevy-ts/test/heap_fields.ts` `push` allowed on `Mut`, refused on read
- [ ] 5.6 Bench: 8- and 64-item `f32[]` `push` in place against whole assignment, `[i] =` one item
- [ ] 5.7 Docs: `docs/ecs.md` (edits, when they are seen, cost, marked only on a real change as `set_if_neq`; a Rust mirror of `Vec4`/`Quat` items uses `[f32; 4]`), `docs/reflection.md` (heap edits share the flush), `docs/research/bevy-coverage.md` row updated to the remaining follow-ups (nested lists, heap members in items, struct fields outside lists)
- [ ] 5.8 `cargo build`, tests of `bevy-ts`, `bevy-ts-cli`, `bevy-ts-remote` through `gate-slot`; `openspec validate ts-heap-fields-2 --strict`

## 6. Bench verdict

- [ ] 6.1 `bench/ts-heap-fields/run.sh` through process `ts-heap-fields-bench`, 3 runs per arm, 8 where arms are within noise; plain controls match main, stage-1 arms match stage 1's numbers (design.md Context); a table in `docs/ecs.md`
