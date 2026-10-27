## ADDED Requirements

### Requirement: Extra asset sources in bevy-ts.json
`bevy-ts.json` SHALL take `assetSources`, a map from a name to a folder. Each SHALL be registered as a Bevy asset source of that name before `AssetPlugin` builds, so `name://path` loads `path` under that folder in any asset slot. A folder SHALL be an absolute path, a path starting `~/` (the user's home), or a path relative to `bevy-ts.json`. A name SHALL be lowercase letters, digits and `-`; `embedded` and an empty name SHALL fail at start naming the name. The project's `assets/` SHALL stay the default source, loaded without a prefix.

#### Scenario: A sprite from outside the project
- **WHEN** `bevy-ts.json` has `"assetSources": { "stardew": "<dir>" }` and a script mounts `<Sprite image="stardew://Characters/Abigail.png" />`
- **THEN** the sprite's image loads from `<dir>/Characters/Abigail.png`, and nothing is copied under the project

#### Scenario: A bad name
- **WHEN** `assetSources` has the name `Stardew` or `embedded`
- **THEN** `bevy-ts dev` exits 1 naming `assetSources` and the name

### Requirement: A missing source folder warns
A source whose folder does not exist SHALL log one warning at start naming the source and the resolved folder, and the game SHALL run. A load from it SHALL fail as a missing file does.

#### Scenario: Another machine
- **WHEN** a project names a source whose folder is absent and starts with `bevy-ts dev`
- **THEN** the game runs, a warning names the source and folder, and a sprite from that source has a failed load state

### Requirement: dev watches every source
Under `bevy-ts dev` each extra source SHALL be watched as the default one is: a changed file in it reloads its assets.

#### Scenario: A sheet edited outside the project
- **WHEN** a game under `bevy-ts dev` shows `stardew://Maps/springobjects.png` and that file changes
- **THEN** the image reloads

### Requirement: A build carries its sources
`bevy-ts build` SHALL copy each existing source folder to `sources/<name>/` in the built game and list the names in `game.json`. `bevy-ts play` SHALL register each listed source from there, unwatched. A source whose folder is absent at build time SHALL fail the build naming it.

#### Scenario: A built game with a source
- **WHEN** a project with source `stardew` is built and played
- **THEN** the game folder holds `sources/stardew/`, `game.json` lists `stardew`, and `stardew://` paths load in play

### Requirement: levels check resolves source prefixes
`bevy-ts levels check` SHALL check a `name://` asset path under that source's folder, read from the `bevy-ts.json` found by walking up from the first file. An unknown name SHALL be an error naming it and the path.

#### Scenario: A level using a source
- **WHEN** a level names `stardew://Maps/Town.png`, the source exists, and the file is there
- **THEN** `levels check` reports no missing asset; a path naming source `nope` is an error naming `nope`
