## 1. Before: measure today's cost

- [x] 1.1 Agent town's villager scene (the `villager-anim` bench, research.md §3), 100 villagers, three arms: playing, `speed` 0 (today's pause), all hidden. CPU % and frame time, 8 runs of 300 frames, headless 960x540 or a tart VM, niced. Record in research.md
- [x] 1.2 The fox bench (`animation_bench_engine.rs`) at 400 foxes, arms `play`, `nograph`, `hidden`, for the after numbers to compare with. Every cargo step through `~/dev/abyme/scripts/gate-slot`

## 2. `PlayAnimation` (`crates/bevy-ts-animation`)

- [x] 2.1 `repeat: RepeatAnimation` (default `Forever`), `paused: bool`, `cull: bool`; `apply` passes `repeat` to `set_repeat`
- [x] 2.2 `start` honours `paused`: plays, then pauses and parks
- [x] 2.3 `AnimationTargets` gains `meshes`; `pub fn meshes(world, root)` beside `players`, filled in `start`
- [x] 2.4 The park system (design 2): removes or inserts `AnimationGraphHandle`, `pause_all`/`resume_all` on `paused`; `stop_removed` gives the handle back
- [x] 2.5 `tests/play.rs`: `repeat_count_finishes`; the existing tests pass with the enum
- [x] 2.6 `tests/park.rs`: `paused_parks_and_resumes_where_it_stood` (no handle, bones unchanged across frames, seek time kept), `speed_zero_does_not_park`, `culled_roots_park_off_screen` (with a camera turned away, then back; and `cull: false` keeps playing), `paused_while_culled_stays_parked`, `spawned_paused`, `removing_a_parked_root_restores_the_handle`

## 3. `Animations` status (`crates/bevy-ts-animation`, `crates/bevy-ts-cli`)

- [x] 3.1 `Animations` param with `#[bevy_ts::export(ts = "Animations", module = "bevy-ts/animation")]`, `status(root) -> Option<AnimationStatus>` (a `NativeLayout` record)
- [x] 3.2 Register it in every engine where `Cameras` is (`dev`, `play`, a shipped game, `types`)
- [x] 3.3 `tests/animation.rs`: `animations_status`, a TS system reading a playing, a paused and a non-root entity

## 4. `Animations` writers (after writable-native-params lands)

- [x] 4.1 Rebase onto main once writable-native-params lands
- [x] 4.2 `seek(root, secs)` (`seek_to`), `rewind(root)`, `replay(root)` on every player of the root; false when there is no status
- [x] 4.3 `tests/animation.rs`: `animations_seek_rewind_replay`, including the start-offset scenario and a seek on a parked root

## 5. Types, docs, examples

- [x] 5.1 `scripts/gen-bevy-types.sh`: `bevy-ts/bevy` names the new fields and `RepeatAnimation`; `bevy-ts/animation` declared; `packages/bevy-ts/test/tsx/elements.tsx` covers `repeat`, `paused`, `cull`
- [x] 5.2 `docs/animation.md`: the new fields, parking and its stale wake frame, frozen time, speed 0 versus `paused`, `Animations`, the coverage table, the new bench numbers; `docs/levels.md` and README spell `repeat` the new way
- [x] 5.3 `examples/foxes`: a pause button and `cull` on the grid

## 6. After: measure

- [x] 6.1 Fox bench arms `paused` (TSX foxes with `paused: true`) and `culled` (`cull: true`, camera turned away), against 1.2's `play` and `nograph`, 3 runs per arm interleaved
- [x] 6.2 Agent town with the local engine: 100 villagers playing, `paused: true`, and hidden with `cull: true`; CPU % and frame time against 1.1. Paused and culled should land near the 0-villager arm plus skin upload
- [x] 6.3 Numbers in `docs/animation.md`'s Cost section

## 7. Gate

- [ ] 7.1 Build, the crates' tests, clippy, `scripts/gen-bevy-types.sh --check`, through gate-slot; `openspec validate animation-control --strict`

## 8. Stage 2: AnimationGraph from TS

