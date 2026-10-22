## MODIFIED Requirements

### Requirement: Viewport mode runs the engine with no window of its own
`bevy-ts dev --viewport <entry>` SHALL start the engine with no native window and no winit event loop, load the game as `bevy-ts dev` does, and wait for a viewer to attach. Simulation SHALL run from start, unless `--edit` is given (see edit-mode); nothing SHALL be rendered until a viewer is attached and visible. `bevy-ts dev` without `--viewport` SHALL keep bridge's behaviour (a window of its own). Viewport mode SHALL be available on macOS only; on other platforms `--viewport` SHALL fail with a message naming the platform.

#### Scenario: Started by abyme
- **WHEN** abyme starts `bevy-ts dev --viewport examples/cube/src/game.ts`
- **THEN** no window or Dock icon appears, `bevy-ts/hello` answers with `viewport: true`, and the cube's systems run

#### Scenario: Started by hand outside abyme
- **WHEN** a developer runs `bevy-ts dev examples/cube/src/game.ts` in a terminal
- **THEN** a window of the engine's own opens, as in bridge

#### Scenario: Started in edit mode by abyme
- **WHEN** abyme starts `bevy-ts dev --viewport --edit examples/cube/src/game.ts`
- **THEN** no window appears, `bevy-ts/hello` answers with `viewport: true`, and the cube's systems do not run until a client sets `play`

### Requirement: The viewport in the protocol contract
The viewport methods (`bevy-ts/viewport_attach`, `viewport_detach`, `viewport_resize`, `viewport_input`, `viewport_mode`, `viewport_camera`, `viewport_pick`, `viewport_select`, `viewport_selection+watch`, `viewport_edits+watch`, `viewport_edit_result`, `viewport_revert`, `viewport_stats`, `assets`, `assets+watch`, `asset_thumbnail`) and the channel's message types (`hello`, sent by both sides, `input`, carrying one `ViewportInput` event, `surface`, `frame`, `tick`, `cursor`, `cursor_grab`, `cursor_warp`, `ime_area`, `ime_allowed`, `replaced`) SHALL be added to `protocol/bevy-ts.openrpc.json` with `x-bevy-ts-crate: "bevy-ts-viewport"`, as an additions-only minor bump of the contract (1.2.0 when it landed; 1.5.0 with the `display` and level-route fields; 1.10.0 with the `hello` answer, `replaced`, `touch` and `gamepad`; 1.16.0 with `AssetInfo.thumbnail`, `PickHit.point` and `ViewportPickResult.ground`; the next free minor with `ViewportModeResult.started`), generated into Rust and TS types by bridge's pipeline. Channel messages SHALL be JSON-RPC 2.0 notifications whose `method` is the message type, so one schema covers HTTP and the channel. `bevy-ts/hello` SHALL report `viewport: true|false`.

#### Scenario: An old client
- **WHEN** a 1.0 client connects to an engine serving the viewport methods
- **THEN** every 1.0 method answers as before

#### Scenario: A game built without the viewport crate
- **WHEN** a client calls `bevy-ts/viewport_attach` on it
- **THEN** it gets method-not-found

#### Scenario: A studio on an older engine
- **WHEN** a client of this version lists assets from a 1.10 engine
- **THEN** no asset has `thumbnail`, and the client asks for thumbnails of images and meshes only, as before

#### Scenario: A studio on an engine without `started`
- **WHEN** a client of this version reads `viewport_mode {}` from a 1.17 engine
- **THEN** the answer has no `started`, and the client treats the game as started
