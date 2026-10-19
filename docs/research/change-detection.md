# Change detection for TS systems over zero-copy columns

Recommendation: hand JS Bevy's own per-row changed-tick array as a second zero-copy `Uint32Array` beside each `write` column, and have the generated accessor setters store the run's tick on the row they write (`ticks[row] = tick`); `q.tables` gets Bevy's two `ContiguousMut` surfaces, `write(T)` (whole table marked) and `split(T)` (arrays plus `ticks`, caller marks). No post-run scan, no copy, no bitset, no command for a field write.

Researched and measured 2026-10-19 on this Mac: Apple M5 Max, 128 GB, macOS 25.4. Engines: deno 2.9.7 (V8 15.0.245), node 26.12 (V8 14.6), QuickJS-ng 0.17.0, rustc 1.98. Bevy 0.20.0 source read from `~/dev/refs/bevy-0.20`. Benchmarks in `bench/change-detection/` (`write-paths.js`, `accessor-forms.js`, `rust/`), raw output in `bench/change-detection/results/`.

## What decides it

- **Bevy's grain is the row, and `Changed<T>` reads nothing else.** `Column` keeps `changed_ticks` as a contiguous `u32` array indexed like the data (`storage/table/column.rs`), reachable through `Table::get_changed_ticks_slice_for`. `Changed<T>::filter_fetch` reads that row's tick (`query/filter.rs`); the summary tick is only ever consulted by code that calls `summary_tick_is_changed()` on a contiguous iterator. So "summary ticks let readers skip unwritten tables" was wrong for the readers that matter: a coarse mark is paid per row by every `Changed<T>` system. Bevy itself has several on the path a TS system touches: transform propagation (`Changed<Transform>`, `bevy_transform/src/systems.rs:47,112`), visibility (`Changed<GlobalTransform>`), light and mesh extraction (`bevy_pbr/src/render/{light,mesh,skin}.rs`), plus `bevy_remote`'s watch (`get_change_ticks_by_id`), bevy-ts's own watch streams and TSX's `useComponent`. Marking a 1M-row table changed because a system moved 3k rows re-propagates and re-extracts 1M transforms a frame.
- **The per-row tick store is a typed-array store, and JS can make it directly into Bevy's array.** The tick slot lives in the row, is swap-removed with the row, and reallocates with the column, so nothing has to be remapped; structural changes are deferred past the run anyway. The stage is exclusive, so no Rust system reads a column while JS writes its ticks. The summary tick is an `AtomicTick` Rust sets once per `write` column after the run; one set when nothing changed is safe under Bevy's documented contract, one missed is not.
- **On V8 the store is nearly free.** Raw loop over 1M rows writing every row: 1.88 ms without, 2.26 ms with the tick store (deno); 2.84 / 2.84 (node). Through flyweight accessors: 1.77 / 2.15 (deno), 2.18 / 2.71 (node). Writing 0.3% of rows: within run-to-run noise. TurboFan inlines the accessors fully (accessor loop = raw loop at 100k and 1M), and typed arrays over 64 bytes are off-heap in V8 anyway (`JSTypedArray::kMaxSizeInHeap = 64`), so an external backing store compiles to the same code as an allocated one. The optimized loop body for `ticks[i] = tick` (node, `--no-maglev --print-opt-code`) is a bounds compare, a branch and a `str`, plus a Smi-or-HeapNumber check on the tick because it arrived as an untyped parameter; a tick read from the frame descriptor's `Uint32Array` is a known integer. Tick values stay below 2^31 for about 10 hours at 60k system runs a second; past that V8 reoptimizes once with a double representation.
- **Without a JIT the loop is bound by property dispatch, not the store.** QuickJS-ng, 100k rows written through three setters: class accessors 36.7 ms, with the tick store 50.5 ms; raw loop 8.3 / 8.9 ms. The store costs a quarter to a third of an accessor loop that is already 4× a raw loop and 100× V8. `accessor-forms.js` shows the fix belongs to the accessor form, which is per engine, not to the tick rule: on QuickJS-ng a closure-scoped accessor (row position in module variables, getters on an object literal) runs 25% faster than the class form and the tick store on top of it is free (78.2 → 79.0 ms); on V8 that same form is 100× slower under TurboFan (25 ms against 0.24) and slower in jitless too, so the V8 backend keeps class accessors, where the store costs 10 to 18%. Each backend's `bevy-ts` runtime generates its own accessor form; the setter stores the tick in both.
- **Writing the same value marks the row, as Bevy's `Mut` does.** Bevy's `DerefMut` marks on any write and offers `set_if_neq` for callers who care. The comparing setter costs 2× the plain one on V8 (`acc+neq` 6.1 ms against `acc+tick` 3.1 ms at 1M), so it is not the default; a TS author writes `if (m.x !== v) m.x = v` where it matters. A shadow copy would be the only path that marks only real changes, and it is the most expensive one (below).
- **Prior art agrees once the grain is accounted for.** Unity DOTS tracks change versions per chunk and bumps them on declared write access, not on a value change ("the change filter applies to whole archetype chunks, and not individual entities"; "only checks whether a system that declared write access to the component has run"). Its ecosystem lives with that by declaring read-only access everywhere it can, by comparing values after `DidChange`, and because a chunk is at most 16 KB (about 128 entities); a Bevy table is unbounded, so the coarse rule is far worse here than there. flecs tracks a dirty counter per table per component; a write through `ensure` is invisible until `ecs_modified`, `set` marks, iterating a query with `out` terms marks unless `skip()`; so the write API is the mark, which is what the accessor setter is. flecs has no official JS binding (flecs-polyglot is a WIP wasm scripting layer). Bevy 0.20's `ContiguousMut` marks every row on `DerefMut` and hands `split()` to callers who want to mark rows themselves; `q.tables` mirrors that pair exactly.
- **Options that only move the cost around.** A separate dirty bitset costs the same store in JS plus a scan per column per run in Rust and a remap if rows moved; the tick array already is the bitset. Per-page (64-row) dirty bits need a Rust fill per touched page and lose the per-row precision the consumers want. Write intent per field changes nothing Bevy records. Field writes through the command buffer handle every value twice, break "a system sees its own writes", and cost 2.5× the accessor loop before Rust applies an op.

