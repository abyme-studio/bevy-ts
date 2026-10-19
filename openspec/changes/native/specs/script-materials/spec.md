## ADDED Requirements

### Requirement: Materials are declared in TS as WGSL plus a parameter schema
The `bevy-ts/material` module SHALL export `material(name, { params, textures?, fragment, vertex?, alphaMode? })`, where `params` maps names to WGSL-layout types (`f32`, `i32`, `u32`, `vec2f`, `vec3f`, `vec4f`, `mat4x4f`, `color`, and fixed arrays of these), `textures` maps names to `texture2d` with a sampler, and `fragment`/`vertex` are WGSL source strings using Bevy's PBR or mesh shader imports. The prebuilt engine SHALL render such materials with no Rust per material.

#### Scenario: A glowing material with no Rust
- **WHEN** `game.ts` declares `material("game::Glow", { params: { color: color, pulse: f32 }, fragment: glowWgsl })` and a TSX `<material kind={Glow} color="#f80" pulse={2} />` sits on a mesh entity
- **THEN** the prebuilt engine draws the mesh with that WGSL and those values

### Requirement: Parameter layout follows WGSL uniform rules, computed in Rust
Rust SHALL lay out a material's params as one WGSL uniform struct (`vec3f` aligned to 16, struct size rounded to 16, arrays strided to 16), generate the WGSL struct declaration and its `@group(#{MATERIAL_BIND_GROUP}) @binding(n)` bindings, and prepend them to the author's source. Material param layout SHALL be separate from component layout: component fields keep bridge's C rules.

#### Scenario: Generated binding
- **WHEN** a material declares `params: { color: color, pulse: f32 }` and `textures: { noise: texture2d }`
- **THEN** the WGSL the shader receives declares a `GlowParams` struct of size 32 at binding 0, the texture at binding 1 and its sampler at binding 2

### Requirement: Materials and their shaders hot reload
A changed `fragment` or `vertex` source in a new generation SHALL replace the shader asset in place so Bevy re-specializes pipelines; a changed params schema SHALL rebuild the material's bind group layout and re-encode every instance by param name, as components migrate by field name. A WGSL compile error SHALL be reported with the material name and the line in the author's source, and the previous shader SHALL keep drawing.

#### Scenario: Editing a shader
- **WHEN** the author changes a constant in `glowWgsl` and saves
- **THEN** every `game::Glow` mesh draws with the new shader within a frame of the generation applying, and no entity respawns

#### Scenario: A WGSL error
- **WHEN** the new `fragment` source has a type error on its line 12
- **THEN** the error names `game::Glow` and line 12 of the source, and the old shader keeps drawing

### Requirement: Render-world work beyond materials stays Rust
Render passes, render-graph systems, compute pipelines, GPU readback and custom draw functions SHALL be written in the native crate. bevy-ts SHALL NOT run JS on the render thread or in the render world.

#### Scenario: A custom post-process
- **WHEN** a project needs a post-processing pass
- **THEN** it is a Rust system in `Core3d` in the native crate, and TS may set its settings through a layout-tier component on the camera
