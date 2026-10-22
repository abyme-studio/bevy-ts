## 1. `<material alpha alphaMode>` (`crates/bevy-ts-tsx`, `crates/bevy-ts-material`, `packages/bevy-ts`)

- [ ] 1.1 `kinds.rs`: make the `AlphaDecl` parser public so `<material>` uses the same names
- [ ] 1.2 `intrinsics.rs`: `MaterialProps.alpha` (checked 0..1) and `alphaMode`; `material()` sets `base_color` alpha and the mode, defaulting to `Blend` below 1 as `StandardMaterial::from(Color)`
- [ ] 1.3 `jsx-runtime.d.ts`: `MaterialProps.alpha`, `alphaMode` (the `AlphaMode` type from `bevy-ts/material`)

## 2. `SceneAlpha` (`crates/bevy-ts-material`)

- [ ] 2.1 Rebase onto main once scene-material lands (build-scene-material says when)
- [ ] 2.2 `alpha.rs`: `SceneAlpha { alpha: f32 }` (reflected, layout tier), `SceneAlphaTargets`, `sync(world, root)`: copy each distinct original once, point meshes at copies, rewrite copies on change only when they differ
- [ ] 2.3 Triggers: a `WorldInstanceReady` observer that records the root; a `PostUpdate` sync system `.after(SceneMaterialSet)` for recorded roots, `Changed<SceneAlpha>`, `AssetEvent::Modified` of an original, removal (restores meshes, fixes copies in `SceneMaterialTargets`); registered by `ScriptMaterialRenderPlugin`
- [ ] 2.4 `scripts/gen-bevy-types.sh`: `bevy-ts/bevy` names `SceneAlpha`

## 3. Tests and docs

- [ ] 3.1 Headless render test (offscreen image, as `bevy-ts-material/tests/spike.rs`): a red cube with `<material>` alpha 0.5 over a white backdrop reads pink; `crate.gltf` under a root with `SceneAlpha(0.5)` reads blended, and a second crate with no `SceneAlpha` beside it reads as before
- [ ] 3.2 `bevy-ts-material/tests/alpha.rs`, headless: copies per root, originals untouched, a changed alpha adds no asset, removal restores, a respawned scene is faded again, a TSX child untouched, a script-material mesh untouched and restored correctly with both removed in either order
- [ ] 3.3 `bevy-ts-cli/tests/materials.rs`: TSX end to end, `<SceneAlpha alpha={() => k()} />` on a `WorldAssetRoot` follows a signal frame by frame; `<material alpha={2}>` fails at the element
- [ ] 3.4 `docs/materials.md`: alpha, alphaMode, `SceneAlpha`, its batching cost, remove it when not fading
- [ ] 3.5 The full gate: build, workspace tests, clippy, `scripts/gen-bevy-types.sh --check`
