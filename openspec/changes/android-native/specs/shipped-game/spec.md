## MODIFIED Requirements

### Requirement: An engine plays the game folder beside it
An engine started with no arguments SHALL play the game in the `game/` folder beside its executable when `game/game.json` exists there, and SHALL print its usage and exit 2 otherwise. `bevy-ts play <game folder> [--headless]` SHALL play the game in the named folder (the folder holding `game.json`), in a window, or with `--headless` offscreen with no window. An embedding host SHALL name the game folder in `bevy_ts_create` (engine-embedding), and that engine SHALL play it with the same checks and the same app as `bevy-ts play`. Playing SHALL NOT depend on the working directory, and only the bare start SHALL look beside the executable.

#### Scenario: Starting a built game
- **WHEN** the operator runs `dist/aarch64-apple-darwin/arena` from their home folder
- **THEN** a window opens and the arena plays, its levels loaded from `game/assets/levels/`

#### Scenario: An engine with no game
- **WHEN** the prebuilt engine is started with no arguments and no `game/` beside it
- **THEN** it prints its usage and exits 2, as it does today

#### Scenario: A host names the folder
- **WHEN** an embedding host creates an engine on a game folder in its own storage, far from the library's path
- **THEN** that game plays, and the engine reads nothing beside its library
