## MODIFIED Requirements

### Requirement: A game folder records what built it
`game/game.json` SHALL hold `name`, `entry` (relative to `game/scripts/`), `bevyTs` (the version) and `engine` (prebuilt or the project engine's hash), and MAY hold `meshPicking` (a bool, absent meaning `false`), copied from `bevy-ts.json`. An engine SHALL refuse to play a `game.json` whose `bevyTs` is not its own version, naming both.

#### Scenario: A mismatched engine
- **WHEN** a 0.2.0 engine is started on a game folder built by 0.1.0
- **THEN** it exits 1 with a message naming both versions

#### Scenario: Mesh picking recorded
- **WHEN** a project with `"meshPicking": true` is built
- **THEN** its `game.json` holds `"meshPicking": true`, and one built without it holds no `meshPicking`
