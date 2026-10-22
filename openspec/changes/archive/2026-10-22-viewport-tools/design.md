## Context

Edit mode (`crates/bevy-ts-viewport/src/edit.rs`) already runs Bevy 0.20's `TransformGizmoPlugin`. `edit_keys` sets `TransformGizmoSettings.mode` and `space` from `W E R X`. The settings also have `snap_translate`, `snap_rotate` (radians) and `snap_scale`, unused today. The selection already streams through `viewport_selection+watch` with an `origin`. abyme's `bevy-viewport` team builds the toolbar against this; the protocol is agreed with spec@bevy-viewport.

## Bevy precedent

Operator rule (2026-10-22): bevy-ts builds what Bevy has or its editor work discusses. Checked against Bevy 0.20 and `bevy_editor_prototypes` at `55723a3` (2026-04-14).

| Piece | Where Bevy has it | Verdict |
|---|---|---|
| Mode set from a toolbar | `bevy_gizmos` 0.20 `transform_gizmo.rs` module doc: "users set `TransformGizmoSettings::mode` however they like (keyboard shortcuts, UI buttons, …)" | keep |
| Local or world space | `bevy_gizmos` 0.20 `TransformGizmoSpace`; prototype `bevy_transform_gizmos` `alignment_rotation` | keep |
| Snap on or off, fixed steps | `bevy_gizmos` 0.20 `snap_translate`, `snap_rotate`, `snap_scale`; prototype `bevy_transform_gizmos` `snap_enabled` with steps 0.5, 15°, 0.1 | keep, steps as the prototype |
| Orientation gizmo | prototype `bevy_editor_panes/bevy_3d_viewport/src/view_gizmo.rs`: three axes with end spheres, a camera mirroring `EditorCam`, top-left | keep, top-left |
| Click an axis to look down it | the same file: "TODO react to click on the spheres to snap camera to axis"; `bevy_editor_cam` `extensions/look_to.rs` (`LookToTrigger`, used per axis in `examples/cad.rs`) | keep |
| Drag the gizmo to orbit | not in either | cut |
| "No camera" in play | not in either; the prototype roadmap has no play-in-editor | cut |

## Goals / Non-Goals

**Goals:** set and watch mode, space and snap; an orientation gizmo that turns the camera down an axis.

**Non-Goals:** configurable snap steps, a snap key, orbiting from the gizmo, a camera hint overlay, a "no camera" signal.

## Decisions

- **One owner for the tools: a `ViewportTools` resource.** A change during a gizmo drag is copied to the settings on release. It holds mode, space and snap, and is the only thing that writes `TransformGizmoSettings` (mode, space and the three snap fields). The method and `edit_keys` both write `ViewportTools` with an origin; a change-detection system copies it to the settings and queues the watch message. Alternative: treat `TransformGizmoSettings` as the state. Rejected: it has no origin, and the `locked`-field logic in `edits.rs` already reads it, so one writer keeps them in step.
- **Snap uses Bevy's own snapping.** `snap: true` sets `snap_translate = 0.5`, `snap_rotate = 15°` in radians, `snap_scale = 0.1`. An axis drag snaps the delta along the axis; that is Bevy's rule and the spec's scenario.
- **The watch stream follows `selection_watch`.** `viewport_tools+watch` is an `add_watching_method` handler that sends on open and then on a change counter, as the selection stream does.
- **The orientation gizmo is a second small camera rendering to a texture, as the prototype's is, redrawn only on change** (operator ruling 2026-10-22). An `EditorOnly` orthographic `Camera3d`, order -11 (before the editor camera), clearing to transparent, renders layer 32 into one texture, the 96 logical px square at 3x (288 px), made at startup and never swapped: a new render-target texture is taken up some frames late, more under load, and one resized in place not at all. The quad shows it at any window scale up to 3. The camera is active only on the two frames after the editor camera's rotation or projection changed: gizmo lines reach the render a frame after they are drawn. Once a process, on its first 1.5 s of edit mode, it draws every frame: Bevy compiles its pipelines in the background, and a camera on for only two frames under load drew nothing. The axes are projected through the editor camera's rotation in Rust and drawn flat: three unlit discs for the filled `+` ends, gizmo lines and circles for the axes and hollow `-` ends, so the hit test uses the same projection. The editor view shows the texture on an unlit quad, a child of the editor camera just past its near plane over the corner, on the editor's layer 31: one draw a frame when the view is still. The prototype shows its texture in a UI node; bevy-ts-viewport has neither Bevy's UI render nor its 2D render, and either would be a new dependency. No light shares layer 32, so Bevy builds no shadow maps for the small camera (`bevy_pbr` skips a directional light whose layers miss the view's). Layer 32, not 30: 30 is the asset thumbnails' layer (15 Bevy's gizmo overlay, 20-29 previews, 31 the editor). It lives in `orient.rs`, added only in viewport mode, so the engine's own window never has it.
- **Its input is hit-tested in Rust before picking.** A press inside the square is claimed: `click_to_select` and the transform gizmo skip it. On release with no drag, the nearest axis end within 12 logical px (projected as drawn) wins; the editor camera's yaw and pitch are set to look down it at `focus`, keeping distance; `±Y` stops at the orbit's pitch limit (one `PITCH_LIMIT` for orbit, fly and previews).

## Risks / Trade-offs

- [The corner camera costs a render pass in edit mode] → No clear, a 96 px viewport and a handful of lines; measured in tasks against edit mode without it, and the operator's go asked before build.
- [A press in the corner hides a gizmo handle or entity under it] → The square is small and in a corner; frame or orbit to reach what is under it.
- [engine-components and asset-catalog also bump the protocol] → This takes the next free minor on main when it lands (planned 1.12), rebases and regenerates types; the number is not pinned before then.
