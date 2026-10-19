## Why

Custom materials are data (WGSL plus parameters), so a project should get them without Rust or a project engine. They were specced inside `native`; the native review (docs/research/dx.md, 2026-10-20) split them out because they ship in the prebuilt engine and share nothing with project engines or plugin adoption.

## What Changes

- Script materials: a generic material in the prebuilt engine whose WGSL, parameter layout and textures are declared in TS at runtime, laid out with WGSL uniform rules and hot reloaded with the shaders. Render graph nodes, passes and compute stay Rust.
- TypeGPU (`typegpu` 0.12.8) evaluated as the layout vocabulary and the shader language: borrow its ideas, do not adopt it (design.md, decision 2).

## Capabilities

### New Capabilities
- `script-materials`: runtime-defined materials declared in TS and rendered by Bevy.

### Modified Capabilities

None.

## Impact

- New crate `bevy-ts-material`, in the prebuilt engine; the default engine grows by it only.
- New TS module `bevy-ts/material`.
