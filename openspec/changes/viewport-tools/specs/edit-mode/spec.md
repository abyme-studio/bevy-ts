## ADDED Requirements

### Requirement: Gizmo tools over the protocol
`bevy-ts/viewport_tools` (`mode?`: `translate`, `rotate` or `scale`; `space?`: `world` or `local`; `snap?`: boolean) SHALL set the transform gizmo's mode, space and snap and answer `ViewportTools`: `mode`, `space`, `snap`, and `steps` (`translate: 0.5`, `rotate: 15` in degrees, `scale: 0.1`). With no params it SHALL answer the tools unchanged. With `snap: true` a drag SHALL move, turn or scale in whole steps of the mode's step; with `snap: false` it SHALL not snap. The tools SHALL start as `translate`, `world`, snap off, and SHALL be kept for the process across mode switches and reloads. The method SHALL be served in play too; the tools act in edit mode. `bevy-ts/viewport_tools+watch` SHALL send `ViewportTools` on open and on every change, with `origin: "editor"` for a change by the method and `origin: "viewport"` for one by the `W`, `E`, `R` or `X` key. No key SHALL toggle snap.

#### Scenario: Picking rotate from the toolbar
- **WHEN** enemy `b` is selected in edit mode and the editor calls `bevy-ts/viewport_tools` with `mode: "rotate"`
- **THEN** the gizmo shows rotation rings, the answer has `mode: "rotate"`, and the watch stream sends it with `origin: "editor"`

#### Scenario: A key changes the strip
- **WHEN** the viewer sends `KeyX` with the tools in `world` space
- **THEN** the watch stream sends `space: "local"` with `origin: "viewport"`

#### Scenario: Snapped move
- **WHEN** snap is on and the operator drags enemy `b`'s x handle from 3 by 1.4 units
- **THEN** enemy `b`'s x is 4.5 during the drag and in the edit on release

### Requirement: Orientation gizmo
In edit mode the engine SHALL draw, in the game viewport only, an orientation gizmo in the top-left corner: a 96 logical px square inset 8 px from the top and left edges, showing the world's axes as the editor camera sees them, with `+X` red, `+Y` green and `+Z` blue as filled ends and the negative axes as hollow ends. It SHALL NOT be drawn in play, in asset previews or in the engine's own window. A click with no drag on an axis end SHALL turn the editor camera to look down that axis at its focus point, keeping its distance from the focus. A press in the square SHALL NOT pick, change the selection or start a gizmo drag.

#### Scenario: Looking down from above
- **WHEN** in edit mode the operator clicks the gizmo's `+Y` end
- **THEN** the editor camera looks straight down at its focus point from the same distance, and the selection is unchanged

#### Scenario: Play hides it
- **WHEN** the client switches to `play`
- **THEN** no orientation gizmo is drawn and a click in the top-left corner reaches the game
