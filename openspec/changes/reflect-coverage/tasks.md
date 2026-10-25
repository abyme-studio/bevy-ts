## 0. Before starting

- [ ] 0.1 `reflect-enums-color` has landed; rebase on main. Rebase on bevy-ui-coverage and mesh-primitives if they have landed, keeping their sides of each seam (design, "Seams")

## 1. Stage 1: one shape for every kind (TSX and protocol)

- [x] 1.1 `reflect_value::read` for every kind in design decision 1; deterministic order for sets and non-string-keyed maps; newtype components as `{ value }` per bevy-ui-coverage
- [x] 1.2 Extend `reflect_value::write` to the same kinds: lists, sets and maps written whole; tuples and arrays from arrays; math types; `Duration` from seconds or `{ secs, nanos }`; wide integers from numbers or strings; `Entity`; opaque via `ReflectDeserialize`; `Handle` `null` keeps the default
- [x] 1.3 `reflect_value::UNCOVERED` with a reason per entry; computed components refuse writes naming themselves
- [x] 1.4 Unit tests: a fixture component with one field per kind round-trips; `u64` past 2^53; a list replaced; a map with `Entity` keys
- [x] 1.5 `crates/bevy-ts-cli/tests/it/reflect_coverage.rs`: the hole check (design decision 3), including the stale-entry check; green on the prebuilt engine plus Rapier
- [x] 1.6 `types.rs`: every kind's TS type; no covered type as `unknown`; regenerate `bevy.d.ts`; `tests/typescript.rs` cases for collections, `Duration`, wide integers, maps
- [x] 1.7 TSX: a reflect-tier or hole prop of every kind loads through `write` (extend stage 1's `reflect_values.rs`)
- [x] 1.8 Protocol: `get_components`, `+watch` and `mutate_components` use `read`/`write` for reflected components; regenerate `protocol/bevy-ts.openrpc.json`; bump to the next free minor
- [x] 1.9 `docs/reflection.md`: the coverage matrix (kind, read, write, TS type, test) and the uncovered list; link from `docs/native.md` and `README.md`

## 2. Stage 2: scripts write; recipes go

- [ ] 2.1 Rebase on main at or after 0dc7cc1, ambient-light-script's default-bytes encode (`ResolvedComponent.defaults` in `manifest.rs` and `generation.rs`; `encodeWithLayout` in `core.js` starts from `C.defaults`)
- [ ] 2.2 Generated reflect-tier classes become `reflectClass(name)` with `.of(json)` and default use; filters keep working
- [ ] 2.3 `OP_INSERT_REFLECT` in `core.js` and `commands.rs`; `spawn`/`insert` emit it; Rust writes over `ReflectDefault`/`ReflectFromWorld` and inserts; a failing value fails only its command, naming type and field
- [ ] 2.4 Layout-tier `.of` with hole fields: bytes from `encodeWithLayout`, holes as a reflect patch in the same command
- [ ] 2.5 `Transform.lookingAt(eye, target, up?)` in the JS runtime and `index.d.ts`; test against Bevy's `looking_at`
- [ ] 2.6 Delete `crates/bevy-ts/src/recipes.rs` and its registration; `spawnNative` of a removed name fails naming the replacement; drop the three `spawnNative` overloads from `index.d.ts`
- [ ] 2.7 Migrate `examples/cube/src/game.ts`, `examples/arena/src/game.tsx`, the CLI tests calling `camera3d` (`material_errors.rs`, `play_stop.rs`, `tsx.rs`, `viewport_edit.rs`, `viewport_walk.rs`, `viewport_level.rs`) and `docs/materials.md`; grep for `spawnNative("` finds no built-in recipe
- [ ] 2.8 `crates/bevy-ts-cli/tests/reflect_scripts.rs`: the orthographic camera, warm sun and removed-recipe scenarios

## 3. Stage 3: scripts read (operator's go first)

- [x] 3.1 Operator's go recorded for the per-frame reflected read path; do not start without it
- [x] 3.2 Drop `NotReflect`; reflect-tier classes as query data and `Res`/`ResMut`; tables without columns for them
- [x] 3.3 Reads and writes per design decision 7: the changed-tick column for reflect-tier terms in `systems.rs`; the per-type tick cache (dropped on reload, entries on despawn and removal); the op compiler in `reflect_value` from `TypeInfo`, with JSON fallback for types it cannot express; the JS decoder and encoder generated from the ops; host calls `reflect_read(entity, type)` and `reflect_write(entity, type, path, bytes)`; a write applied at once with the change tick and the cache entry refreshed; the hole check asserts decoder output equals `JSON.parse` of `read` per covered type
- [x] 3.4 Hole getters and setters on layout-tier row accessors; never in `q.tables`
- [x] 3.5 `NodeRef.get` reads reflected components and holes through `reflect_read` and subscribes as today
- [x] 3.6 Tests: the camera switch, untouched rows make no host call, column views valid after a reflected write, computed components refuse writes
- [x] 3.7 Bench (`crates/bevy-ts-cli/tests/it/reflect_bench.rs`, ignored, like the repo's other benches; it needs V8) over 1,000 rows of `DirectionalLight.color` and `Projection`: reads on cache hits, reads on misses through the codec, the JSON path as the baseline, and writes; against a layout-tier column read. Targets from the research: about 10 ns a hit, about 0.1 µs a miss. The numbers go into `docs/reflection.md` (measured at load 31–71: targets unmet, every read tier well clear of JSON)
- [x] 3.8 `docs/native.md`: reflected rows and holes in TS systems
- [ ] 3.9 Follow-up: one `Mut` write a frame costs about 30 µs, a fixed flush cost per run; find it and cut it

## 4. Landing (each stage)

- [ ] 4.1 `cargo test` for the touched crates while working; the workspace build, tests and clippy once before landing; `pnpm check`
- [ ] 4.2 `openspec validate reflect-coverage --strict`