## Numbers

### JS write loop, V8 with JIT, ms per run (median; deno, node in brackets)

A 32-byte component, three `f32` fields written per touched row, `cond` means the system's own logic writes that fraction of rows and reads the rest.

| rows | path | 0.3% written | 10% | 100% |
|---|---|---|---|---|
| 100k | raw typed arrays | 0.066 | 0.146 | 0.324 |
| 100k | raw + tick store | 0.066 | 0.140 | 0.370 |
| 100k | accessor | 0.093 | 0.172 | 0.324 |
| 100k | accessor + tick store | 0.101 | 0.184 | 0.373 |
| 100k | accessor, writes as commands (JS half only) | 0.084 | 0.182 | 0.443 |
| 1M | raw typed arrays | 0.52 (0.76) | 1.13 (1.30) | 1.88 (2.84) |
| 1M | raw + tick store | 0.56 (0.74) | 1.21 (1.65) | 2.26 (2.84) |
| 1M | raw + separate dirty bitset | 0.55 (0.57) | 1.18 (1.22) | 2.59 (2.51) |
| 1M | accessor | 0.51 (0.70) | 1.10 (1.23) | 1.77 (2.18) |
| 1M | accessor + tick store | 0.68 (0.53) | 1.24 (1.27) | 2.15 (2.71) |
| 1M | accessor + dirty flag, tick once per row | 1.08 (0.73) | 1.63 (1.39) | 2.55 (2.61) |
| 1M | accessor, writes as commands (JS half only) | 0.55 (0.57) | 1.45 (1.52) | 4.39 (5.34) |
| 1M | accessor + compare before write (`set_if_neq`), every row written | 6.13 (3.38) | 6.53 (4.85) | 6.08 (4.83) |
| 1M | accessor + tick store, every row written | 3.14 (2.13) | 3.14 (2.14) | 3.14 (3.40) |

### Post-run passes in Rust, criterion, median

Per `write` column per run. `mark_all` is option A. `bits_to_ticks` is the Rust half of a separate bitset. `shadow_*` is the diff-against-a-copy option (copy before the run, compare after). `apply_cmds` is the Rust half of field writes as commands (three ops per written row). `changed_scan` is what any `Changed<T>` consumer pays to read the ticks, the same under every option. `hash_column` is the design's dev-only read-column guard with a serial FNV-style hash.

