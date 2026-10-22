## Why

agent-town fades shapes now, and a cluster of up to 10 glTF buildings later, out over 2 s before they leave. Today nothing in bevy-ts can make a mesh see-through: `<material>` has no alpha mode, so a colour's alpha is ignored (`StandardMaterial` defaults to `Opaque`), and a scene's meshes keep their own materials with no way to fade them. The fade must be a number bound to a signal each frame, with no remount, on a plain mesh and on a scene alike.

## What Changes

- **`<material alpha alphaMode>`.** `alpha` (0 to 1) sets the alpha of `base_color`, Bevy's `Color::with_alpha`. `alphaMode` takes the values a script material's `alphaMode` takes (`"opaque"`, `"blend"`, `"premultiplied"`, `"add"`, `"multiply"`, `"alphaToCoverage"`, `{ mask: 0.5 }`). Unset, it is `"blend"` when the final alpha is below 1 and `"opaque"` otherwise, as Bevy's `StandardMaterial::from(Color)` does. A colour written with alpha (`"#ff000080"`) now draws see-through by the same rule. Both are plain props, not bindings, as every `<material>` prop is. They set a shape's resting look; the fade is the component below.
- **`MaterialAlpha { alpha }`.** A new component in `bevy-ts-material` that fades what an entity draws. It applies to the entity's own `MeshMaterial3d<StandardMaterial>` (a `<mesh>` with `<material>`, or one set from Rust), and, when the entity is a `WorldAssetRoot`, to every scene mesh with a `StandardMaterial`. Each such mesh draws with a copy of its material owned by the entity, whose alpha is the original's times `alpha`, and whose mode becomes `Blend` below 1 when it was `Opaque`, `Mask` or `AlphaToCoverage`. It is a layout-tier component, so TSX binds it like any field: `<MaterialAlpha alpha={() => k()} />`. Changing it each frame edits the copies in place; it makes no asset. Removing it restores the original materials. `alpha` is clamped to 0..1.
- Names: `alpha`, as Bevy says it (`Color::alpha`, `AlphaMode`); Bevy has no "opacity" on materials. `MaterialAlpha` names what it scales, the alpha of the entity's materials, not where it sits, as Bevy names `Visibility` or `NotShadowCaster`. The first draft's `SceneAlpha` read wrong once it fades plain meshes too.
- Builds on `scene-material` (lands first): its `scene_entities(world, root)` lists the instance's own meshes, so TSX children are never touched. A mesh it swapped to a script material has no `StandardMaterial` and is left alone; its alpha is its kind's `alphaMode`.
- **Out of scope:** binding `<material>` props; fading TSX children through their parent (each child takes its own `MaterialAlpha`); fading script-material meshes; shadows of faded meshes (Bevy draws no shadow for `Blend`).

## Capabilities

### New Capabilities

- `standard-materials`: `<material>`'s `StandardMaterial` props, now with `alpha` and `alphaMode`, and `MaterialAlpha` on a mesh entity or a glTF scene root.

### Modified Capabilities

None.

## Impact

- `crates/bevy-ts-material`: new `alpha.rs` (`MaterialAlpha`, `MaterialAlphaTargets`, its sync), registered by `ScriptMaterialRenderPlugin`; `kinds.rs` gives its alpha-mode parser to `<material>`.
- `crates/bevy-ts-tsx/src/intrinsics.rs`: `MaterialProps.alpha`, `alphaMode`.
- `packages/bevy-ts/jsx-runtime.d.ts`: `MaterialProps.alpha`, `alphaMode`; `bevy-ts/bevy` regenerated, now naming `MaterialAlpha`.
- `docs/materials.md`. No protocol change.
- Performance: each faded entity holds one `StandardMaterial` copy per distinct material it draws, so its meshes no longer batch with others sharing that material while it fades. Changing `alpha` re-prepares those copies each frame it changes.

### Approved ask-first items

lead@material-alpha approved these on 2026-10-23 under the operator's delegation for bevy work:

1. Per-frame re-prepare of the copies and lost batching while faded.
2. `color="#rrggbbaa"` with alpha below 1 now blends, as Bevy does.
3. The material-override component pattern (a component that swaps an entity's materials for owned copies and restores them on removal).

The plain-mesh revision adds nothing new to this list: it is the same pattern on one more kind of entity.
