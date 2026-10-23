## Why

A hundred villagers cost the same CPU whether they play, sit at speed 0 or stand off screen (research.md §3: 100 % against 56 % with none). Bevy's pause and speed 0 still sample and write every bone; only removing a player's `AnimationGraphHandle` (or stopping it) skips the work, and Bevy has no visibility culling. `PlayAnimation` also offers TS only `speed`, a bool `repeat` and a clip switch: no pause, no seek, no count, nothing to read back.

## What Changes

- **`PlayAnimation.paused`** (default `false`). True pauses the root's players (`pause_all`) and parks them: Rust removes each player's `AnimationGraphHandle` and keeps the `AnimationPlayer`, so Bevy samples and writes nothing. False puts the handle back and resumes where the clip stood.
- **`PlayAnimation.cull`** (default `false`, so nothing changes unless asked). True parks the root while none of its meshes is in any view (`ViewVisibility`), and wakes it the frame one is. Per root. A parked clip's time stands still.
- **BREAKING: `PlayAnimation.repeat` becomes Bevy's `RepeatAnimation`** (`Never`, `Count(n)`, `Forever`; default `Forever`, today's default). TSX `repeat={true}` becomes `repeat="Forever"` (or is dropped), `repeat={false}` becomes `"Never"`; a level writes `repeat: Count(3)`. `repeat` stops being a TS column (an enum with data is a hole) until reflect-coverage stage 3.
- **`speed` 0 stays a plain write**, Bevy's meaning: it holds the pose and still costs a playing clip. Docs say to use `paused`.
- **`Animations`, an exported param in `bevy-ts/animation`.** Keyed by the root, which names its clip by asset path. `status(root)` reads the root's clip: loaded, duration, seek time, elapsed, completions, just completed, finished, paused, parked, weight. `seek(root, secs)` (Bevy's `seek_to`), `rewind(root)` and `replay(root)` write; they need writable-native-params. No node index ever reaches TS: `SharedGraph`'s clip-to-node map stays in Rust.
- **Coverage table** in the spec: every public item of `AnimationPlayer`, `ActiveAnimation` and `AnimationTransitions`, its TS name or why it stays Rust, and the test.
- **One walk**: `players`/`players_in` stay the one player walk; parking and `Animations` use them.
- `docs/animation.md` and the fox bench gain the paused and culled arms; agent town's 100 villagers are measured playing, paused and hidden, before and after.

## Capabilities

### New Capabilities

None.

### Modified Capabilities

- `scene-animation`: `PlayAnimation` gains `paused` and `cull` and an enum `repeat`; parking; the `Animations` param; the coverage table.

## Impact

- `crates/bevy-ts-animation`: `PlayAnimation` fields, a park system, mesh records on `AnimationTargets`, the `Animations` export.
- `crates/bevy-ts-cli`: registers the export in every engine (as `Cameras`); `tests/animation.rs`; `examples/animation_bench_engine.rs` arms.
- `packages/bevy-ts`: `bevy-ts/bevy` regenerated (`PlayAnimation`, `RepeatAnimation`), `bevy-ts/animation` declared; `test/tsx/elements.tsx`.
- `examples/foxes`, `docs/animation.md`, `docs/levels.md` (the `repeat` spelling), README.
- Agent town: `src/world.tsx` passes `repeat={true}`, which the new type refuses; it migrates when it takes the engine release.
- Depends on writable-native-params (still landing) for `seek`, `rewind`, `replay`. `status` and every `PlayAnimation` field work without it.
- Performance: defaults unchanged. `paused: true` now costs nothing per frame instead of a playing clip's cost.
