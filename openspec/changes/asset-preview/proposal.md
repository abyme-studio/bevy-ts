## Why

abyme's Bevy studio previews a 3D asset with three.js (abyme's generic glTF viewer), so a model is shown by a renderer that is not Bevy's: its materials, lighting and tonemapping differ from the game's. The engine already renders assets offscreen, but only as a still thumbnail (`asset_thumbnail`), and it serves one viewport at a time, so it cannot show a live, orbitable preview beside the game. The operator asked (2026-10-20) for the engine to preview assets itself, with three.js kept until this lands.

## What Changes

- **Several viewports per engine.**
  - `bevy-ts/viewport_attach` takes a `view`: `"game"` (today's viewport, the default) or a preview id. It answers a `viewport` id with its own Mach service, ring, pacing and input.
  - Viewports of different views run together. A second attach of the same view still replaces the first.
  - `viewport_detach`, `viewport_resize` and the other viewport methods take the id. Without one they mean the game viewport, so 1.x clients are unchanged.
  - The surface protocol's `layer` field keeps its meaning: images stacked inside one viewport.
- **`bevy-ts/asset_preview`.**
  - It loads an asset into an isolated preview scene, with its own render layer, camera and lights and preview-only entities, and answers a preview id that a viewer attaches to.
  - The preview camera orbits, zooms and frames from the viewport's input. Animations are listed and played with `asset_preview_control`, and `asset_preview_close` ends it.
  - Preview entities never reach TS systems, `tsx_tree`, `entities+watch` or `.bsn` writing, as editor-only entities do not.
- **Preview mode.**
  - `bevy-ts preview --assets <dir>` starts the prebuilt engine in viewport mode with no game: no TS loaded and no project required.
  - It serves `hello` (`mode: "preview"`), `assets`, `assets+watch`, `asset_thumbnail`, `asset_preview` and the viewport methods.
  - abyme's studio starts it as a process when it needs a preview and no game is running.
- **Protocol:** 1.7.0, additions only, generated into Rust and TS types as before.
- **Out of scope:**
  - abyme's side: the studio's asset view and `viewers` choosing the engine preview when one is available, which is a follow-up in abyme's `bevy-studio`.
  - Removing three.js, which stays as abyme's generic viewer and the fallback with no engine.
  - Previewing audio, fonts or materials as live scenes.

## Capabilities

### New Capabilities
- `asset-preview`: live, orbitable previews of meshes and glTF scenes rendered by the engine in an isolated scene, and preview mode, the engine with no game.

### Modified Capabilities
- `shared-surface`: attaching a viewer names a view, and an engine serves several viewports at once.

## Impact

- `crates/bevy-ts-viewport`:
  - viewports keyed by id (`methods.rs`, `mach.rs`, `ring.rs`, `shared.rs`, `surface.rs`);
  - a preview scene module next to `assets.rs`, reusing its loading and framing.
- `crates/bevy-ts-cli`: the `preview` subcommand.
- `protocol/bevy-ts.openrpc.json`: 1.7.0.
- GPU and CPU: one more camera render per attached, visible preview, paced by its viewer, and nothing while hidden or detached. The cost is measured in tasks (operator asks first on performance).
- abyme: none in this change; `bevy-studio` takes it up after.
