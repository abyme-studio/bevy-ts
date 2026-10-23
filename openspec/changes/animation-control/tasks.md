## 1. Before: measure today's cost

- [ ] 1.1 Agent town's villager scene (the `villager-anim` bench, research.md §3), 100 villagers, three arms: playing, `speed` 0 (today's pause), all hidden. CPU % and frame time, 8 runs of 300 frames, headless 960x540 or a tart VM, niced. Record in research.md
- [ ] 1.2 The fox bench (`animation_bench_engine.rs`) at 400 foxes, arms `play`, `nograph`, `hidden`, for the after numbers to compare with. Every cargo step through `~/dev/abyme/scripts/gate-slot`

## 2. `PlayAnimation` (`crates/bevy-ts-animation`)

- [ ] 2.1 `repeat: RepeatAnimation` (default `Forever`), `paused: bool`, `cull: bool`; `apply` passes `repeat` to `set_repeat`
- [ ] 2.2 `start` honours `paused`: plays, then pauses and parks
- [ ] 2.3 `AnimationTargets` gains `meshes`; `pub fn meshes(world, root)` beside `players`, filled in `start`
- [ ] 2.4 The park system (design 2): removes or inserts `AnimationGraphHandle`, `pause_all`/`resume_all` on `paused`; `stop_removed` gives the handle back
- [ ] 2.5 `tests/play.rs`: `repeat_count_finishes`; the existing tests pass with the enum
- [ ] 2.6 `tests/park.rs`: `paused_parks_and_resumes_where_it_stood` (no handle, bones unchanged across frames, seek time kept), `speed_zero_does_not_park`, `culled_roots_park_off_screen` (with a camera turned away, then back; and `cull: false` keeps playing), `paused_while_culled_stays_parked`, `spawned_paused`, `removing_a_parked_root_restores_the_handle`

## 3. `Animations` status (`crates/bevy-ts-animation`, `crates/bevy-ts-cli`)

- [ ] 3.1 `Animations` param with `#[bevy_ts::export(ts = "Animations", module = "bevy-ts/animation")]`, `status(root) -> Option<AnimationStatus>` (a `NativeLayout` record)
- [ ] 3.2 Register it in every engine where `Cameras` is (`dev`, `play`, a shipped game, `types`)
- [ ] 3.3 `tests/animation.rs`: `animations_status`, a TS system reading a playing, a paused and a non-root entity

## 4. `Animations` writers (after writable-native-params lands)

- [ ] 4.1 Rebase onto main once writable-native-params lands
- [ ] 4.2 `seek(root, secs)` (`seek_to`), `rewind(root)`, `replay(root)` on every player of the root; false when there is no status
- [ ] 4.3 `tests/animation.rs`: `animations_seek_rewind_replay`, including the start-offset scenario and a seek on a parked root

## 5. Types, docs, examples

- [ ] 5.1 `scripts/gen-bevy-types.sh`: `bevy-ts/bevy` names the new fields and `RepeatAnimation`; `bevy-ts/animation` declared; `packages/bevy-ts/test/tsx/elements.tsx` covers `repeat`, `paused`, `cull`
- [ ] 5.2 `docs/animation.md`: the new fields, parking and its stale wake frame, frozen time, speed 0 versus `paused`, `Animations`, the coverage table, the new bench numbers; `docs/levels.md` and README spell `repeat` the new way
- [ ] 5.3 `examples/foxes`: a pause button and `cull` on the grid

## 6. After: measure

- [ ] 6.1 Fox bench arms `paused` (TSX foxes with `paused: true`) and `culled` (`cull: true`, camera turned away), against 1.2's `play` and `nograph`, 3 runs per arm interleaved
- [ ] 6.2 Agent town with the local engine: 100 villagers playing, `paused: true`, and hidden with `cull: true`; CPU % and frame time against 1.1. Paused and culled should land near the 0-villager arm plus skin upload
- [ ] 6.3 Numbers in `docs/animation.md`'s Cost section

## 7. Gate

- [ ] 7.1 Build, the crates' tests, clippy, `scripts/gen-bevy-types.sh --check`, through gate-slot; `openspec validate animation-control --strict`

## 8. Stage 2: AnimationGraph from TS

- [ ] 8.1 `PlayAnimation.graph: Handle<AnimationGraph>` (default none); `graph_of(play, shared)` gives a root's graph handle, and `start`, `park` and `stop_removed` call it in place of `shared.handle`
- [ ] 8.2 A graph root plays every clip node with `repeat` and `speed`; `clip` beside `graph` warns once (`AnimationReports`) and is unused; a changed `graph` restarts; on `AssetEvent::Modified` of a used graph, start new clip nodes and stop gone ones; clearing `graph` returns to `clip`
- [ ] 8.3 `Animations` on a graph root: `status` reads the first clip node in node order; `seek`, `rewind`, `replay` act on every playing clip (with task 4)
- [ ] 8.4 `tests/graph.rs`: `a_ron_graph_plays` (a `.animgraph.ron` fixture under `assets/`), `a_graph_blends_two_clips`, `an_additive_blend_adds`, `a_mask_keeps_a_bone_still`, `a_changed_graph_restarts`, `an_edited_graph_plays_new_clips`, `graph_and_clip_warns`, `a_parked_graph_root_wakes_with_its_graph`
- [ ] 8.5 Rebase onto main once reflect-assets stage 1 and the reflected-coverage hole check land
- [ ] 8.6 The `AnimationGraph` hand-map in `bevy_ts::reflect_asset`'s list (design 12), agreed with lead@reflect-assets: write from `SerializedAnimationGraph` JSON with clips loaded by path and `mask_groups` as `[target, mask]` pairs (UUID or `{ names }`, through `from_names`); read through `TryFrom<AnimationGraph>`, naming the node of a pathless clip; `reflect_asset.rs::animation_graph_round_trips`
- [ ] 8.7 Delete the `reflect_value::UNCOVERED` entry for `AnimationGraph`'s `graph`; the hole check passes with no entry for it
- [ ] 8.8 `packages/bevy-ts`: the `AnimationGraph` builder (design 10) and `AnimationTargetId.fromNames`/`fromName`, emitted into `bevy-ts/bevy` by `scripts/gen-bevy-types.sh` in place of the generated class; `test/animation-graph.test.ts` (each `add*`'s node number and JSON, `fromClip`, `fromClips`, `addEdge`, `removeEdge`, `get(node)` writes, mask methods); `test/tsx/elements.tsx` covers `graph`
- [ ] 8.9 `crates/bevy-ts-cli/tests/animation.rs`: `a_ts_built_graph_blends` (a TSX fox with `graph={g}`) and `a_ts_graph_reweights_live` (`assets.set` of a weight from a TS system)
- [ ] 8.10 `docs/animation.md`: building a graph, masks and target paths, a graph file, the graph rows of the coverage table; "When to write Rust instead" drops blends and masks; `examples/foxes` gains a blended fox
- [ ] 8.11 Gate as 7.1, plus `pnpm -C packages/bevy-ts test`; `openspec validate animation-control --strict`
