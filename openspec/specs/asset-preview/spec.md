# asset-preview Specification

## Purpose
A mesh or glTF scene from the game's assets, shown in a viewport of its own while the game runs: its own render layer, camera and light rig, lit like the game, orbited and animated in Rust, and invisible to the game's systems, its entity lists and the protocol's entity methods.

## Requirements

### Requirement: Previewing an asset
`bevy-ts/asset_preview` (`path`) SHALL load a mesh or glTF scene and build a preview scene for it, answering `{ preview, kind, animations }`, or `pending: true` while the asset loads (the client asks again). The scene SHALL have its own render layer from a pool of ten (layers 20 to 29), a root far from the game's origin, a camera that targets the preview's viewport and frames the scene's bounds, and a light rig visible only on that layer. The camera SHALL use the game's main camera's tonemapping, exposure and environment lighting, or the defaults when there is none. An eleventh preview SHALL close the least recently attached. A path that is not a mesh or glTF scene SHALL answer an error naming the kind.

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

### Requirement: Asset preview in the protocol contract
`asset_preview`, `asset_preview_control` and `asset_preview_close`, `viewport_attach`'s `view` and its answer's `viewport`, the optional `viewport` on the viewport methods SHALL be added to `protocol/bevy-ts.openrpc.json` with `x-bevy-ts-crate: "bevy-ts-viewport"` as an additions-only minor bump to 1.8.0, generated into Rust and TS types by bridge's pipeline.

#### Scenario: An engine without preview support
- **WHEN** a client calls `bevy-ts/asset_preview` on a 1.7 engine
- **THEN** it gets method-not-found, and the studio keeps showing the asset with three.js
