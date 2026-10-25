# shipped-game Specification

## Purpose
How an engine plays a built game folder: from a bare start beside `game/` or `bevy-ts play`, loading the shipped scripts once, with no compiler, watcher, protocol, inspector or studio tools.

## Requirements

### Requirement: An engine plays the game folder beside it
An engine started with no arguments SHALL play the game in the `game/` folder beside its executable when `game/game.json` exists there, and SHALL print its usage and exit 2 otherwise. An engine with `game/game.json` beside it SHALL play that game whatever arguments it is started with, and SHALL pass every one of them to the game as `process.argv`'s arguments; it SHALL NOT read any of them as a command. `bevy-ts play <game folder> [--headless] [-- <args>…]` SHALL play the game in the named folder (the folder holding `game.json`), in a window, or with `--headless` offscreen with no window, passing the arguments after `--` to the game. Playing SHALL NOT depend on the working directory.

#### Scenario: Starting a built game
- **WHEN** the operator runs `dist/aarch64-apple-darwin/arena` from their home folder
- **THEN** a window opens and the arena plays, its levels loaded from `game/assets/levels/`

#### Scenario: An engine with no game
- **WHEN** the prebuilt engine is started with no arguments and no `game/` beside it
- **THEN** it prints its usage and exits 2, as it does today

#### Scenario: A built game with arguments
- **WHEN** the operator runs `dist/aarch64-apple-darwin/arena --server 10.0.0.5 dev`
- **THEN** the arena plays, and `process.argv.slice(2)` is `["--server", "10.0.0.5", "dev"]`

#### Scenario: Playing a folder with arguments
- **WHEN** the operator runs `bevy-ts play dist/arena/game --headless -- --seed 7`
- **THEN** the game plays headless and `process.argv.slice(2)` is `["--seed", "7"]`

### Requirement: A shipped game runs without dev tooling
Playing SHALL load the modules under `game/scripts/` once, as one generation, from `game.json`'s entry, with no TS compiler, no file watcher, no remote protocol server, no inspector and none of the studio's viewport tools (edit, assets, previews), and with bevy-ts in non-dev mode (`BevyTsPlugin::dev(false)`). Assets SHALL be read from `game/assets/` with Bevy's file watcher off. A script that throws SHALL report as under `bevy-ts dev`, mapped through the shipped source maps to the TS file and line.

#### Scenario: Nothing listens
- **WHEN** a built game is playing
- **THEN** nothing listens on port 15702 or any other port the engine would open under `bevy-ts dev`

#### Scenario: Editing a script does not reload
- **WHEN** a file under `game/scripts/` changes while the game plays
- **THEN** the running game is unchanged

#### Scenario: A thrown error names the TS line
- **WHEN** a system in `src/enemy.tsx` throws in a built game
- **THEN** the error printed names `src/enemy.tsx` and its line

### Requirement: A game folder records what built it
`game/game.json` SHALL hold `name`, `entry` (relative to `game/scripts/`), `bevyTs` (the version) and `engine` (prebuilt or the project engine's hash), and MAY hold `meshPicking` (a bool, absent meaning `false`), copied from `bevy-ts.json`. An engine SHALL refuse to play a `game.json` whose `bevyTs` is not its own version, naming both.

#### Scenario: A mismatched engine
- **WHEN** a 0.2.0 engine is started on a game folder built by 0.1.0
- **THEN** it exits 1 with a message naming both versions

#### Scenario: Mesh picking recorded
- **WHEN** a project with `"meshPicking": true` is built
- **THEN** its `game.json` holds `"meshPicking": true`, and one built without it holds no `meshPicking`
