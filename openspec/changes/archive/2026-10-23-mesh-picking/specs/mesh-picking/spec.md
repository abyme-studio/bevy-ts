## ADDED Requirements

### Requirement: A project opts into mesh picking
`bevy-ts.json` SHALL accept `meshPicking`, a bool, default `false`. When it is `true`, `bevy-ts dev` SHALL add Bevy's `MeshPickingPlugin` with its default settings, under a window, `--viewport` and `--headless` alike. When it is `false` or absent, the engine SHALL NOT add it. The switch SHALL NOT need a project engine: it is not an engine hash input and starts no `cargo`. A value that is not a bool SHALL fail `bevy-ts dev` with a message naming `meshPicking`.

#### Scenario: A script-only game clicks a mesh
- **WHEN** a project with `"meshPicking": true` and no `native` runs `bevy-ts dev`, and the player clicks a cube whose `<entity>` has `onPointerClick`
- **THEN** the prebuilt engine runs, and the handler fires once with the cube's entity

#### Scenario: Off by default
- **WHEN** a project's `bevy-ts.json` has no `meshPicking`
- **THEN** the app has no `MeshPickingSettings` resource and no mesh ray casts run

#### Scenario: A wrong type
- **WHEN** `bevy-ts.json` holds `"meshPicking": "yes"`
- **THEN** `bevy-ts dev` exits 1 with a message naming `meshPicking` and `bevy-ts.json`

### Requirement: A built game keeps the switch
`bevy-ts build` SHALL write the project's `meshPicking` into `game/game.json`, and playing SHALL add `MeshPickingPlugin` when `game.json`'s `meshPicking` is `true`. A `game.json` without `meshPicking` SHALL play with mesh picking off.

#### Scenario: Built and played
- **WHEN** a project with `"meshPicking": true` is built and its folder played
- **THEN** `game.json` holds `"meshPicking": true` and the played app has `MeshPickingSettings`

### Requirement: A mesh handler with mesh picking off is named once
When an entity with `Mesh3d` has an `onPointer*` handler and the app has no `MeshPickingSettings`, the engine SHALL log one warning per process that names the entity and says to set `"meshPicking": true` in `bevy-ts.json`. It SHALL NOT warn when mesh picking is on, however it was added (the switch or a native plugin).

#### Scenario: A silent click made loud
- **WHEN** a game without `meshPicking` mounts two cubes with `onPointerClick`
- **THEN** one warning naming the first cube and `"meshPicking": true` is logged, and no second

#### Scenario: A native plugin added it
- **WHEN** a project engine's plugin adds `MeshPickingPlugin` itself and `bevy-ts.json` has no `meshPicking`
- **THEN** no warning is logged
