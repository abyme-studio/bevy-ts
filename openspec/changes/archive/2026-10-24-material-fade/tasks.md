## 1. The fade material

- [x] 1.1 `src/fade.wgsl`: `bayer4`, a main-pass fragment that discards above `alpha` unless the base is a blend mode, and a prepass fragment that always discards above `alpha` (design 2)
- [x] 1.2 `Fade` extension and `FadeMaterial` alias in `alpha.rs`, the shader embedded, `MaterialPlugin::<FadeMaterial>` added by `alpha::plugin`; export `FadeMaterial` from `lib.rs`

## 2. alpha.rs and scene.rs

- [x] 2.1 `faded()` / `copy_of()` build a `FadeMaterial`: opaque, mask and ATC originals keep mode and colour with `Fade.alpha`; blend originals get `base_color` alpha times `alpha`
- [x] 2.2 `sync` swaps `MeshMaterial3d<StandardMaterial>` for `MeshMaterial3d<FadeMaterial>`; in-place edits write `Fade.alpha` and the base colour only when they differ; `restore` swaps back
- [x] 2.3 `targets()` counts a mesh holding either type; `MaterialAlphaTargets` keys copies by `AssetId<FadeMaterial>`; `recopy` rebuilds the base from a reloaded original
- [x] 2.4 `alpha::standard_of(world, mesh)`; `scene.rs` uses it to keep a faded mesh's original and removes `MeshMaterial3d<FadeMaterial>` when it swaps; drop the kept-copy rewrite in `restore` (design 4)
- [x] 2.5 `docs/materials.md`: the `MaterialAlpha` bullets (dither, mode kept, shadow follows, 1/16 steps, the pattern shows) and the scene-material note
- [x] 2.6 The sync runs `.after(AnimationSystems)` (lead@animation-control's stage 2 animates `MaterialAlpha.alpha`), with a test that an alpha written in that set reaches the copy that frame

## 3. Tests (docs/testing.md: modules of `tests/it`, no fixed waits, find by component)

- [x] 3.1 `tests/it/alpha.rs`: copies are `FadeMaterial`, opaque keeps `Opaque`, a `Blend` original scales `base_color` alpha, a changed alpha adds no asset, removal leaves `StandardMaterial` and no `FadeMaterial`, scene-material composes in either order, `NotShadowCaster` untouched
- [x] 3.2 `tests/it/alpha_render.rs`, look: a red cube holding a black cube under a root at 0.5, unlit, white clear, `Msaa::Off`; no near-black pixel in its rect, red fraction 0.5 ± 1/16; the existing pink/red/ship assertions updated to the dithered fraction
- [x] 3.3 `tests/it/alpha_render.rs`, shadow: lit, a directional light with shadows, a white ground, cubes at 1, 0.5 and 0 and a `ship.gltf` root at 0; ground under each ordered as the spec says, under 0 equal to unshadowed ground
- [x] 3.4 `bevy-ts-cli/tests/it/materials.rs` and `scene_material.rs`: read a faded mesh's material as `FadeMaterial`; the TS fade test still passes end to end
- [x] 3.5 Run the material crate's and the CLI's test binaries; both green

## 4. Before/after frames (headless only, nothing on the operator's screen)

- [x] 4.1 From the look test's scene plus a textured multi-part model at 0.67, save a PNG on `main` (Blend) and on this branch (dither) under `/tmp/material-fade/`, as evidence, not a golden file (Blend reproduced on this branch with main's own copy rule, which saved a second engine build)
- [x] 4.2 Re-take the agent-town scene of `/tmp/civ-fade-1.png` (a Synty model at 0.67, same camera) on this branch, headless or in a tart VM, beside the original; if neither runs it, say so in the report
- [x] 4.3 Note in the report whether the first fade's pipeline compile shows as a hitch
