## 1. `<material alpha alphaMode>` (`crates/bevy-ts-tsx`, `crates/bevy-ts-material`, `packages/bevy-ts`)

- [x] 1.1 `kinds.rs`: make the `AlphaDecl` parser public so `<material>` uses the same names
- [x] 1.2 `intrinsics.rs`: `MaterialProps.alpha` (checked 0..1) and `alphaMode`; `material()` sets `base_color` alpha and the mode, defaulting to `Blend` below 1 as `StandardMaterial::from(Color)`
- [x] 1.3 `jsx-runtime.d.ts`: `MaterialProps.alpha`, `alphaMode` (the `AlphaMode` type from `bevy-ts/material`)

## 2. `MaterialAlpha` (`crates/bevy-ts-material`)

- [x] 2.1 Rebase onto main once scene-material lands (build-scene-material says when)
- [x] 2.2 `alpha.rs`: `MaterialAlpha { alpha: f32 }` (reflected, layout tier), `MaterialAlphaTargets`, `sync(world, entity)`: targets are the entity's own `MeshMaterial3d<StandardMaterial>` and, on a `WorldAssetRoot`, its scene meshes; copy each distinct original once, point targets at copies, rewrite copies on change only when they differ
- [x] 2.3 Triggers: a `WorldInstanceReady` observer that records the root; a `PostUpdate` sync system `.after(SceneMaterialSet)` for recorded roots, `Changed<MaterialAlpha>`, `Changed<MeshMaterial3d<StandardMaterial>>` on a target holding a handle that is not its copy, a target's material removed, `AssetEvent::Modified` of an original, removal (restores targets, fixes copies in `SceneMaterialTargets`); registered by `ScriptMaterialRenderPlugin`
- [x] 2.4 `scripts/gen-bevy-types.sh`: `bevy-ts/bevy` names `MaterialAlpha`

## 3. Tests and docs

- [ ] 3.1 Headless render test (offscreen image, as `bevy-ts-material/tests/spike.rs`), over a white backdrop: a red cube with `<material>` alpha 0.5 reads pink; a red cube with `MaterialAlpha(0.5)` reads pink, and a second red cube sharing its material with no `MaterialAlpha` reads red; `crate.gltf` under a root with `MaterialAlpha(0.5)` reads blended, and a second crate with no `MaterialAlpha` beside it reads as before
- [ ] 3.2 `bevy-ts-material/tests/alpha.rs`, headless: on a plain mesh and on a scene root, one copy per distinct original, originals untouched, a changed alpha adds no asset, removal restores; a plain mesh whose material handle is replaced is faded again; a respawned scene is faded again; a TSX child untouched; a script-material mesh untouched and restored correctly with both removed in either order
- [ ] 3.3 `bevy-ts-cli/tests/materials.rs`: TSX end to end, `<MaterialAlpha alpha={() => k()} />` follows a signal frame by frame on a `<mesh>` with `<material>` and on a `WorldAssetRoot`; a TS save that changes the shape's `<material color>` keeps it faded; `<material alpha={2}>` fails at the element
- [x] 3.4 `docs/materials.md`: alpha, alphaMode, `MaterialAlpha` on a shape and a scene, its batching cost, remove it when not fading
- [ ] 3.5 The full gate: build, workspace tests, clippy, `scripts/gen-bevy-types.sh --check`
