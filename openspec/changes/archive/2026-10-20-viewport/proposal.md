## Why

The operator ruled (2026-10-19) that the game draws inside abyme's macOS window from the start, in a native hole behind the page, as abyme's browser view already does, and that the edit viewport is the engine itself in edit mode. `bridge` gives the game a window of its own and a protocol, and `tsx` maps every entity back to its TSX element, but nothing lets the engine render into another process's view, take input from it, or edit the world with a camera, gizmos and picking. This change specs the bevy-ts half of that: the shared GPU surface, the edit mode, and protocol 1.1.

## What Changes

- A viewport mode for the engine: no window of its own. Cameras render into a ring of three IOSurface-backed Metal textures the engine owns, wrapped for wgpu through `wgpu-hal`'s Metal interop; each finished frame is handed to the viewer by slot, after the GPU has completed it. Surfaces cross to the viewer as Mach ports over a Mach bootstrap service per viewport, the channel abyme's browser host already uses.
- A virtual primary window: the engine spawns a Bevy `Window` with no native handle, sized to the hole, whose swap chain is the ring. Game code, `bevy_ui`, `bevy_picking` and Bevy's own transform gizmo see a normal primary window.
- Input forwarded from the viewer (pointer, wheel and gestures, keys, IME, focus) and fed into Bevy as the messages winit would send, so game input code runs unchanged.
- Frame pacing driven by the viewer's display link (a `tick` per vsync of the hole's display), resize by a rounded-up ring and a crop, and an idle edit mode that renders only when something changed.
- Edit mode: the game paused (`bevy-ts/pause`), an editor camera that starts at the game camera's pose, engine-drawn gizmos on an editor-only render layer, selection outlines, Bevy 0.20's `TransformGizmoPlugin`, and picking that selects a prefab instance root (⌥ for the exact entity).
- Edits from the viewport: a gizmo drag changes the world live, and on release becomes a `ViewportEdit` routed by provenance: a `.bsn` level override (written by bevy-ts through the coming `levels` change), a TSX prop (rewritten by the editor, which judges whether it is a literal), refused (bound props), or transient (entities no source describes).
- Protocol 1.1 (a minor bump of `protocol/bevy-ts.openrpc.json`): `bevy-ts/viewport_attach`, `viewport_detach`, `viewport_resize`, `viewport_input`, `viewport_mode`, `viewport_pick`, `viewport_select`, `viewport_selection+watch`, `viewport_edits+watch`, `viewport_edit_result`, `viewport_revert`, `viewport_camera`, `viewport_stats`, `assets`, `assets+watch`, `asset_thumbnail`.
- `bevy-ts dev` keeps its window outside abyme; `--viewport` starts windowless and waits for a viewer. Edit mode works in both.
- Out of scope: abyme's half (listed in design.md for its own change), streamed frames to remote devices, platforms other than macOS, multiple simultaneous viewports, multi-selection drags, the `.bsn` format and writer (the `levels` change), source rewriting of TSX (the editor's).

## Capabilities

### New Capabilities
- `shared-surface`: viewport mode, the IOSurface ring, the Mach channel and how surfaces are handed over, resize, frame pacing, GPU and cross-process sync, the virtual window, and the windowed fallback.
- `viewport-input`: input events from the viewer, their mapping onto Bevy's input messages, focus, cursor and IME feedback to the viewer.
- `edit-mode`: play and edit modes, the editor camera, engine-drawn gizmos, picking and selection, and selection sync with the editor.
- `viewport-edits`: gizmo drags producing edits, routing an edit to a `.bsn` override, a TSX prop, a refusal or a transient change, and the editor's answer.
- `asset-list`: listing the project's assets, watching them change, and thumbnails for the asset viewer.

### Modified Capabilities

None. `bridge` and `tsx` are not archived; what this change needs from them is written in design.md, "Depends on bridge and tsx".

## Impact

- New crate `bevy-ts-viewport` (surface ring, Mach channel, virtual window, input mapping, edit mode, picking and selection, edits, assets, protocol methods), macOS only behind `cfg(target_os = "macos")`, depending on `bevy-ts`, `bevy-ts-remote` and `bevy-ts-tsx`. `bevy-ts-cli` gains `--viewport`.
- Dependencies: `wgpu-hal` 30 with the `metal` feature (as Bevy 0.20's `wgpu` 30 already pulls), `objc2-metal`, `objc2-io-surface`, `objc2-core-foundation` (the `objc2` family `wgpu-hal` already uses), Bevy 0.20 features `bevy_picking` (mesh picking), `bevy_gizmos` (with the transform gizmo).
- Protocol 1.0 → 1.1: methods added only. A game without `bevy-ts-viewport` answers them with method-not-found.
- abyme's Bevy app gets a list of what it must provide (design.md, "What abyme must provide") for a second change in the abyme repo.
