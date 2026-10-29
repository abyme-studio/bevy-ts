## Why

learn's star map in agent-town draws each subject as a pixel-art planet: sea, land, cloud and a ring, each a WGSL fragment ported from Deep-Fold's PixelPlanets (MIT). The map is a `Camera2d` world, and bevy-ts's 2D side has only `ColorMaterial` (`<material2d>`). A custom material exists only in 3D (`material()` on `<mesh>`) and as a fullscreen pass. `docs/2d.md` lists a custom `Material2d` as "not planned" and `docs/materials.md` as "not yet". A 2D material is the same data as a 3D one: a params schema and a `.wgsl` file. So a script can declare one.

## What Changes

- `bevy-ts/material` exports `material2d(name, { params, instance, textures, fragment, vertex?, alphaMode })`: a 2D material kind, with `material()`'s params, instance params, textures and checks. `alphaMode` takes 2D's three: `"opaque"`, `"blend"`, `{ mask }`.
- TSX `<material2d kind={Planet} ... />` beside a `<mesh2d>` draws the mesh with that kind, as `<material kind>` does beside `<mesh>`. `<material2d>` without `kind` stays a `ColorMaterial`. Instance params may be bound; params and textures are keyed props, shared by equal props.
- The engine draws it through Bevy 0.20's 2D material pipeline (`PreparedMaterial2d`, `Mesh2dPipeline`): one erased asset type for every 2D kind, as `ScriptMaterial` is for 3D. It appends the params struct, the textures, and the instance array at `@group(2)`, and checks the file with Bevy's 2D imports.
- `.wgsl` saves and schema changes hot reload as 3D materials do, through `PipelineWatch`. WGSL errors name the kind and the author's `file:line:col`.
- Pixel art stays crisp under `PixelCamera`: the doc and tests fix how a shader finds its art pixel (`floor(in.uv * pixels)` on a quad placed on whole world units, or `floor(in.world_position.xy)` anywhere), and a script 2D material sorts under `YSort` as any `Mesh2d` does.
- A bench, `bench/material2d`: planets of three layers under `PixelCamera` zoom 3 at 1920×1080, from one planet to 500, against no planets.
- Out of scope, follow-ups: a script material on a `Sprite` (Bevy 0.20 draws a sprite as a unit quad whose size, anchor, rect and flip live in `SpriteMeshMaterial`'s uniform, so a swapped material loses them; Bevy's own `SpriteMaterial<M>` takes a static shader); `Kind.add` owned instances in 2D; a script material on a `TilemapChunk`; render targets for drawing a planet once at art size.

## Capabilities

### New Capabilities
- `script-material2d`: 2D mesh materials declared in TS with a params schema, instance params, textures and a `.wgsl` fragment, drawn by Bevy's 2D material pipeline with no Rust per kind.

### Modified Capabilities
- `script-2d`: `<material2d>` is a `ColorMaterial` only without `kind`; with `kind` it is a script 2D material.

## Impact

- Frame cost. This is on the operator's ask-first list.
  - GPU: each covered screen pixel runs the kind's fragment once per layer. `PixelCamera` zoom 3 runs it 9 times per art pixel. A planet's disc covers 0.785 of its quad.
  - CPU: per drawn entity, Bevy's 2D extract, specialise (cached after the first frame) and queue. Opaque layers of one kind and one asset batch into one draw across planets: they bin by pipeline and bind group, and instance params live in one storage buffer read by `MeshTag`, not in the asset. Blend layers are sorted by z, so they batch only where consecutive draws share a kind and asset: stacked layers of one planet alternate kinds and draw one call each, unless each layer kind sits in its own z band. Instance params upload the whole buffer once in a frame where any changed (256 bytes an entity); learn writes them only on a read, never per frame.
  - Estimate, from the learn-planets research (about 150 noise hashes per covered pixel for a Terran planet, about 150 M hashes per ms on this GPU), not yet measured: one 200-art-px planet at most 0.3 ms GPU; one 48-art-px planet at most 0.02 ms; 20 planets at most 0.3 ms; 100 at most 1.6 ms; 500 at most 8 ms. CPU about 0 at rest. An app with no script 2D material costs nothing new: the plugin's systems find no entities.
  - `bench/material2d` measures it. Gate: `n100` adds under 2 ms to `off`'s median frame, on a machine at load under 10. Its results land with the code and replace the estimate in `docs/materials.md`.
- `crates/bevy-ts-material`: a new module `material2d.rs` (the 2D asset, its `ErasedRenderAsset`, extract and specialisation), reusing `layout.rs`, `wgsl.rs`, `kinds.rs` and `instance.rs`. No new dependency: `bevy_sprite_render` is already built.
- `crates/bevy-ts-tsx`: `<material2d kind>`. `packages/bevy-ts`: its types in `jsx-runtime.d.ts`.
- `bevy-ts/material` (`ts/material.js`, `ts/material.d.ts`): `material2d()` and `Material2dKind`.
- Docs: `docs/materials.md` gains "2D materials" and loses script `Material2d` from "Not yet"; `docs/2d.md`'s `<material2d>` section points at it and its coverage table moves custom `Material2d` to working; `CHANGELOG.md` a line.
- `examples/shapes-2d`: a planet of two layers.
- Tests: `bevy-ts-material`'s `material2d.rs` and `material2d_render.rs`, `bevy-ts-cli`'s `materials_2d.rs`.
- Every test, example and bench texture or shader is made in the repo. No PixelPlanets file is committed here; agent-town ports them.
