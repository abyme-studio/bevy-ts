## MODIFIED Requirements

### Requirement: Attaching a viewer
`bevy-ts/viewport_attach` (`w`, `h` in points, `scale`, `hz`, optional `display`, optional `view`) SHALL create a viewport for `view`, which is `"game"` (the default) or `{ preview: <id> }` from `bevy-ts/asset_preview`. It SHALL check in a Mach bootstrap service with a name unique to the process and the attach (`dev.bevy-ts.viewport.<pid>.<n>`), and answer `{ viewport, service, protocol }`. Each viewport SHALL have its own service, ring of surfaces, pacing, input queue and camera target, and viewports of different views SHALL run at the same time. A second attach of a view that already has a viewport SHALL replace that viewport: its old service is ended and its surfaces released, and other views are untouched. `bevy-ts/viewport_detach` (optional `viewport`, default the game viewport) SHALL end that service and release its ring; the game keeps running. A viewer whose reply port dies SHALL be detached as if it had called `viewport_detach` for its viewport. `viewport_resize`, `viewport_input`, `viewport_camera` and `viewport_stats` SHALL take an optional `viewport`, defaulting to the game viewport. The virtual primary window, edit mode, picking and edits SHALL belong to the game viewport alone. A viewport SHALL render only while attached and visible.

#### Scenario: Attach then look up
- **WHEN** a client calls `bevy-ts/viewport_attach` with `w: 800, h: 600, scale: 2, hz: 120`
- **THEN** it gets a viewport id and a service name, and a process in the same GUI bootstrap domain that looks it up and sends `hello` with a reply port receives three `surface` messages of 1600×1200 or larger, then `frame` messages

#### Scenario: The viewer quits
- **WHEN** the viewer's process exits while attached
- **THEN** the engine detaches that viewport within one frame, stops rendering it and keeps simulating

#### Scenario: A preview beside the game
- **WHEN** a client has the game viewport attached and attaches a second viewport with `view: { preview: 3 }`
- **THEN** both services deliver frames at their own viewers' pace, input sent to the preview's channel moves only the preview camera, and the game viewport's frames are unchanged

#### Scenario: A 1.7 client
- **WHEN** a client calls `viewport_attach` without `view`, then `viewport_resize` without `viewport`
- **THEN** both act on the game viewport, as before
