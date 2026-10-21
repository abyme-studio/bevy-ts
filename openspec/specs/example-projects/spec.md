# example-projects Specification

## Purpose
The examples the `bevy-ts` npm package ships, how `bevy-ts examples` lists them and how `bevy-ts new` makes a playable project from one, with no checkout and no Rust.

## Requirements

### Requirement: An example ships when it declares itself
An example SHALL ship in the `bevy-ts` npm package exactly when its directory under `examples/` holds an `example.json` with `id` (`[a-z0-9-]+`, unique), `title`, `description` and `entry` (the path of the entry file, relative to the example). An example SHALL declare itself only when it runs on the prebuilt engine, that is when its `bevy-ts.json` has nothing under `native`. The package SHALL carry each shipped example's `src/`, `assets/`, `bevy-ts.json`, `README.md` and `thumbnail.png` where present, and its `example.json`, and nothing else from it.

#### Scenario: The first release
- **WHEN** the `bevy-ts` package is packed
- **THEN** it holds `examples/cube` and `examples/arena` and no `rapier`, `thrusters`, `WALK.md` or `walk.mjs`

#### Scenario: An example that needs Rust
- **WHEN** an example with `native.ports` in `bevy-ts.json` holds an `example.json`
- **THEN** packing fails and names that example

### Requirement: `bevy-ts examples` lists the shipped examples
`bevy-ts examples --json` SHALL print one JSON object on stdout and exit 0: `{ "version": <bevy-ts version>, "examples": [ { "id", "title", "description", "entry", "thumbnail"? } ] }`, in the order to show them (`cube` first, then by id). `thumbnail` SHALL be the absolute path of the example's PNG and SHALL be absent when it has none. Without `--json` it SHALL print one line per example: id, title, description. It SHALL NOT start the engine.

#### Scenario: Listing for abyme
- **WHEN** abyme runs `npx --yes bevy-ts@0.1.0-alpha.1 examples --json`
- **THEN** stdout parses as JSON with `version` `0.1.0-alpha.1` and examples `cube` and `arena`, each with title, description and entry, and no `thumbnail` key

### Requirement: `bevy-ts new` makes a project from an example
`bevy-ts new <dir> [--example <id>] [--no-install] [--json]` SHALL make a project in `<dir>` from the example (`cube` when `--example` is absent), refusing with exit 1 and a message on stderr when `<dir>` exists and is not empty, or when the id is unknown. The project SHALL hold the example's shipped files except `example.json` and `thumbnail.png`, a `package.json` (`name` from the directory's name, `private`, `type: module`, a `dev` script running `bevy-ts dev <entry>`, and `bevy-ts` as a dev dependency at exactly the running version), a `tsconfig.json` resolving `bevy-ts` from `node_modules` with no paths into a checkout, and a `.gitignore` of `node_modules/` and `.bevy-ts/`. It SHALL NOT start the engine.

#### Scenario: A cube project
- **WHEN** `bevy-ts new ~/games/bob --example cube --no-install` runs with bevy-ts 0.1.0-alpha.1
- **THEN** `~/games/bob` holds `src/game.ts`, `package.json` with `"bevy-ts": "0.1.0-alpha.1"` and `"dev": "bevy-ts dev src/game.ts"`, `tsconfig.json` with no `paths`, and `.gitignore`, and `tsc -p ~/games/bob` passes once bevy-ts is installed

#### Scenario: A directory with files in it
- **WHEN** `<dir>` holds a file
- **THEN** `new` exits 1, says so on stderr and writes nothing

#### Scenario: An unknown example
- **WHEN** `--example nope` is given
- **THEN** `new` exits 1, names the known ids on stderr and writes nothing

### Requirement: `bevy-ts new` installs by default
After writing the project, `new` SHALL run the install in `<dir>` with the package manager that ran it (from `npm_config_user_agent`: npm, pnpm, yarn or bun), npm otherwise, its output on stderr. `--no-install` SHALL skip it. With `--json`, stdout SHALL carry only one object, `{ "dir": <absolute dir>, "entry": <entry>, "example": <id>, "installed": <bool> }`. A failed install SHALL leave the project in place, print `installed: false` under `--json`, and exit 1.

#### Scenario: abyme makes a project
- **WHEN** abyme runs `npx --yes bevy-ts@0.1.0-alpha.1 new /Users/a/games/bob --example arena --json`
- **THEN** npm's progress goes to stderr, `node_modules/bevy-ts` is 0.1.0-alpha.1, and stdout is `{"dir":"/Users/a/games/bob","entry":"src/game.tsx","example":"arena","installed":true}`

#### Scenario: The registry is unreachable
- **WHEN** the install fails
- **THEN** the project files stay, stdout under `--json` has `"installed": false`, and the exit code is 1

### Requirement: `new` and `examples` belong to the launcher
The Node launcher (`bin/bevy-ts.js`) SHALL handle `new` and `examples` itself and pass every other command to the engine binary. The engine binary run directly SHALL refuse them with exit 2 and a message pointing to `npx bevy-ts`.

#### Scenario: Without a platform package
- **WHEN** `bevy-ts examples --json` runs where no platform package is installed
- **THEN** it lists the examples and exits 0
