# A painted, cel-shaded, outlined look under an orthographic camera (2026-10-23)

The question, from agent town's art direction: a 3D world, an orthographic camera, and shading
that reads as a hand-painted 2D illustration ("2.5D"): soft cel bands, ink outlines, a warm
palette. Reference: `~/.local/state/abyme/documents/bevy-ts/agent-town-art-reference.jpg`.

## Answer

Do it in a script material, and fix the engine so script materials take part in Bevy's prepass and
shadow passes.

- Cel lighting is game work: a WGSL file that reads the sun from `mesh_view_bindings::lights`,
  samples its shadow with `shadows::fetch_directional_shadow` and steps the result into bands.
  Script materials can already do this.
- Ink lines are game work too: the same material reads the depth and normal prepass
  (`bevy_pbr::prepass::utils`) at four neighbours and darkens where depth or normal jumps. Under
  an orthographic camera depth is linear, so one threshold holds across the whole screen.
- The engine work is one gap: a script material is drawn in the main pass only. It casts no shadow
  and writes nothing to the prepass, so the shadows and the ink both come out empty. The fix is
  small and proven (below).

Sure: high for the look and the engine fix (rendered here). Medium for cost (estimated, not
measured on skinned characters).

## The proof

`proof.patch` holds a headless test, `crates/bevy-ts-material/tests/toon_proof.rs`, plus the engine
fix in `crates/bevy-ts-material/src/render.rs`. Run it with
`git apply docs/research/toon-look/proof.patch && cargo test -p bevy-ts-material --test toon_proof`.
It renders one small scene to three offscreen images through the real `ScriptMaterial` path: a
ground, a pond, trees, rocks, a house and six figures, with an orthographic camera at 55° pitch.

| panel | material | camera |
|---|---|---|
| `a-pbr.png` | `StandardMaterial` | default tonemapping |
| `b-toon.png` | script toon: two soft steps, a cool shade colour, a warm rim | `Tonemapping::None` |
| `c-ink.png` | the same, plus ink from the depth and normal prepass | plus `DepthPrepass`, `NormalPrepass` |

Without the engine fix, b and c had no cast shadows and c had no lines. With it, both draw as
intended. Two artefacts are left, and both are tuning: a line along the frame border (clamp the
neighbour taps to the screen) and stair-stepped shadow edges (one low-res cascade; raise the
shadow map size or soften the step).

## Engine work vs game work

Engine (one change, for spec):

1. Script materials join the prepass and the shadow pass. In `render.rs`'s `prepare_asset`:
   add the `PrepassOpaque`, `PrepassOpaqueDepthOnly`, `Shadows` and `ShadowsDepthOnly` draw
   functions, set `prepass_specialize`, `shadows_enabled` and `prepass_enabled`. Bevy's
   `prepass_specialize` is private, so the crate keeps a 20-line copy (all its types are public).
   Two edges to decide: a kind with its own `vertex` shader moves vertices that Bevy's default
   prepass vertex shader will not move (keep those kinds out of the prepass, or add a
   `prepassVertex`); and `alphaMode: { mask }` kinds need the alpha-mask prepass functions and a
   discard. An opt-out flag (`shadows: false`) follows Bevy's `Material::enable_shadows`.
2. Check that a game can build this camera from TS. The `camera3d` recipe takes only `position`
   and `lookAt`. By the native-components spec, Bevy's reflected `Projection`, `DepthPrepass`,
   `NormalPrepass` and `Tonemapping` should already work as TSX elements; this is unverified.
   If they do not, add `projection`, `prepass` and `tonemapping` to the recipe.
3. Probably a later change: toon materials on glTF models. Skinned characters will arrive as glTF
   scenes carrying `StandardMaterial`. Swapping in a script material (and carrying each
   primitive's base colour texture into a texture param) has no TS path today. Skinning itself
   is fine: Bevy's mesh pipeline skins any material.

Game (agent town):

- `toon.wgsl`: bands, shade colour, rim, ink. The proof's shader is a starting point.
- Painterly touches, all doable in script materials today: a ramp texture sampled by N·L in place
  of the `smoothstep`s (`textures: { ramp: texture2d }`); a world-space paper or brush noise
  texture that wobbles the band edges; vertex colours; a cool-shifted shade colour per material
  (as in the proof); `Tonemapping::None` so the palette lands exactly; Bevy's `ColorGrading` on
  the camera for an overall warm grade.
- Art: painted albedo textures and a small, warm palette do more than any shader.

## Outlines: why in the material

| option | where | verdict |
|---|---|---|
| Ink in the material from the prepass | game WGSL, after engine fix 1 | **Chosen.** No new pass and no dependency, and it draws crease lines inside shapes too, as the reference does. It needs every outlined thing to use a toon material. |
| Edge-detect post-process | engine Rust | Later, only if `StandardMaterial` meshes must be outlined. Bevy 0.20's `FullscreenMaterial` binds only the screen texture, sampler and a uniform, with no prepass textures, so this needs a custom `Core3d` node (as in Bevy's `custom_post_processing` example), with settings on a camera component. |
| Inverted hull (`bevy_mod_outline`) | new dependency | No. 0.13 targets Bevy 0.19, with no 0.20 release yet. It adds an extra draw per outlined mesh (skinned again for each character), gives silhouettes only, and creased meshes need smoothed outline normals. |

Upstream Bevy has no toon shading and no outline feature in 0.20. The parts used here are all
upstream: prepass, shadow sampling, orthographic projection, tonemapping and colour grading.

## Cost on 50-100 skinned characters

Estimated, not measured:

- The toon fragment is cheaper than PBR: one light, no specular or environment lighting.
- The prepass draws each opaque mesh once more, skinned again in its vertex shader. The shadow pass
  already does the same for `StandardMaterial`. 100 characters at ~5k triangles is ~0.5M
  triangles per pass, well under a millisecond on the M-class GPUs this runs on.
- Ink is 8 extra texel loads per toon pixel, about 16M at 1080p: a fraction of a millisecond, and
  the same whether there are 5 characters or 100.
- An inverted hull would instead add a skinned draw per character, and the cost grows with the cast.

The change should measure it: 100 skinned glTF characters (Bevy's `Fox.glb`) in release, frame
time with and without prepass and ink.

## Sources

- bevy-ts: `crates/bevy-ts-material/src/render.rs` (draw functions it registers),
  `openspec/specs/script-materials/spec.md`, `openspec/specs/native-components/spec.md`,
  `crates/bevy-ts/src/recipes.rs`.
- Bevy 0.20: `bevy_pbr/src/material.rs` (`MaterialPlugin`'s draw functions, `prepass_specialize`),
  `bevy_pbr/src/prepass/utils.wesl`, `bevy_pbr/src/render/shadows.wesl`,
  `bevy_core_pipeline/src/fullscreen_material.rs`, [release notes](https://bevy.org/news/bevy-0-20/).
- crates.io API: `bevy_mod_outline` 0.13.0 (2026-07-09) depends on `bevy ^0.19.0`;
  `bevy_outline_post_process` 0.5 on 0.15; `bevy_toon_shader` 0.3 on 0.12.
- The Reddit thread in the brief could not be fetched (reddit blocks the fetcher).