- [x] 8.1 `PlayAnimation.graph: Handle<AnimationGraph>` (default none); `graph_of(play, shared)` gives a root's graph handle, and `start`, `park` and `stop_removed` call it in place of `shared.handle`
- [x] 8.2 A graph root plays every clip node with `repeat` and `speed`; `clip` beside `graph` warns once (`AnimationReports`) and is unused; a changed `graph` restarts; on `AssetEvent::Modified` of a used graph, start new clip nodes and stop gone ones; clearing `graph` returns to `clip`
- [x] 8.3 `Animations` on a graph root: `status` reads the first clip node in node order; `seek`, `rewind`, `replay` act on every playing clip (with task 4)
- [x] 8.4 `tests/graph.rs`: `a_ron_graph_plays` (a `.animgraph.ron` fixture under `assets/`), `a_graph_blends_two_clips`, `an_additive_blend_adds`, `a_mask_keeps_a_bone_still`, `a_changed_graph_restarts`, `an_edited_graph_plays_new_clips`, `graph_and_clip_warns`, `a_parked_graph_root_wakes_with_its_graph`
- [x] 8.5 Rebase onto main once reflect-assets stage 1 and the reflected-coverage hole check land
- [x] 8.6 The `AnimationGraph` hand-map in `bevy_ts::reflect_asset`'s list (design 12), agreed with lead@reflect-assets: write from `SerializedAnimationGraph` JSON with clips loaded by path and `mask_groups` as `[target, mask]` pairs (UUID or `{ names }`, through `from_names`); read through `TryFrom<AnimationGraph>`, naming the node of a pathless clip; `bevy-ts-animation/src/hand.rs::animation_graph_round_trips` (moved there onto `app.ts_hand_map` once reflect-assets replaced the list)
- [x] 8.7 Delete the `reflect_value::UNCOVERED` entry for `AnimationGraph`'s `graph`; the hole check passes with no entry for it
- [x] 8.8 `packages/bevy-ts`: the `AnimationGraph` builder (design 10) and `AnimationTargetId.fromNames`/`fromName`, emitted into `bevy-ts/bevy` by `scripts/gen-bevy-types.sh` in place of the generated class; `test/vitest/animation-graph.test.ts` (each `add*`'s node number and JSON, `fromClip`, `fromClips`, `addEdge`, `removeEdge`, `get(node)` writes, mask methods); `test/tsx/elements.tsx` covers `graph`
- [x] 8.9 `crates/bevy-ts-cli/tests/animation.rs`: `a_ts_built_graph_blends` (a TSX fox with `graph={g}`) and `a_ts_graph_reweights_live` (`assets.set` of a weight from a TS system)
- [x] 8.10 `docs/animation.md`: building a graph, masks and target paths, a graph file, the graph rows of the coverage table; "When to write Rust instead" drops blends and masks; `examples/foxes` gains a blended fox
- [x] 8.11 Gate as 7.1, plus `pnpm -C packages/bevy-ts test`; `openspec validate animation-control --strict`

## 9. Stage 2: AnimationClip from TS, on any entity

- [x] 9.1 Plain roots (design 19): `PlayAnimation` with no `WorldAssetRoot` starts in `apply_changed`; `start` inserts `AnimationPlayer` on the root when `players` finds none and there is no scene; target ids from name paths below the root (root: empty path) and `AnimatedBy(root)` on the root and named descendants without an id
- [x] 9.2 `ScriptCurve<A>` (design 17) for every constructor and adaptor in design 16, over every `Animatable` value type, delegating to Bevy's curves
- [x] 9.3 `ReflectedField<A>` (design 18): `AnimatableProperty` by type path and top-level field, `get_mut_by_id` + `ReflectFromPtr`, `ComponentField` evaluator id as `animated_field!` hashes it
- [x] 9.4 The `AnimationClip` hand-map entry on `&Slots` (design 20), agreed with build@reflect-assets: write with `add_curve_to_target`, check mode validates only, errors name the curve; read gives the written JSON, `curves: null` for a file clip; `bevy-ts-animation/src/hand.rs::animation_clip_round_trips`
- [x] 9.5 `bevy-ts-material`: `alpha::changed` `.after(AnimationSystems)`: material-fade's, in its change (never edit alpha.rs here); the alpha tween test waits on it landing
- [x] 9.6 `tests/it/clip.rs`: `a_tween_translates_with_an_ease`, `a_tween_rotates`, `scale_ping_pongs`, `a_fade_draws_the_frame_it_samples`, `a_named_child_moves_alone`, `a_wrong_field_fails`, `a_paused_tween_parks`, `a_script_rotation_blends_with_a_gltf_clip`, `every_ease_samples_as_bevy`, `every_curve_samples_as_bevy` (each against Bevy's own curve sampled at the same times)
- [x] 9.7 `packages/bevy-ts`: `AnimationClip`, `AnimatableCurve`, `animatedField` (typed so an unknown field or non-animatable type fails `tsc`), every curve class and adaptor, `EaseFunction`, `interval`, `AnimationTargetId.root`, emitted into `bevy-ts/bevy`; `test/vitest/animation-clip.test.ts` (JSON of each, the type errors)
- [x] 9.8 `crates/bevy-ts-cli/tests/animation.rs`: `a_tsx_tween_moves_with_no_ts_per_frame` (a TSX entity tweens; the TS system run count does not grow with frames)
- [x] 9.9 Bench: 400 cubes tweening `translation`, a clip arm against a TS-system arm writing `Transform`; numbers in `docs/animation.md`
- [x] 9.10 `docs/animation.md`: tweens (the four cases), curves and eases, targets on plain entities, fades through `MaterialAlpha`, the clip rows of the coverage table, why clip events stay Rust; `examples/foxes` gains a tweened sign
- [ ] 9.11 Delete any `reflect_value::UNCOVERED` entry for `AnimationClip`; gate as 8.11