| pass | 10k/0.3% | 10k/10% | 10k/100% | 100k/0.3% | 100k/10% | 100k/100% | 1M/0.3% | 1M/10% | 1M/100% |
|---|---|---|---|---|---|---|---|---|---|
| mark_all | 0.0004 | 0.0006 | 0.0004 | 0.0038 | 0.0032 | 0.019 | 0.041 | 0.109 | 0.048 |
| bits_to_ticks | 0.0001 | 0.0006 | 0.0057 | 0.0008 | 0.0039 | 0.459 | 0.0083 | 0.363 | 0.636 |
| shadow_copy | 0.0050 | 0.0052 | 0.0064 | 0.050 | 0.084 | 0.229 | 0.448 | 1.86 | 0.651 |
| shadow_diff | 0.013 | 0.013 | 0.0099 | 0.124 | 0.529 | 0.374 | 0.622 | 3.67 | 1.01 |
| shadow_diff_pages | 0.012 | 0.0021 | 0.0009 | 0.229 | 0.174 | 0.036 | 0.718 | 1.71 | 0.135 |
| apply_cmds | 0.0001 | 0.0021 | 0.012 | 0.0004 | 0.105 | 0.667 | 0.0048 | 0.385 | 1.25 |
| changed_scan | 0.0018 | 0.0018 | 0.0012 | 0.016 | 0.079 | 0.039 | 0.118 | 0.180 | 0.134 |
| hash_column | 0.093 | 0.085 | 0.076 | 0.786 | 4.50 | 0.922 | 7.90 | 9.57 | 7.60 |

Milliseconds. The 10% column is the branch-unfriendly one (a compare that goes either way per row), which is why `shadow_diff` and `changed_scan` peak there; `shadow_copy` 1M/10% and `hash_column` 100k/10% also caught a background hiccup, so read the 0.3% and 100% columns for scale. The shadow option at 1M rows costs 1.1 to 5.5 ms per `write` column per run; `mark_all` costs 0.05 ms and is the cheapest pass by far, which is not where its cost is.

### Engines without a JIT, 100k rows, ms per run (median)

| path | V8 jitless, 0.3% / 10% / 100% | QuickJS-ng, 0.3% / 10% / 100% |
|---|---|---|
| raw typed arrays | 1.20 / 1.78 / 7.90 | 1.17 / 2.38 / 8.29 |
| raw + tick store | 0.87 / 1.78 / 8.91 | 1.16 / 2.08 / 8.93 |
| accessor | 2.50 / 4.75 / 22.5 | 3.66 / 6.82 / 36.7 |
| accessor + tick store | 2.62 / 4.93 / 28.7 | 4.06 / 8.44 / 50.5 |
| accessor, writes as commands (JS half only) | 3.32 / 6.61 / 40.3 | 4.39 / 12.2 / 79.6 |

Accessor forms (`accessor-forms.js`), 100k rows, every row written through three setters, ms per run:

| form | QuickJS-ng | V8 jitless | V8 JIT |
|---|---|---|---|
| class accessor (`this.f32[this.b]`) | 104.8 | 31.9 | 0.22 |
| class accessor + tick store | 131.6 | 37.8 | 0.24 |
| class accessor + dirty flag, tick once per row | 120.8 | 33.5 | 0.23 |
| closure accessor (module-scope position) | 78.2 | 43.4 | 25.1 |
| closure accessor + tick store | 79.0 | 53.6 | 25.9 |
| raw typed arrays | 9.5 | 11.0 | 0.16 |
| raw + tick store | 11.1 | 13.2 | 0.14 |

The 1M-row interpreter tables in `results/` are kept but not read from: boxed doubles make them GC-noise (identical work varies 3× between columns).

## Decision

Per-row ticks written by JS into Bevy's tick column, through the accessor setters by default and through `split(T)` by hand; `write(T)` keeps the `ContiguousMut` whole-table rule for raw loops that want it. Rust's part per run is to hand out one more view per `write` column, set summary ticks and fill `write(T)` tables.

What it beat:

- **A. Mark every row of each `write` column after the run.** Cheapest fill (0.05 ms per 1M rows) and the most expensive consequence: every `Changed<T>` consumer, Bevy's own included, runs over every row of every written table every frame; a watch stream streams every entity; `useComponent` wakes JS every frame. Unity's version of this rule is bounded by a 16 KB chunk; a Bevy table is not.
- **B'. A separate dirty bitset.** Same JS cost as the tick store plus a Rust scan per column per run and a remap on any row move; the tick array is already that bitset.
- **C. Field writes through the command buffer.** 2.5× the accessor loop in JS at 1M rows, the Rust apply on top, every value handled twice, and a system no longer reads its own writes. Stays for structural changes, which settles decision 2 as written.
- **D. Shadow copy diffed after the run.** A copy and a compare of every write column per run, whether anything changed or not: 1.1 to 5.5 ms per 1M-row column in Rust, against 0 to 0.4 ms for the tick store in JS. It is the only option that marks only real changes; an author who wants that compares before writing.
- **Per-page dirty bits, write intent per field.** Neither changes what Bevy records.

Side findings:

- The dev-only read-column hash (design risk "JS can write into a `read` column") with a serial hash costs about 8 ms per 1M rows per read column per system, twice (before and after the run); it needs a multi-lane or SIMD hash, or sampling, before dev builds run at that scale.
- The accessor form is per engine: closure-scoped on QuickJS-ng (25% faster, tick store free), class-based on V8 (the closure form is 100× slower under TurboFan). The `bevy-ts` runtime module should let a backend pick its accessor generator; the setter's tick store is the same in both.

