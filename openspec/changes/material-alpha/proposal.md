## Why

agent-town fades a cluster of up to 10 glTF buildings out over 2 s before it leaves. Today nothing in bevy-ts can make a mesh see-through: `<material>` has no alpha mode, so a colour's alpha is ignored (`StandardMaterial` defaults to `Opaque`), and a scene's meshes keep their own materials with no way to fade them. The fade must be a number bound to a signal each frame, with no remount.

## What Changes

- **`<material alpha alphaMode>`.** `alpha` (0 to 1) sets the alpha of `base_color`, Bevy's `Color::with_alpha`. `alphaMode` takes the values a script material's `alphaMode` takes (`"opaque"`, `"blend"`, `"premultiplied"`, `"add"`, `"multiply"`, `"alphaToCoverage"`, `{ mask: 0.5 }`). Unset, it is `"blend"` when the final alpha is below 1 and `"opaque"` otherwise, as Bevy's `StandardMaterial::from(Color)` does. A colour written with alpha (`"#ff000080"`) now draws see-through by the same rule. Both are plain props, not bindings, as every `<material>` prop is.
- **`SceneAlpha { alpha }` on a glTF scene root.** A new component in `bevy-ts-material`. On a `WorldAssetRoot`, every scene mesh with a `StandardMaterial` draws with a copy of its material whose alpha is the original's times `alpha`, and whose mode becomes `Blend` below 1 when it was `Opaque`, `Mask` or `AlphaToCoverage`. It is a layout-tier component, so TSX binds it like any field: `<SceneAlpha alpha={() => k()} />`. Changing it each frame edits the copies in place; it makes no asset. Removing it restores the scene's own materials. `alpha` is clamped to 0..1.
- Names: `alpha`, as Bevy says it (`Color::alpha`, `AlphaMode`); Bevy has no "opacity" on materials.
- Builds on `scene-material` (lands first): its `scene_entities(world, root)` lists the instance's own meshes, so TSX children are never touched. A mesh it swapped to a script material has no `StandardMaterial` and is left alone; its alpha is its kind's `alphaMode`.
- **Out of scope:** binding `<material>` props; `SceneAlpha` on a plain mesh entity (use `<material alpha>`, or a scene); fading script-material meshes; shadows of faded meshes (Bevy draws no shadow for `Blend`).

## Capabilities

### New Capabilities

- `standard-materials`: `<material>`'s `StandardMaterial` props, now with `alpha` and `alphaMode`, and `SceneAlpha` on a glTF scene root.

### Modified Capabilities

None.

## Impact

- `crates/bevy-ts-material`: new `alpha.rs` (`SceneAlpha`, `SceneAlphaTargets`, its sync), registered by `ScriptMaterialRenderPlugin`; `kinds.rs` gives its alpha-mode parser to `<material>`.
- `crates/bevy-ts-tsx/src/intrinsics.rs`: `MaterialProps.alpha`, `alphaMode`.
- `packages/bevy-ts/jsx-runtime.d.ts`: `MaterialProps.alpha`, `alphaMode`; `bevy-ts/bevy` regenerated, now naming `SceneAlpha`.
- `docs/materials.md`. No protocol change.
- Performance: each faded root holds one `StandardMaterial` copy per distinct material in its scene, so its meshes no longer batch with other roots of the same model while it fades. Changing `alpha` rewrites those copies' uniforms each frame it changes.
