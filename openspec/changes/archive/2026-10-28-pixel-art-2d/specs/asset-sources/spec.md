## ADDED Requirements

### Requirement: Extra asset sources in bevy-ts.json
`bevy-ts.json` SHALL take `assetSources`, a map from a name to a folder. Each SHALL be registered as a Bevy asset source of that name before `AssetPlugin` builds, so `name://path` loads `path` under that folder in any asset slot. A folder SHALL be an absolute path, a path starting `~/` (the user's home), or a path relative to `bevy-ts.json`. A name SHALL be lowercase letters, digits and `-`; `embedded` and an empty name SHALL fail at start naming the name. The project's `assets/` SHALL stay the default source, loaded without a prefix.

#### Scenario: A sprite from outside the project
- **WHEN** `bevy-ts.json` has `"assetSources": { "stardew": "<dir>" }` and a script mounts `<Sprite image="stardew://Characters/Abigail.png" />`
- **THEN** the sprite's image loads from `<dir>/Characters/Abigail.png`, and nothing is copied under the project

#### Scenario: A bad name
- **WHEN** `assetSources` has the name `Stardew` or `embedded`
- **THEN** `bevy-ts dev` exits 1 naming `assetSources` and the name

### Requirement: A missing source folder warns in dev
Under `bevy-ts dev`, a source whose folder does not exist SHALL log one warning at start naming the source and the resolved folder, and the game SHALL run. A load from it SHALL fail as a missing file does.

#### Scenario: Another machine
- **WHEN** a project names a source whose folder is absent and starts with `bevy-ts dev`
- **THEN** the game runs, a warning names the source and folder, and a sprite from that source has a failed load state

### Requirement: dev watches every source
Under `bevy-ts dev` each extra source SHALL be watched as the default one is: a changed file in it reloads its assets.

#### Scenario: A sheet edited outside the project
- **WHEN** a game under `bevy-ts dev` shows `stardew://Maps/springobjects.png` and that file changes
- **THEN** the image reloads

### Requirement: A build never copies a source unless asked
`bevy-ts build` SHALL NOT copy any file of an extra source into the built game. It SHALL write `assetSources` into `game.json`, each name to its folder: an absolute or `~/` folder as written, a relative one made absolute. A folder absent at build time SHALL warn naming the source and folder, and the build SHALL go on. `bevy-ts build --copy-sources` SHALL copy each source to `sources/<name>/` in the built game, name `sources/<name>` in `game.json`, report each source's file count, and fail naming a source whose folder is absent.

#### Scenario: A default build
- **WHEN** a project with source `stardew` at `~/dev/assets/stardew` is built without flags
- **THEN** the built folder holds no file from that source and no `sources/` folder, and `game.json` has `"assetSources": { "stardew": "~/dev/assets/stardew" }`

#### Scenario: A build that copies
- **WHEN** the same project is built with `--copy-sources` and played
- **THEN** the built folder holds `sources/stardew/`, `game.json` names `sources/stardew`, the report names `stardew` and its file count, and `stardew://` paths load in play

### Requirement: A built game without its source exits at start
`bevy-ts play`, and a built game's executable, SHALL register each source in `game.json` before `AssetPlugin` builds, unwatched, reading `~/` on the playing machine and a relative folder from the game folder. A source whose folder is absent SHALL exit 1 before a window opens, printing one line that names the source, the folder looked in, and the fix: put the folder there, or build with `--copy-sources`.

#### Scenario: A player without the sprites
- **WHEN** a game built without `--copy-sources` starts on a machine with no `~/dev/assets/stardew`
- **THEN** it exits 1 before a window opens, with a line naming `stardew`, the resolved folder and `--copy-sources`

### Requirement: levels check resolves source prefixes
`bevy-ts levels check` SHALL check a `name://` asset path under that source's folder, read from the `bevy-ts.json` found by walking up from the first file. An unknown name SHALL be an error naming it and the path.

#### Scenario: A level using a source
- **WHEN** a level names `stardew://Maps/Town.png`, the source exists, and the file is there
- **THEN** `levels check` reports no missing asset; a path naming source `nope` is an error naming `nope`
