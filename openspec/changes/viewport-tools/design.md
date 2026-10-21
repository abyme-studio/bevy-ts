## Context

Edit mode (`crates/bevy-ts-viewport/src/edit.rs`) already runs Bevy 0.20's `TransformGizmoPlugin`. `edit_keys` sets `TransformGizmoSettings.mode` and `space` from `W E R X`. The settings also have `snap_translate`, `snap_rotate` (radians) and `snap_scale`, unused today. `set_mode` answers `{ mode, ui, gamepadUnfocused, paused }`; nothing streams it. The selection already streams through `viewport_selection+watch` with an `origin`. abyme's `bevy-viewport` team builds the toolbar and the "no camera" message against this; the protocol below is agreed with spec@bevy-viewport.

## Goals / Non-Goals

**Goals:** set and watch mode, space and snap; an orientation gizmo; tell the viewer when play has no camera.

**Non-Goals:** configurable snap steps, a snap key, a camera hint overlay, any engine-drawn "no camera" text.

## Decisions

- **One owner for the tools: a `ViewportTools` resource.** It holds mode, space and snap, and is the only thing that writes `TransformGizmoSettings` (mode, space and the three snap fields). The method and `edit_keys` both write `ViewportTools` with an origin; a change-detection system copies it to the settings and queues the watch message. Alternative: treat `TransformGizmoSettings` as the state. Rejected: it has no origin, and the `locked`-field logic in `edits.rs` already reads it, so one writer keeps them in step.
- **Snap uses Bevy's own snapping.** `snap: true` sets `snap_translate = 1`, `snap_rotate = 15°` in radians, `snap_scale = 0.1`. An axis drag snaps the delta along the axis; that is Bevy's rule and the spec's scenario.
- **Watch streams follow `selection_watch`.** `viewport_tools+watch` and `viewport_mode+watch` are `add_watching_method` handlers that send on open and then on a change counter, as the selection stream does.
- **The orientation gizmo is a second small camera.** An `EditorOnly` camera with `Camera.viewport` set to the corner square (96 logical px, inset 8, scaled by the viewport's `scale`), an order above the editor camera, no clear, rendering only layer 30. `EditorGizmos`-style line and sphere gizmos on layer 30 draw the axes at the origin; each frame the camera's rotation copies the editor camera's and sits a fixed distance back. Layer 30 is free: 15 is Bevy's gizmo overlay, 20-29 previews, 31 the editor. Alternative: draw it with screen-space 2D gizmos. Rejected: projecting the axes by hand duplicates what a camera does, and a camera keeps it right under any FOV.
- **Its input is hit-tested in Rust before picking.** A press inside the square is claimed: `click_to_select` and the transform gizmo skip it. On release with no drag, the nearest axis end within 12 logical px (projected through the corner camera) wins; the editor camera's yaw and pitch are set to look down it at `focus`, keeping distance. A drag from the square feeds the existing orbit path.
- **`camera` is computed, not tracked by events.** Each frame in play, `camera` is whether any active `Camera` not `EditorOnly` or `PreviewOnly` targets the primary window. A change bumps the mode stream's counter. Edit mode is always `true`.

## Risks / Trade-offs

- [The corner camera costs a render pass in edit mode] → No clear, a 96 px viewport and a handful of lines; measured in tasks against edit mode without it, and the operator's go asked before build.
- [A press in the corner hides a gizmo handle or entity under it] → The square is small and in a corner; frame or orbit to reach what is under it.
- [engine-components and asset-catalog also bump the protocol] → This takes the next free minor on main when it lands (planned 1.12), rebases and regenerates types; the number is not pinned before then.
