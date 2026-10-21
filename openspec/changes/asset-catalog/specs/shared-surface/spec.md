## MODIFIED Requirements

### Requirement: The viewport in the protocol contract
The viewport methods (`bevy-ts/viewport_attach`, `viewport_detach`, `viewport_resize`, `viewport_input`, `viewport_mode`, `viewport_camera`, `viewport_pick`, `viewport_select`, `viewport_selection+watch`, `viewport_edits+watch`, `viewport_edit_result`, `viewport_revert`, `viewport_stats`, `assets`, `assets+watch`, `asset_thumbnail`) and the channel's message types (`hello`, sent by both sides, `input`, carrying one `ViewportInput` event, `surface`, `frame`, `tick`, `cursor`, `cursor_grab`, `cursor_warp`, `ime_area`, `ime_allowed`, `replaced`) SHALL be added to `protocol/bevy-ts.openrpc.json` with `x-bevy-ts-crate: "bevy-ts-viewport"`, as an additions-only minor bump of the contract (1.2.0 when it landed; 1.5.0 with the `display` and level-route fields; 1.10.0 with the `hello` answer, `replaced`, `touch` and `gamepad`; 1.17.0 with `AssetInfo.thumbnail`, `PickHit.point` and `ViewportPickResult.ground`), generated into Rust and TS types by bridge's pipeline. Channel messages SHALL be JSON-RPC 2.0 notifications whose `method` is the message type, so one schema covers HTTP and the channel. `bevy-ts/hello` SHALL report `viewport: true|false`.

#### Scenario: An old client
- **WHEN** a 1.0 client connects to an engine serving the viewport methods
- **THEN** every 1.0 method answers as before

#### Scenario: A game built without the viewport crate
- **WHEN** a client calls `bevy-ts/viewport_attach` on it
- **THEN** it gets method-not-found

#### Scenario: A studio on an older engine
- **WHEN** a client of this version lists assets from a 1.10 engine
- **THEN** no asset has `thumbnail`, and the client asks for thumbnails of images and meshes only, as before
