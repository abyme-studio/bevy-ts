## 1. Walking skeleton: a cube moved by JS through a zero-copy Transform column

- [x] 1.1 Create the Cargo workspace (`bevy-ts`, `bevy-ts-v8`, `bevy-ts-cli` with a `bevy-ts` binary) on Bevy 0.20 and `deno_core` 0.412; the binary opens a window with a camera, a light and a cube spawned in Rust
- [x] 1.2 Define the backend trait in `bevy-ts` (load generation, run system, run startup, pump) and a fake backend for tests
- [x] 1.3 Create the `JsRuntime` on the main thread as non-send data and run one hard-coded JS script that declares one system writing `Transform`
- [x] 1.4 Register `Transform`'s native layout with `offset_of!` and hand its column and its changed ticks (`Table::get_changed_ticks_slice_for`) to JS as external `ArrayBuffer`s, detached after the run
- [x] 1.5 Run that system from an exclusive `Update` stage system with its own change tick; the script stores the tick on the rows it writes; the cube bobs on screen and a Rust `Changed<Transform>` system sees one entity
- [x] 1.6 Add a test that a retained view reads length 0 after the run, and that the build fails with `v8_enable_sandbox` on

## 2. TS compilation and module loading

- [x] 2.1 Compile `.ts` with `oxc_transformer` (type stripping, source maps) on a background thread
- [x] 2.2 Write the `deno_core` `ModuleLoader`: relative imports, the built-in `bevy-ts` and `bevy-ts/bevy` modules, a clear error for bare specifiers
- [x] 2.3 Map JS error stacks to TS locations and route them to the log with once-per-second dedup
- [x] 2.4 Compile `.tsx` with oxc's automatic runtime (`importSource: "bevy-ts"`), using the development transform under `bevy-ts dev`; resolve `bevy-ts/jsx-runtime` and `bevy-ts/jsx-dev-runtime` as built-in stub modules; test that an element carries its TS file, line and column
- [x] 2.5 Start the npm package `bevy-ts` with `app`, `component`, field types and their TS inference; `tsc` checks the demo

## 3. TS components

- [x] 3.1 Collect component declarations into the generation manifest and validate types, defaults and hints
- [x] 3.2 Compute C layouts in Rust; unit-test offsets for mixed, vector, enum, array and entity fields
- [x] 3.3 Register components with `new_with_layout` (table, no drop, summary tick) and return ids and offsets to JS
- [x] 3.4 Generate per-component accessors in the `bevy-ts` runtime from returned offsets, setters storing `ctx.tick` into the row's tick; test that a Rust `QueryBuilder` query reads what TS wrote and that only written rows pass `Changed`

## 4. TS systems and queries

- [x] 4.1 Build a `QueryState` per system from `read`/`write`/`with`/`without` with `QueryBuilder`; reject unknown and Rust-without-layout components
- [x] 4.2 Per run: take a change tick, walk matched tables, build data views per table × component and tick views per table × `write` component, copy entity pairs, make one call into JS, detach
- [x] 4.3 Implement `q.each` with reused accessors and `q.tables` with `read`, `write` and `split`; test that `each`, `write` and `split` give identical values and that `split` marks only the rows the caller ticked
- [x] 4.4 Add stages `preUpdate`/`update`/`postUpdate`/`fixedUpdate` with `BevyTsSet`, declaration order, `after`, and cycle errors
- [x] 4.5 After each run set summary ticks on `write` columns and fill ticks for tables taken through `table.write`; add the dev-only read-column hash check and, under `track_location`, the `changed_by` fill for rows at the run's tick
- [ ] 4.7 Add a test that a system writing 3 of 10,000 rows leaves exactly 3 rows `Changed` and that `bevy-ts/get_components+watch` sends nothing for an unwritten entity
- [x] 4.6 Add a test that 10,000 entities in 4 tables cost one JS call per run

## 5. Command buffer

- [x] 5.1 Define the binary op format and the JS encoder; build the Rust-owned arena with overflow kept in JS
- [x] 5.2 Apply spawn, despawn, insert, remove and setParent after each system with `insert_by_id` and `spawn_empty_at`
- [x] 5.3 Pre-fill the entity pool from `EntityAllocator::alloc` and free unused ids after the stage
- [x] 5.4 Add native spawn recipes `camera3d`, `directionalLight` and `cuboid`; discard a throwing run's commands

