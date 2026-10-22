## MODIFIED Requirements

### Requirement: Edit mode and the protocol work in windowed mode
Every viewport method except `viewport_attach`, `viewport_detach`, `viewport_resize` and `viewport_input` SHALL work the same in windowed mode, against the engine's own window. Under `--headless`, `viewport_input` SHALL be served for `pointer` events alone, as the viewport-input spec's headless pointer says; the other viewport methods are not served headless.

#### Scenario: Edit mode without abyme
- **WHEN** a client calls `bevy-ts/viewport_mode` with `edit` against `bevy-ts dev` in its own window
- **THEN** the game pauses and the editor camera, gizmos and picking work in that window

#### Scenario: Pointer input headless
- **WHEN** a client calls `bevy-ts/viewport_input` with a `pointer` `move` against `bevy-ts dev --headless`
- **THEN** it answers `{ queued: 1 }`, and `bevy-ts/viewport_attach` there is still not served
