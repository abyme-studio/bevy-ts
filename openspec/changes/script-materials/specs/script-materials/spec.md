## ADDED Requirements

### Requirement: Materials are declared in TS with a parameter schema and WGSL files
The `bevy-ts/material` module SHALL export `material(name, { params, textures?, fragment, vertex?, alphaMode? })`, where `params` maps names to WGSL-layout types (`f32`, `i32`, `u32`, `vec2f`, `vec3f`, `vec4f`, `mat4x4f`, `color`, and fixed arrays of these), `textures` maps names to `texture2d` with a sampler, and `fragment`/`vertex` are shader-file values from a relative `.wgsl` import (see script-runtime), each a file of WESL, as Bevy 0.20 compiles material shaders, using Bevy's PBR or mesh imports (`import bevy_pbr::render::forward_io::VertexOutput;`). A WGSL source string SHALL be rejected with the material name and the declaring file and line. Every param and texture name SHALL be a WGSL identifier that is not a WGSL reserved word and does not start with `__`. A texture name SHALL NOT be used by a param, `params`, or another texture's `<name>_sampler`, and no texture or sampler name SHALL equal the generated struct's name (`GlowParams` for `game::Glow`). Any of these SHALL fail the load at the `material()` call. An unset `color` param, and each element of an `array(color, n)`, SHALL default to white. The params schema SHALL stay in TS. The prebuilt engine SHALL render such materials with no Rust per material.

#### Scenario: A glowing material with no Rust
- **WHEN** `game.ts` writes `import glow from "./glow.wgsl"`, declares `material("game::Glow", { params: { color: color, pulse: f32 }, fragment: glow })`, and a TSX `<material kind={Glow} color="#f80" pulse={2} />` sits on a mesh entity
- **THEN** the prebuilt engine draws the mesh with that WGSL and those values

### Requirement: Parameter layout follows WGSL uniform rules, computed in Rust
Rust SHALL lay out a material's params as one WGSL uniform struct (`vec3f` aligned to 16, struct size rounded to 16, arrays strided to 16), generate the WGSL struct declaration and its `@group(3) @binding(n)` bindings (Bevy's material bind group, `MATERIAL_BIND_GROUP_INDEX`), and append them after the author's last line, since WESL imports must come first and appending keeps every author line number. The author's file SHALL NOT declare them. Material param layout SHALL be separate from component layout: component fields keep bridge's C rules.

#### Scenario: A texture name that collides
- **WHEN** a material declares `params: { noise_sampler: f32 }` and `textures: { noise: texture2d }`
- **THEN** the load fails at the `material()` call, naming `noise_sampler`

#### Scenario: Generated binding
- **WHEN** a material declares `params: { color: color, pulse: f32 }` and `textures: { noise: texture2d }`
- **THEN** the WGSL the shader receives declares a `GlowParams` struct of size 32 at binding 0, the texture at binding 1 and its sampler at binding 2

### Requirement: Materials and their shaders hot reload
A saved `.wgsl` file SHALL replace the shader asset of every material that names it, in place, so Bevy re-specializes pipelines; it SHALL NOT start a new generation, since no JS changed. A new generation that points `fragment` or `vertex` at another file SHALL do the same. A changed params schema SHALL rebuild the material's bind group layout, regenerate the appended declarations and re-encode every instance by param name, as components migrate by field name. When a new schema's shaders fail the check before the swap, the kind SHALL keep drawing as it was; the newer declaration SHALL be kept, and a later `.wgsl` save that fixes the file SHALL apply it with no new generation. A schema change that adds a stage (a `vertex` where there was none) SHALL be checked the same way, under the fragment's live declarations. While a newer schema waits, a `.wgsl` save that the live schema accepts SHALL still update the live shader.

#### Scenario: Editing a shader
- **WHEN** the author changes a constant in `glow.wgsl` and saves
- **THEN** every `game::Glow` mesh draws with the new shader within a frame, no entity respawns and the generation number does not change

#### Scenario: A schema change the shader does not match yet
- **WHEN** a save adds `speed: f32` to `game::Glow`'s params and `glow.wgsl` still fails the check
- **THEN** `game::Glow` meshes keep drawing as before, and saving a fixed `glow.wgsl` applies the new schema with no new generation

### Requirement: WGSL errors point into the author's file
Rust SHALL record the author's line count for each shader. A WESL or WGSL parse or validation error SHALL be reported in the terminal and on the protocol's log stream with the material name, the author's file path and the line and column in that file; since the declarations are appended, author lines keep their numbers. An error past the author's last line, in the generated declarations, SHALL name the material and its params schema in TS instead of a line. On any WGSL error the previous shader SHALL keep drawing; with no previous shader the material SHALL draw nothing until fixed. A failure from Bevy's renderer SHALL be reported only if it reproduces on the stage's current source, and once per source, so a stale pipeline error is not reported against a file already fixed.

#### Scenario: A WGSL error
- **WHEN** `glow.wgsl` has a type error on its line 12 and is saved
- **THEN** the error names `game::Glow` and `src/glow.wgsl:12`, not the line in the combined source, and the old shader keeps drawing

#### Scenario: A field the schema does not declare
- **WHEN** `glow.wgsl` reads `params.speed` on line 8 and the schema has no `speed`
- **THEN** the error names `game::Glow` and `src/glow.wgsl:8`

### Requirement: Material instances are shared assets keyed by kind and props
A TSX `<material kind={...} />` SHALL resolve to a `ScriptMaterial` asset keyed by its kind and its props, shared by every element with equal props, as `<material>`'s `StandardMaterial` cache shares by props. The entity SHALL hold `ScriptMaterial3d(handle)`. A param bound to a function (`prop={() => ...}`) SHALL fail the load at the element with `<material> props cannot be bound yet; bind a component field or a UI prop`, as on `<mesh>` and `<material>`. Props SHALL be checked against the materials section of the element's own generation, whatever the order of the manifest's sections.

#### Scenario: Two equal instances
- **WHEN** two meshes carry `<material kind={Glow} color="#f80" pulse={2} />`
- **THEN** both hold the same `ScriptMaterial3d` handle, and a third with `pulse={3}` holds another

#### Scenario: A bound param
- **WHEN** an element writes `<material kind={Glow} pulse={() => t.value} />`
- **THEN** the load fails at that element with `<material> props cannot be bound yet; bind a component field or a UI prop`

### Requirement: Render-world work beyond materials stays Rust
Render passes, render-graph systems, compute pipelines, GPU readback and custom draw functions SHALL be written in the native crate. bevy-ts SHALL NOT run JS on the render thread or in the render world.

#### Scenario: A custom post-process
- **WHEN** a project needs a post-processing pass
- **THEN** it is a Rust system in `Core3d` in the native crate, and TS may set its settings through a layout-tier component on the camera
