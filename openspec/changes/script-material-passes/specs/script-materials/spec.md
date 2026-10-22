## MODIFIED Requirements

### Requirement: Materials are declared in TS with a parameter schema and WGSL files
The `bevy-ts/material` module SHALL export `material(name, { params, textures?, fragment, vertex?, alphaMode?, shadows?, prepass? })`, where `params` maps names to WGSL-layout types (`f32`, `i32`, `u32`, `vec2f`, `vec3f`, `vec4f`, `mat4x4f`, `color`, and fixed arrays of these), `textures` maps names to `texture2d` with a sampler, `shadows` and `prepass` are booleans (see "Materials draw in the prepass and the shadow pass"), and `fragment`/`vertex` are shader-file values from a relative `.wgsl` import (see script-runtime), each a file of WESL, as Bevy 0.20 compiles material shaders, using Bevy's PBR or mesh imports (`import bevy_pbr::render::forward_io::VertexOutput;`). A WGSL source string SHALL be rejected with the material name and the declaring file and line. Every param and texture name SHALL be a WGSL identifier that is not a WGSL reserved word and does not start with `__`. A texture name SHALL NOT be a predeclared WGSL type (`sampler`, `f32`, `texture_2d` and the other `texture_*`), and SHALL NOT be used by a param, `params`, or another texture's `<name>_sampler`. No texture or sampler name SHALL equal the generated struct's name (`GlowParams` for `game::Glow`), and a kind whose struct name would start with `__` SHALL be refused. Any of these SHALL fail the load at the `material()` call. An unset `color` param, and each element of an `array(color, n)`, SHALL default to white. The params schema SHALL stay in TS. The prebuilt engine SHALL render such materials with no Rust per material.

#### Scenario: A glowing material with no Rust
- **WHEN** `game.ts` writes `import glow from "./glow.wgsl"`, declares `material("game::Glow", { params: { color: color, pulse: f32 }, fragment: glow })`, and a TSX `<material kind={Glow} color="#f80" pulse={2} />` sits on a mesh entity
- **THEN** the prebuilt engine draws the mesh with that WGSL and those values

## ADDED Requirements

### Requirement: Materials draw in the prepass and the shadow pass
A script material kind SHALL take part in Bevy's prepass and shadow pass as a typed `Material` does, through Bevy's default prepass shader, which skins and morphs: its meshes SHALL cast shadows from shadow-casting lights, and SHALL write depth and normals to the prepass of a camera with `DepthPrepass` or `NormalPrepass`, so the kind's own fragment shader and other kinds can read them (`bevy_pbr::prepass::utils`). `shadows` and `prepass` SHALL turn each off, as Bevy's `Material::enable_shadows` and `enable_prepass` do. Each SHALL default to true for a kind whose `alphaMode` is `"opaque"` and that has no `vertex`, and to false for any other kind, since the default prepass shader would draw a moved vertex where it was and a cut-out as solid. An author MAY set either to true on such a kind to take Bevy's default prepass shader anyway. A kind with `prepass: false` SHALL still draw in the main pass of a camera that has a prepass. A changed flag SHALL apply on the next generation, as a changed `alphaMode` does, with no entity respawned. Bevy's per-entity `NotShadowCaster` and `NotShadowReceiver` SHALL apply to script materials as to typed ones.

#### Scenario: A toon ground receives a cast shadow
- **WHEN** an opaque script material with no `vertex` covers a ground plane and a cube above it, under a directional light with shadows
- **THEN** the ground under the cube draws darker than the ground in the open, as read from the kind's fragment shader through `fetch_directional_shadow`

#### Scenario: Ink from the prepass
- **WHEN** a camera has `DepthPrepass` and `NormalPrepass` and an opaque script material reads `prepass_depth` and `prepass_normal` at a pixel's neighbours
- **THEN** pixels along a mesh's silhouette draw in the ink colour, and pixels inside a flat face do not

#### Scenario: Opting out
- **WHEN** a kind declares `shadows: false`
- **THEN** its meshes cast no shadow, and still receive shadows in their own fragment shader

#### Scenario: A vertex kind stays out by default
- **WHEN** a kind declares a `vertex` file and neither flag
- **THEN** its meshes cast no shadow and write nothing to the prepass, and with `shadows: true` they cast the shadow of the unmoved mesh

#### Scenario: Skinned meshes
- **WHEN** an opaque script material is on a skinned mesh
- **THEN** its shadow and its prepass depth follow the animated pose

### Requirement: An orthographic prepass camera from TSX
A TSX camera entity SHALL be able to carry `Projection` (orthographic, with its scaling mode), `DepthPrepass`, `NormalPrepass` and `Tonemapping` from `bevy-ts/bevy`, as reflect-tier or layout-tier elements, beside the `camera3d` recipe, with no Rust in the game.

#### Scenario: The toon camera
- **WHEN** a TSX entity uses the `camera3d` recipe with `<Projection>` set to orthographic with a fixed vertical height, `<DepthPrepass />`, `<NormalPrepass />` and `<Tonemapping value="None" />`
- **THEN** the spawned camera has an orthographic projection of that height, both prepass markers and `Tonemapping::None`
