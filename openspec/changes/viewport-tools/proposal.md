## Why

The studio's viewport has no tool strip: the operator can pick move, rotate, scale, local or world, and snap only by keys they cannot see, and snap not at all. There is no orientation gizmo, and a game with no camera shows a blank viewport with no reason given. The bevy editor study (2026-10-22, change 7) asks the engine for these so abyme's `bevy-viewport` team can build the toolbar. The protocol was agreed with spec@bevy-viewport.

## What Changes

- **`bevy-ts/viewport_tools`** `{ mode?, space?, snap? }` sets the transform gizmo's mode (`translate`, `rotate`, `scale`), space (`world`, `local`) and snap on or off. It answers `ViewportTools { mode, space, snap, steps }`, with fixed steps: translate 1, rotate 15 (degrees), scale 0.1.
- **`bevy-ts/viewport_tools+watch`** sends `ViewportTools` on open and on every change, from the method (`origin: "editor"`) or from `W E R X` in the viewport (`origin: "viewport"`), so the strip follows the keys. No snap key.
- **Orientation gizmo.** In edit mode the engine draws an axis gizmo in the game viewport's top-right corner. A click on an axis end turns the editor camera to look down that axis; a drag from it orbits. Clicks there never pick. Input comes through `viewport_input`; no new method.
- **"No camera".** `ViewportModeResult` gains `camera: bool`: false in play when no active game camera renders to the primary window. **`bevy-ts/viewport_mode+watch`** sends the full `ViewportModeResult` on open and on every change. The engine draws nothing for it; the studio shows the message.
- **Protocol 1.11.0**, additions only.
- **Out of scope:** abyme's toolbar and "no camera" message (team `bevy-viewport`); configurable snap steps; a snap key; a camera hint overlay.

## Capabilities

### New Capabilities

None.

### Modified Capabilities

- `edit-mode`: the gizmo's mode, space and snap are set and watched over the protocol; an orientation gizmo; the mode result says whether play has a camera, and is watched.

## Impact

- `crates/bevy-ts-viewport`: `edit.rs` (tool state, keys, orientation gizmo and its input, camera presence), `methods.rs` (three methods), `tests/edit.rs`.
- `protocol/bevy-ts.openrpc.json`: 1.11.0, regenerated Rust and TS types. asset-catalog also bumps from 1.10; whichever lands second takes the next minor.
- `docs/viewport.md`: the tools, the orientation gizmo and `camera`, and a note for abyme's studio.
- Performance: one more small camera render (the orientation gizmo's corner) per frame, in edit mode only, while the game viewport is visible. Nothing in play. This needs the operator's go before build (performance is on the ask-first list); the cost is measured in tasks.
- abyme: `bevy-viewport` builds the toolbar and the message against this, gated on `hello.protocol >= 1.11`.
