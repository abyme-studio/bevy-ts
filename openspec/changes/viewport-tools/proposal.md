## Why

The studio's viewport has no tool strip: the operator can pick move, rotate, scale, local or world only by keys they cannot see, and snap not at all. There is no orientation gizmo. The bevy editor study (2026-10-22, change 7) asks the engine for these so abyme's `bevy-viewport` team can build the toolbar. Each piece is one Bevy has or its editor work discusses (operator rule 2026-10-22; see design). The protocol is agreed with spec@bevy-viewport.

## What Changes

- **`bevy-ts/viewport_tools`** `{ mode?, space?, snap? }` sets the transform gizmo's mode (`translate`, `rotate`, `scale`), space (`world`, `local`) and snap on or off. It answers `ViewportTools { mode, space, snap, steps }`, with fixed steps as Bevy's editor prototype sets them: translate 0.5, rotate 15 (degrees), scale 0.1.
- **`bevy-ts/viewport_tools+watch`** sends `ViewportTools` on open and on every change, from the method (`origin: "editor"`) or from `W E R X` in the viewport (`origin: "viewport"`), so the strip follows the keys. No snap key.
- **Orientation gizmo.** In edit mode the engine draws an axis gizmo in the game viewport's top-left corner, as Bevy's editor prototype does. A click on an axis end turns the editor camera to look down that axis. Clicks there never pick. Input comes through `viewport_input`; no new method.
- **Protocol: the next free minor on main when this lands** (planned 1.12, after engine-components), additions only.
- **Out of scope:**
  - abyme's toolbar (team `bevy-viewport`);
  - configurable snap steps; a snap key; dragging the orientation gizmo to orbit;
  - a "no camera" signal in play. Cut: Bevy's editor neither has nor discusses one, and its roadmap has no play-in-editor yet.

## Capabilities

### New Capabilities

None.

### Modified Capabilities

- `edit-mode`: the gizmo's mode, space and snap are set and watched over the protocol; an orientation gizmo.

## Impact

- `crates/bevy-ts-viewport`: `edit.rs` (tool state, keys, orientation gizmo and its input), `methods.rs` (two methods), `tests/edit.rs`.
- `protocol/bevy-ts.openrpc.json`: the next free minor, regenerated Rust and TS types. engine-components (planned 1.11) and asset-catalog (planned 1.13) also bump; lead@bevy-ts names the version this lands as.
- `docs/viewport.md`: the tools and the orientation gizmo, and a note for abyme's studio.
- Performance: one more small camera render (the orientation gizmo's corner) per frame, in edit mode only, while the game viewport is visible. Nothing in play. This needs the operator's go before build (performance is on the ask-first list); the cost is measured in tasks.
- abyme: `bevy-viewport` builds the toolbar against this. It does not fall back: an engine older than the version this lands as gets a "bevy-ts too old" view.
