## MODIFIED Requirements

### Requirement: Render-world work beyond materials stays Rust
Render passes other than script passes (script-passes), render-graph systems, compute pipelines, GPU readback and custom draw functions SHALL be written in the native crate. bevy-ts SHALL NOT run JS on the render thread or in the render world; a script pass's shader and params are data the engine draws, as a material's are.

#### Scenario: A custom post-process
- **WHEN** a project needs a post-processing pass that a script pass cannot express (a second texture, a render target, compute, or a `Camera3d`)
- **THEN** it is a Rust system in `Core3d` or `Core2d` in the native crate, and TS may set its settings through a layout-tier component on the camera

#### Scenario: A 2D grading pass
- **WHEN** a project grades its `Camera2d`'s image with one fragment shader and params
- **THEN** it is a script pass declared in TS, with no native crate
