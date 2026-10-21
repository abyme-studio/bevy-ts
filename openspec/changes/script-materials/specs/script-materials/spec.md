## ADDED Requirements

### Requirement: Materials are declared in TS with a parameter schema and WGSL files
The `bevy-ts/material` module SHALL export `material(name, { params, textures?, fragment, vertex?, alphaMode? })`, where `params` maps names to WGSL-layout types (`f32`, `i32`, `u32`, `vec2f`, `vec3f`, `vec4f`, `mat4x4f`, `color`, and fixed arrays of these), `textures` maps names to `texture2d` with a sampler, and `fragment`/`vertex` are shader-file values from a relative `.wgsl` import (see script-runtime), each a file of WGSL using Bevy's PBR or mesh shader imports. A WGSL source string SHALL be rejected with the material name and the declaring file and line. The params schema SHALL stay in TS. The prebuilt engine SHALL render such materials with no Rust per material.

#### Scenario: A glowing material with no Rust
- **WHEN** `game.ts` writes `import glow from "./glow.wgsl"`, declares `material("game::Glow", { params: { color: color, pulse: f32 }, fragment: glow })`, and a TSX `<material kind={Glow} color="#f80" pulse={2} />` sits on a mesh entity
- **THEN** the prebuilt engine draws the mesh with that WGSL and those values

### Requirement: Parameter layout follows WGSL uniform rules, computed in Rust
Rust SHALL lay out a material's params as one WGSL uniform struct (`vec3f` aligned to 16, struct size rounded to 16, arrays strided to 16), generate the WGSL struct declaration and its `@group(#{MATERIAL_BIND_GROUP}) @binding(n)` bindings, and prepend them to the author's file. The author's file SHALL NOT declare them. Material param layout SHALL be separate from component layout: component fields keep bridge's C rules.

#### Scenario: Generated binding
- **WHEN** a material declares `params: { color: color, pulse: f32 }` and `textures: { noise: texture2d }`
- **THEN** the WGSL the shader receives declares a `GlowParams` struct of size 32 at binding 0, the texture at binding 1 and its sampler at binding 2

### Requirement: Materials and their shaders hot reload
A saved `.wgsl` file SHALL replace the shader asset of every material that names it, in place, so Bevy re-specializes pipelines; it SHALL NOT start a new generation, since no JS changed. A new generation that points `fragment` or `vertex` at another file SHALL do the same. A changed params schema SHALL rebuild the material's bind group layout, regenerate the prepended declarations and re-encode every instance by param name, as components migrate by field name.

#### Scenario: Editing a shader
- **WHEN** the author changes a constant in `glow.wgsl` and saves
- **THEN** every `game::Glow` mesh draws with the new shader within a frame, no entity respawns and the generation number does not change

### Requirement: WGSL errors point into the author's file
Rust SHALL record how many lines it prepends to each shader. A WGSL parse or validation error SHALL be reported in the terminal and on the protocol's log stream with the material name, the author's file path and the line and column in that file, by subtracting the prepended lines. An error inside the prepended declarations SHALL name the material and its params schema in TS instead of a line. On any WGSL error the previous shader SHALL keep drawing; with no previous shader the material SHALL draw nothing until fixed.

#### Scenario: A WGSL error
- **WHEN** `glow.wgsl` has a type error on its line 12 and is saved
- **THEN** the error names `game::Glow` and `src/glow.wgsl:12`, not the line in the combined source, and the old shader keeps drawing

#### Scenario: A field the schema does not declare
- **WHEN** `glow.wgsl` reads `params.speed` on line 8 and the schema has no `speed`
- **THEN** the error names `game::Glow` and `src/glow.wgsl:8`

### Requirement: Render-world work beyond materials stays Rust
Render passes, render-graph systems, compute pipelines, GPU readback and custom draw functions SHALL be written in the native crate. bevy-ts SHALL NOT run JS on the render thread or in the render world.

#### Scenario: A custom post-process
- **WHEN** a project needs a post-processing pass
- **THEN** it is a Rust system in `Core3d` in the native crate, and TS may set its settings through a layout-tier component on the camera
