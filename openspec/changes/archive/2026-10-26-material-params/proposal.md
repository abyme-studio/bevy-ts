## Why

A `<material kind>` instance is keyed by its props, and `KindState::instances` keeps a strong
handle to every one made, so new props leak an asset each time (`research.md` §3). Nothing can
change a script material without making a new one: no owned asset to write, and no value per
entity, so a hover on one of a hundred spheres costs a new asset and splits the batch.

## What Changes

Three stages, built in this order. Each lands with its docs, spec and tests.

1. **Sweep.** Each `PostUpdate`, after the TSX reconcile, a keyed instance that only the cache
   holds is dropped, so Bevy frees the asset and its binding. Buildable now.
2. **Owned instances, written in place.** `Glow.add(props)` makes a `ScriptMaterial` asset outside
   the keyed cache and returns reflect-assets' `AssetRef`; `assets.get` and `assets.set` read and
   write it. `bevy-ts-material` registers `ScriptMaterial`'s read and write with reflect-assets'
   `app.ts_hand_map::<ScriptMaterial>(read, write)`. A write keeps the `AssetId`; every mesh
   holding it draws the new values. `<material asset={ref} />` puts one on an entity. Keyed
   instances stay immutable from TSX. Builds after reflect-assets lands `ts_hand_map`.
3. **Per-entity `instance` params.** `material(name, { instance: { hover: f32 }, ... })` declares
   values that live on the entity, not the asset: one `ScriptInstance` component, packed by Bevy
   0.20's `GpuComponentArrayBuffer` into one storage buffer indexed by `MeshTag`. Instance props
   may be bound (`hover={() => h()}`). Opt-in: only entities of a kind with instance params get a
   `ScriptInstance` and a `MeshTag`. An entity that already has a `MeshTag` of its own gets an
   error, and its tag is never overwritten. Builds after stage 2's 60 Hz bench.

`material()` refuses a param, texture or instance param named `asset`, which `<material>` takes
itself. **BREAKING** for a kind that declares one; none in this repo does.

## Capabilities

### New Capabilities

None.

### Modified Capabilities

- `script-materials`: keyed instances are freed when unused and never written in place; owned
  instances (`Kind.add`, `assets.set`, `<material asset>`); per-entity `instance` params; `asset`
  reserved.

## Impact

- Code:
  - Stage 1: `crates/bevy-ts-material/src/kinds.rs` (the sweep), `src/lib.rs` (its system).
  - Stage 2: `crates/bevy-ts-material/src/owned.rs` (new: hand map read and write),
    `src/kinds.rs` (re-encode reaches owned instances), `src/lib.rs`;
    `packages/bevy-ts/runtime/material.js`, `packages/bevy-ts/material.d.ts` (`Kind.add`);
    `crates/bevy-ts-tsx/src/resolve.rs`, `src/intrinsics.rs` (`<material asset>`);
    `packages/bevy-ts/jsx-runtime.d.ts`.
  - Stage 3: `crates/bevy-ts-material/src/instance.rs` (new), `src/layout.rs` (instance struct and
    binding), `src/render.rs` (storage binding), `src/scene.rs` (copy to swapped meshes),
    `src/kinds.rs`; `material.js`, `material.d.ts`; `crates/bevy-ts-tsx/src/resolve.rs` (bound
    instance props).
- Docs: `docs/materials.md` (sharing, owned instances, instance params, a coverage table).
- Tests: `crates/bevy-ts-cli/tests/it/materials.rs`, a new
  `crates/bevy-ts-cli/tests/it/material_params_render.rs`; benches in `bench/material-params/`.
- Depends on: reflect-assets' `app.ts_hand_map::<T>(read, write)` and its `assets` surface. No
  edit in reflect-assets' module from this change.
- Ask-first, needs the operator's go:
  - Stage 2, performance: each frame a owned instance is written, its bind group and uniform
    buffer are rebuilt (Bevy's non-bindless path has no in-place update) and every entity drawing
    it re-specialises from the pipeline cache. Fine for a few materials a frame, not hundreds.
  - Stage 3, new architectural pattern: a storage buffer in a script material's bind group and
    bevy-ts owning `MeshTag` on every entity of a kind with instance params.
  - Stage 3, performance: 256 bytes per tagged entity (2.5 MB at 10k); a frame where any value
    changes clones the whole buffer to the render world and uploads it whole.
