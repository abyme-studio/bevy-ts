## 0. Before starting

- [ ] 0.1 Operator's go on the ask-first items in design.md ("Ask first": host-call reads, protocol minor bump). Stage 1 does not land without it

## 1. Stage 1: `string` fields

Core (`crates/bevy-ts`):

- [ ] 1.1 `layout.rs`: `FieldType::Str`, `is_heap()`, `size()` 24 / `align()` 8 with a `const` assert against `size_of::<String>()`, `ts_name` `string`; `validate` refuses `Array { of: Str }`. `ComponentLayout` puts an 8-byte header at offset 0 when a field is heap, none otherwise (design decision 1)
- [ ] 1.2 New `heap.rs`: `HeapShape` (heap fields' offsets and kinds, a per-shape live count), `HeapBytes` (owns its values, drops through the shape, not `Clone`), the drop fn and the deep clone fn reading the header (design decisions 1, 2)
- [ ] 1.3 `components.rs`: `register` passes the drop fn and clone fn for a layout with a shape, today's `None` and `clone_bytes` otherwise; `validate` encodes defaults through `HeapBytes`; `commit` moves kept heap fields and builds defaults per entity (decision 5)
- [ ] 1.4 `value.rs`: `encode_component` returns `HeapBytes`; `decode_component` reads `Str` through the header; `encode_field`/`decode_field`/`zero` for `Str`. Every caller compiles against it: `commands.rs`, `plugin_api.rs`, `saves.rs`, `crates/bevy-ts-tsx/src/host.rs`, `crates/bevy-ts-levels/src/engine.rs`, `crates/bevy-ts-remote/src/values.rs`
- [ ] 1.5 `commands.rs`: `insert_bytes` heap form taking `HeapBytes` by value; JS bytes for a heap component get every heap slot written by Rust; `OP_INSERT_REFLECT` mode `TS_HEAP_PATCH` (component id, field index, codec bytes), one command with its insert (decision 3)
- [ ] 1.6 `reflect_codec.rs`: `Target` kind TS heap (component id, offset, `FieldType`), codec `Node::Str` from the `FieldType`; `READ` decodes it, `WRITE` takes codec bytes, sets the change tick (decision 4)
- [ ] 1.7 `systems.rs`: `Access.holes` true for TS components with a shape, so the frame carries changed ticks; resolution hands JS the heap fields as holes
- [ ] 1.8 Backstop: `generation.rs` (messages, events), `memory.rs` (`Local`) and TS resources refuse `is_heap()` fields, "heap fields are allowed on components only"

Reader, JS, types:

- [ ] 1.9 `crates/bevy-ts-cli/src/reader.rs` `field_ty`: a `heap` flag for component fields; `string` → `{ kind: "string" }`; other declarations get the components-only error with file and line
- [ ] 1.10 `crates/bevy-ts-v8/js/core.js`: heap fields as holes on TS component accessors (`addHoles`); the `Str` encoder beside `reflectDecoder`; `encodeWithLayout` skips heap fields and spawn/insert emit `TS_HEAP_PATCH`; `q.tables` gives no raw arrays for a component with a shape
- [ ] 1.11 `packages/bevy-ts/index.d.ts`: `string` fields typed `string` on `Read` (readonly) and `Mut`

Protocol:

- [ ] 1.12 `crates/bevy-ts-remote/src/protocol.rs` `FieldType` `string`; `values.rs` decode and `.field` edits through `value.rs`; regenerate `protocol/bevy-ts.openrpc.json` (`scripts/gen-protocol.sh`); bump to the next free minor

Tests (docs/testing.md: one module per crate binary, lowest layer, no fixed waits):

- [ ] 1.13 `crates/bevy-ts/src/layout.rs` unit tests: header offsets; a plain component's layout unchanged (`{ alive, hp, pos }` size 20)
- [ ] 1.14 `crates/bevy-ts/tests/it/heap_fields.rs` (new, `mod heap_fields;` in `main.rs`): each path frees every string once by the shape's live count: insert, `cloneAndSpawn` clone (own copy), reload keep / reset / remove, despawn, world drop, an undeclared component despawned after its class is gone
- [ ] 1.15 `crates/bevy-ts/tests/it/saves.rs`: a `string` round-trips; a field changed from `string` to `f32` resets
- [ ] 1.16 `crates/bevy-ts-cli/tests/it/heap_fields.rs` (new): a TS system reads and writes `label`; a write seen by a Rust system the same frame and by `Changed<Tag>`; the second frame on untouched rows makes no host call (`reflect_codec::CALLS`); a resource with `string` fails naming components-only; `Vec3[]` and object fields fail with file and line
- [ ] 1.17 `crates/bevy-ts-cli/tests/it/protocol.rs`: `get_components` and `.label` edit of a TS `string`; schema validated against the protocol schema
- [ ] 1.18 `crates/bevy-ts-cli/tests/it/tsx.rs` and `crates/bevy-ts-levels/tests/it/levels.rs`: one string prop each reaches the entity (only the call site, not the encoder again)
- [ ] 1.19 `crates/bevy-ts-cli/tests/it/typescript.rs`: `tsc` refuses `tag.label = "x"` on a read `Tag`

Bench and docs:

- [ ] 1.20 `bench/ts-heap-fields/` (crate as `bench/rust-events/`, `run.sh [runs]`, `results/runs.txt` with `uptime`) and process `ts-heap-fields-bench` in `.abyme/processes.ts` through `gate-slot`. Arms: plain column, heap component's plain field, string hit, miss, write, despawn of 1,000 heap rows vs plain. 3 runs per arm, 8 when arms are within noise
- [ ] 1.21 `docs/ecs.md`: heap fields (declaring, reading, whole assignment, what costs a host call) with the bench numbers
- [ ] 1.22 `docs/scripts.md`: the field types list gains `string`; components only
- [ ] 1.23 `docs/reflection.md`: the read path also serves TS heap fields, writes as codec bytes; link docs/ecs.md
- [ ] 1.24 `docs/saves.md`: strings in saved TS components
- [ ] 1.25 `docs/research/bevy-coverage.md`: a row "TS component `String` / `Vec<T>` fields" after "TS components (layout columns)", with status and tests
- [ ] 1.26 `cargo build` and the tests of the crates touched (`bevy-ts`, `bevy-ts-cli`, `bevy-ts-remote`, `bevy-ts-tsx`, `bevy-ts-levels`) through `gate-slot`; `openspec validate ts-component-heap-fields --strict`

## 2. Stage 2: `T[]` fields

- [ ] 2.1 `layout.rs`: `FieldType::List { of }`, 24 bytes align 8; `validate` allows a number kind, `Bool` or `Str` as `of`; `ts_name` `T[]`
- [ ] 2.2 `heap.rs`: list kinds in `HeapShape` (drop and deep clone per item type; `Vec<String>` drops each string)
- [ ] 2.3 `value.rs`: encode and decode `List` from and to JSON arrays, items checked against `of`
- [ ] 2.4 `reflect_codec.rs`: TS heap target codec `Node::List { of }` with the scalar or `Str` item op
- [ ] 2.5 `reader.rs`: `T[]` for component fields with an allowed `T`; others fail with file and line
- [ ] 2.6 `core.js`: `List` encoder; a list reads as a frozen array; `Mut` assigns a whole array; `index.d.ts`: `readonly T[]`
- [ ] 2.7 `saves.rs` `migrate`: a heap field is kept when its saved value encodes as the live type, any length (design decision 6)
- [ ] 2.8 Protocol: `list { of }` in `protocol.rs`; `.field[i]` on a list refused naming the whole-field form; schema regenerated, same minor as stage 1 if unreleased, else the next
- [ ] 2.9 Tests: `crates/bevy-ts/tests/it/heap_fields.rs` lists of `f32`, `bool`, `string` freed once on every path; `saves.rs` a list of 3 round-trips against a default `[]`, an item type change resets; `crates/bevy-ts-cli/tests/it/heap_fields.rs` read, whole write, `push` throws a `TypeError`; `protocol.rs` whole-list edit and the `[i]` refusal; `typescript.rs` `push` refused on `readonly T[]`
- [ ] 2.10 Bench arms: 8-item `f32[]` hit, miss, write; numbers into docs/ecs.md
- [ ] 2.11 Docs: `docs/ecs.md`, `docs/scripts.md`, `docs/saves.md`, the coverage row updated for lists
- [ ] 2.12 `cargo build` and the touched crates' tests through `gate-slot`; `openspec validate ts-component-heap-fields --strict`

## 3. Follow-ups (not this change)

- [ ] 3.1 Record as follow-ups in docs/research/bevy-coverage.md: in-place list edits (`Mut` copy-and-flush), heap fields in `Local`, TS resources, messages and events, list items of vectors, `Entity` and structs, `.field[i]` protocol edits of a list
