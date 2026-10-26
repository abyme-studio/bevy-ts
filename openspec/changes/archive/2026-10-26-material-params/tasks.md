## 0. Gates

- [x] 0.1 Stage 1 is buildable now (lead@engine-core's go)
- [x] 0.2 Stage 2 waits for reflect-assets to land `app.ts_hand_map::<T: Asset + TypePath>(read, write)` (after its stage 1, before its stage 2); rebase on main once it has
- [x] 0.3 Stage 3 waits for stage 2's 60 Hz bench (4.4), its numbers in stage 2's landing note, and the operator's go on its ask-first items (proposal, Impact)

## 1. Stage 1: sweep unused keyed instances (buildable now)

- [x] 1.1 `crates/bevy-ts-material/src/kinds.rs`: `sweep(world)` drops each `KindState::instances` entry whose `Handle::Strong` Arc has a strong count of 1
- [x] 1.2 `crates/bevy-ts-material/src/lib.rs`: run `sweep` in `PostUpdate` after the TSX reconcile (order against `bevy-ts-tsx`'s reconcile set)
- [x] 1.3 `crates/bevy-ts-cli/tests/it/materials.rs`: state-driven `pulse` changing every frame for 120 frames keeps the kind's instances and `Assets<ScriptMaterial>` at most 2; one of two sharers removed keeps the other's handle
- [x] 1.4 `docs/materials.md`: "Equal props share one material instance", plus unused ones are freed and keyed instances are never written in place
- [x] 1.5 `openspec/changes/material-params/specs/script-materials/spec.md` keyed-instance requirement holds; land stage 1

## 2. Stage 2: owned instances written in place (after reflect-assets' `ts_hand_map`)

- [x] 2.1 `crates/bevy-ts-material/src/owned.rs` (new): `read` (`{ kind, ...params, ...textures }`, params decoded by the kind's `ParamsLayout`) and `write` (whole `ScriptMaterial`; `kind` required, unchanged on set; unknown names fail with `kinds.rs`' message); register with `app.ts_hand_map::<ScriptMaterial>` in `src/lib.rs`
- [x] 2.2 `crates/bevy-ts-material/src/layout.rs`: decode params from bytes (the encoder's inverse), with unit tests round-tripping every type
- [x] 2.3 `crates/bevy-ts-material/src/kinds.rs`: a schema change re-encodes every `ScriptMaterial` of the kind in `Assets<ScriptMaterial>`, owned ones included
- [x] 2.4 `packages/bevy-ts/runtime/material.js`, `packages/bevy-ts/material.d.ts`: `Kind.add(props): AssetRef` through `assets.add` with `ScriptMaterial`'s asset class and `{ kind, ...props }`; `asset` refused as a param or texture name
- [x] 2.5 `crates/bevy-ts-tsx/src/resolve.rs`, `src/intrinsics.rs`, `packages/bevy-ts/jsx-runtime.d.ts`: `<material asset={ref} gltfMaterial? />` looks the ref up in reflect-assets' ref table; other props, another type or a stale ref fail at the element
- [x] 2.6 `crates/bevy-ts-cli/tests/it/materials.rs`: get after set; an equal set sends no `Modified`; another type and a stale ref fail at the element; a schema change re-encodes an owned instance
- [x] 2.7 `crates/bevy-ts-cli/tests/it/material_params_render.rs` (new, `mod` in `main.rs`, `common::render_app()`): same `AssetId` and a changed pixel colour after `assets.set`; render `PipelineCache` pipeline count flat over 60 writes
- [x] 2.8 `docs/materials.md`: owned instances (`Kind.add`, `assets.get`/`set`, `<material asset>`), what a write costs, and the coverage table (design decision 5); link from `docs/assets.md`

## 3. Stage 3: per-entity `instance` params (after the stage 2 bench)

- [x] 3.1 `packages/bevy-ts/runtime/material.js`, `material.d.ts`: `instance` schema, name rules, 256-byte limit, typed instance props
- [x] 3.2 `crates/bevy-ts-material/src/layout.rs`, `src/wgsl.rs`: `<Name>Instance` struct, storage binding after textures, `fn instance(i)`, appended; reserved names refused; tests on the generated WGSL
- [x] 3.3 `crates/bevy-ts-material/src/instance.rs` (new): `ScriptInstance([u32; 64])` packed by bevy-ts (rows written with the component, dropped by its remove hook, uploaded in `Last`; Bevy's `GpuComponentArrayBuffer` cannot be ordered); encode from props by the instance layout; inserted only for kinds with `instance`; a user `MeshTag` kept, no `ScriptInstance`, an error naming entity and element
- [x] 3.4 `crates/bevy-ts-material/src/render.rs`: the kind's layout binds the array's buffer (`UnpreparedBindingResource::ShaderBuffer`)
- [x] 3.5 `crates/bevy-ts-tsx/src/resolve.rs`, `src/intrinsics.rs`: instance props split from the key; bound instance props allowed and written to `ScriptInstance`; bound params still fail with today's message
- [x] 3.6 `crates/bevy-ts-material/src/scene.rs`: copy the root's `ScriptInstance` to swapped meshes, follow changes, remove on restore
- [x] 3.7 `crates/bevy-ts-cli/tests/it/material_params_render.rs`: two spheres one asset two colours; a bound hover changes pixels with no new asset; a user `MeshTag` kept and reported; a glTF root's meshes take its values; a kind without `instance` adds no `MeshTag` (the user `MeshTag` test is in `materials.rs`, the glTF root's in `scene_material.rs`, where their setups live)
- [x] 3.8 `docs/materials.md`: instance params, their WGSL, limits (one `MeshTag`, 256 bytes, no prepass reads); coverage table row

## 4. Benches (`bench/material-params/`, outside the workspace)

- [x] 4.1 Bench as `bench/material-params/rust`, release with Bevy static; `results/README.md` with the command
- [x] 4.2 Stage 2: N owned instances (1, 10, 100) written at 60 Hz with `assets.set` vs the same values re-keyed through props: frame time, asset count, bind groups made
- [x] 4.3 Stage 3: 10k entities with an animated instance param vs re-keying: frame time, buffer bytes
- [x] 4.4 Run 4.2 before stage 3 starts; its numbers go in stage 2's landing note

## 5. Landing (each stage)

- [x] 5.1 Cargo for touched crates through `~/dev/abyme/scripts/gate-slot` while working; workspace build, tests and clippy once before landing; `pnpm check`
- [x] 5.2 `openspec validate material-params --strict`