## Changes made to `openspec/changes/bridge`

- `design.md`: decision 6 rewritten (per-row ticks through Bevy's tick column, `write`/`split` surfaces, per-system tick from `World::increment_change_tick`, what it beat); decision 2 no longer asks the operator to confirm and lists field-writes-as-commands as beaten; decision 5 names the setter's tick store and the two `q.tables` surfaces; the data-layout section adds the tick view per `write` column and the post-run summary-tick/`write(T)` fill; the threading paragraph says when ticks move; the coarse-ticks risk is replaced by the same-value-write risk and the `track_location` (`changed_by`) note; the decision 2/6 open question is removed.
- `specs/ts-systems/spec.md`: "Columns are zero-copy views" adds the tick view; "Iteration API" adds `read`/`write`/`split` and `ctx.tick` with a `split` scenario; "Change detection on written columns" becomes "Change detection on written rows" with per-system ticks, setter stores, summary ticks, `write(T)` fill, and scenarios for 3 of 10,000 rows, a quiet watch stream and `write(T)`.
- `specs/ts-components/spec.md`: generated setters store the run's tick.
- `tasks.md`: 1.4, 1.5, 3.4, 4.2, 4.3, 4.5 updated, 4.7 added (3-of-10,000 and quiet-watch test), 11.2 measures with and without the tick store.
- `proposal.md`: the systems bullet names per-row change ticks.

## Sources

- Bevy 0.20.0: `crates/bevy_ecs/src/storage/table/column.rs` (`changed_ticks`, `summary_tick`, `get_changed_ticks_slice`), `storage/table/mod.rs` (`get_changed_ticks_slice_for`), `query/filter.rs` (`Changed::filter_fetch`), `change_detection/params.rs` (`ContiguousMut::deref_mut`, `mark_all_as_changed`, `split`, `summary_tick_is_changed`), `change_detection/traits.rs` (`set_changed`, `set_if_neq`), `component/mod.rs` (summary tick docs), `bevy_transform/src/systems.rs`, `bevy_pbr/src/render/{light,mesh,skin}.rs`, `bevy_remote/src/builtin_methods.rs`
- Unity Entities 1.3 manual, EntityQuery filters: https://docs.unity3d.com/Packages/com.unity.entities@1.3/manual/systems-entityquery-filters.html ; `ArchetypeChunk.DidChange`: https://docs.unity3d.com/Packages/com.unity.entities@1.3/api/Unity.Entities.ArchetypeChunk.DidChange.html ; a representative false-positive thread: https://discussions.unity.com/t/what-caused-the-changed-status-of-a-chunk-to-be-reset/764751
- flecs Queries manual, Change Detection: https://www.flecs.dev/flecs/Queries.html ; `ecs_modified_id` in `include/flecs.h`; flecs-polyglot: https://github.com/flecs-hub/flecs-polyglot
- V8 `src/objects/js-array-buffer.h` (`kMaxSizeInHeap`): https://chromium.googlesource.com/v8/v8/+/refs/heads/main/src/objects/js-array-buffer.h ; optimized code read with `node --no-maglev --print-opt-code --code-comments`
- bevy-ts: `docs/research/js-runtime.md`, `openspec/changes/bridge/design.md`, `openspec/changes/tsx/design.md` (decision 9)

## In the engine: the cube demo's `bob` (2026-10-20)

Measured after the bridge landed, release build, headless (`MinimalPlugins`), same Mac.
`crates/bevy-ts-cli/examples/bob_bench.rs`, raw output in
`bench/change-detection/results/bob-frame.md`. Every row's `translation.y` is written each
frame; the TS run includes building the column views, the one call into JS and the post-run
summary ticks. Medians over 300 frames, microseconds.

| entities | accessor + tick (default) | raw arrays, no tick | raw + tick | whole `app.update()`, default |
|---|---|---|---|---|
| 1 | 4 | 3 | 3 | 54 |
| 1,000 | 20 | 11 | 12 | 59 |
| 100,000 | 667 | 556 | 575 | 709 |

The tick store costs 3% on raw arrays at 100k rows (575 against 556 µs), in line with the
write-loop numbers above; the accessor form costs 20% over raw at 100k, mostly the flyweight's
property dispatch on the 3-lane vector sub-object, not the tick. A fixed cost of about 3 µs per
run covers the call into V8 and the view setup. At 100k entities `bob` takes about 0.7 ms of a
16.7 ms frame.
