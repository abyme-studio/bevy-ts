## Why

A few leaf pixels in agent town's civ shots change between runs at the same load. Two foliage
meshes cover one pixel at the same depth; with `depth_compare: GreaterEqual` the one drawn last
wins. Bins draw in key order, and every part of a bin key is an allocation id (pipeline id,
material slot, mesh `AssetId`) that follows which glTF finished loading first. `StableBinOrder`
sorts only the entities inside one bin, so it does not reach this. A repro (research doc
`leaf-flips.md`) gave 2 distinct shots in 10 runs of one scene.

## What Changes

- Under `FixedStep` only, bevy-ts-remote gives each entity with an `Aabb` a stable 64-bit key: a
  `FixedHasher` hash of its world-space AABB centre, its half extents and its asset path, when it
  has one. The key is the same on every run; allocation ids and entity ids are not.
- `StableBinOrder` becomes `StableBinOrder(MainEntityHashMap<u64>)`, extracted from those keys.
- The vendored `sort_binned_render_phase`, while `StableBinOrder` is there, sorts each bin's
  entities by key, then `MainEntity`, and sorts `batchable_meshes` and `unbatchable_meshes` by each
  bin's smallest key, then the bin's own key. That covers `Opaque3d`, `AlphaMask3d` and their
  prepasses.
- The vendored `sort_phase_system`, while it is there, orders a sorted phase's items by key
  before `SortedPhaseItem::sort`, so `Transparent2d` and `Transparent3d` items at one depth (sprites
  at one z) keep that order through Bevy's stable sort.
- `vendor/bevy_render/PATCH.md`, `docs/build.md` updated.

## Capabilities

### Modified Capabilities

- `headless-run`: a fixed-step shot draws items at one depth in an order that does not follow
  asset load order or entity ids.

## Impact

- `vendor/bevy_render/src/{render_phase/mod.rs,batching/mod.rs}`, `vendor/bevy_render/PATCH.md`.
- `crates/bevy-ts-remote/src/screenshot.rs` (the key and its extraction).
- `crates/bevy-ts-cli/tests/it/screenshot_ready.rs` (tests), `docs/build.md`.
- Ask-first: performance. The new code runs only while `FixedStep` is in the main world (shots,
  `BEVY_TS_FIXED_STEP=1` runs), so normal play pays nothing. Inside a shot it adds a hash per moved
  entity and a sort per phase per frame. Unmeasured: task 5 measures it before landing.
- No new dependency: `FixedHasher` is `bevy_platform`'s.
