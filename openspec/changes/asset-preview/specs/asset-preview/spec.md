## ADDED Requirements

### Requirement: Previewing an asset
`bevy-ts/asset_preview` (`path`) SHALL load a mesh or glTF scene and build a preview scene for it, answering `{ preview, kind, animations }`, or `pending: true` while the asset loads (the client asks again). The scene SHALL have its own render layer from a pool of ten (layers 20 to 29), a root far from the game's origin, a camera that targets the preview's viewport and frames the scene's bounds, and a light rig visible only on that layer. When a game runs, the camera SHALL use its main camera's tonemapping, exposure and environment lighting. An eleventh preview SHALL close the least recently attached. A path that is not a mesh or glTF scene SHALL answer an error naming the kind.

#### Scenario: Previewing a model beside the running game
- **WHEN** the studio calls `asset_preview` with `models/grunt.glb` while the arena runs, then attaches a viewport with `view: { preview: <id> }`
- **THEN** it receives frames of the grunt model lit like the game, and the arena's frames, entities and TS systems are unchanged

### Requirement: Preview entities are invisible to the game and the protocol
Every entity of a preview scene SHALL carry `PreviewOnly`, SHALL render only on the preview's layer, SHALL NOT match TS system queries, and SHALL be absent from `bevy-ts/tsx_tree`, `bevy-ts/entities+watch` and `.bsn` writing. A preview SHALL have no colliders.

#### Scenario: Listing entities with a preview open
- **WHEN** a client lists entities through `bevy-ts/entities+watch` while a preview is open
- **THEN** none of the preview's entities are in it

### Requirement: Orbiting and animating a preview
Input on a preview's viewport SHALL be handled in Rust with no JS call: a drag orbits the camera around the scene's centre, the wheel or a pinch zooms, and `f` reframes. `bevy-ts/asset_preview_control` (`preview`, optional `animation`, `play`, `speed`) SHALL play, pause or switch the scene's glTF animations. `bevy-ts/asset_preview_close` (`preview`) SHALL despawn the scene and free its layer. Detaching a preview's viewport SHALL keep its scene for 30 seconds before closing it.

#### Scenario: Playing an animation
- **WHEN** a preview of a glTF with animations `["idle", "walk"]` gets `asset_preview_control { animation: "walk", play: true }`
- **THEN** the next frames show the walk cycle, and no other entity's animation changes

### Requirement: Preview mode needs no game
`bevy-ts preview --assets <dir>` SHALL start the prebuilt engine in viewport mode with no entry module and no TS loaded, with `<dir>` as the asset root and Bevy's file watcher on. It SHALL answer `bevy-ts/hello` with `mode: "preview"` and `viewport: true`, serve `assets`, `assets+watch`, `asset_thumbnail`, `asset_preview`, `asset_preview_control`, `asset_preview_close` and the viewport methods, and answer method-not-found for methods that need a game (`reload`, `tsx_*`, `level_*`, `viewport_mode`, `viewport_pick`, `viewport_select`, `viewport_edits+watch`). With no viewport attached it SHALL render nothing.

#### Scenario: A modeller drops a file in, with no game
- **WHEN** abyme starts `bevy-ts preview --assets ./assets` and a new `models/tree.glb` appears
- **THEN** `assets+watch` reports it, and `asset_preview` on it gives frames of the tree in a viewport

### Requirement: Asset preview in the protocol contract
`asset_preview`, `asset_preview_control` and `asset_preview_close`, `viewport_attach`'s `view` and its answer's `viewport`, the optional `viewport` on the viewport methods, and `hello`'s `mode` SHALL be added to `protocol/bevy-ts.openrpc.json` with `x-bevy-ts-crate: "bevy-ts-viewport"` as an additions-only minor bump to 1.7.0, generated into Rust and TS types by bridge's pipeline.

#### Scenario: An engine without preview support
- **WHEN** a client calls `bevy-ts/asset_preview` on a 1.6 engine
- **THEN** it gets method-not-found, and the studio keeps showing the asset with three.js