## 6. Hot reload

- [x] 6.1 Watch loaded files with `notify` and produce numbered generations on the dev thread
- [x] 6.2 Load each generation as generation-tagged side modules in the live isolate and apply it whole in `First`
- [x] 6.3 Track startups by name per world; implement `restart` (despawn TS-spawned entities, fresh isolate)
- [x] 6.4 Migrate changed layouts by field name to a new `ComponentId`, retire the old one, report the migration
- [x] 6.5 Keep undeclared components and revive them on redeclaration with the same layout
- [x] 6.6 Keep the old generation on any failure; tests for a syntax error, a throwing module, a duplicate name and a bad `after`
- [ ] 6.7 Wire `bevy-ts dev <entry>` with `--remote-port` and `--inspect`
- [ ] 6.8 Package for npm: a `bevy-ts` package whose bin resolves the engine binary from per-platform optional packages (`@bevy-ts/darwin-arm64`, …), as esbuild ships; `npx bevy-ts dev` works with no Rust toolchain

## 7. Debugging

- [ ] 7.1 Serve CDP from the V8 inspector in dev and pump it each frame and while paused
- [ ] 7.2 Verify breakpoints in TS sources hit in VS Code and Chrome, and still hit after a soft reload; if not, switch to stable specifiers with a fresh isolate and record why

## 8. Protocol schema and codegen

- [ ] 8.1 Write `protocol/bevy-ts.openrpc.json` (OpenRPC 1.3.2, version 1.0.0) for every method in the remote-protocol spec
- [ ] 8.2 Generate Rust types with `typify` and TS types for `bevy-ts/protocol`; add the regenerate-and-diff CI check
- [ ] 8.3 Let feature crates add methods to the same file with `x-bevy-ts-crate`, generated by the same run, and served only when their plugin is added
- [ ] 8.4 Add a conformance test that calls every method against the cube example and validates each response against the schema

## 9. Protocol methods

- [ ] 9.1 Create `bevy-ts-remote`: add the methods to `RemotePlugin` over `RemoteHttpPlugin`, on in dev, off in release by default
- [ ] 9.2 `bevy-ts/hello` and `bevy-ts/discover`
- [ ] 9.3 `bevy-ts/reload` and `bevy-ts/reload+watch`
- [ ] 9.4 `bevy-ts/schema` in `registry.schema`'s shape with `x-bevy-ts`
- [ ] 9.5 `bevy-ts/get_components`, `bevy-ts/get_components+watch` and `bevy-ts/mutate_components` for TS and reflected components, with path and type errors
- [ ] 9.6 `bevy-ts/entities` and `bevy-ts/entities+watch` from lifecycle observers registered only while watched, one message per frame with sequence numbers
- [ ] 9.7 `bevy-ts/pause` and `bevy-ts/step`, `bevy-ts/systems` and `bevy-ts/log+watch`

## 10. Plugin API for feature crates

- [x] 10.1 Manifest sections: registration, a JS writer in `bevy-ts`, delivery at apply time after declarations, rejection with TS locations failing the generation
- [x] 10.2 Host-invoked entries as the seam's fifth operation (fake backend and V8), with system error rules and a zero-call test when the plugin makes no call
- [x] 10.3 Plugin command kinds: opcode registration, JS append API, in-order application with built-in commands, an error for unknown opcodes
- [x] 10.4 Component id and layout lookup by name, and field-map-to-native-bytes encoding with defaults
- [x] 10.5 Change ticks by `ComponentId` and reflected JSON read/write by type path; test against `Text`
- [x] 10.6 Restart notification between despawn and first-time apply
- [x] 10.7 A test feature crate that uses every extension point against the fake backend

## 11. First-milestone demo

- [ ] 11.1 Write `examples/cube/src/game.ts` as in design.md and walk all six demo steps, recording reload latency
- [ ] 11.2 Measure frame cost of `bob` over 1, 1,000 and 100,000 entities, with and without the tick store, and record it next to `docs/research/change-detection.md`'s numbers
