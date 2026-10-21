## Why

Custom materials are data (WGSL plus parameters), so a project should get them without Rust or a project engine. They were specced inside `native`; the native review (docs/research/dx.md, 2026-10-20) split them out because they ship in the prebuilt engine and share nothing with project engines or plugin adoption.

## What Changes

- Script materials: a generic material in the prebuilt engine whose parameter layout and textures are declared in TS at runtime and whose shaders are `.wgsl` files beside the game's TS, imported as `import glow from "./glow.wgsl"` (operator ruling 2026-10-21: shaders are files, not TS strings). Params are laid out with WGSL uniform rules; shaders hot reload in place; WGSL errors name the material and the line in the author's file. Render graph nodes, passes and compute stay Rust.
- TypeGPU (`typegpu` 0.12.8) evaluated as the layout vocabulary and the shader language: borrow its ideas, do not adopt it (design.md, decision 2).

## Capabilities

### New Capabilities
- `script-materials`: runtime-defined materials declared in TS and rendered by Bevy.

### Modified Capabilities
- `script-runtime`: the module loader resolves a relative `.wgsl` import to a watched shader-file value.

## Impact

- New crate `bevy-ts-material`, in the prebuilt engine; the default engine grows by it only.
- New TS module `bevy-ts/material`.
- The script loader (script-runtime) resolves a relative `.wgsl` import and watches the file; the bundled types gain a `*.wgsl` module declaration.
