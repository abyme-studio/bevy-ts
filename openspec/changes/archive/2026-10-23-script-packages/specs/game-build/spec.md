## MODIFIED Requirements

### Requirement: `bevy-ts build` writes a runnable game folder
`bevy-ts build [<dir>] [--out <folder>] [--target <triple>] [--entry <file>] [--json]` SHALL take as the project root `<dir>` (default the working directory) or its nearest ancestor holding `bevy-ts.json`, `tsconfig.json` or `package.json`, as `bevy-ts dev` does, and write a game folder holding:
- `<name>` (`<name>.exe` on Windows): the release engine, which is the project engine built in the release profile for a project with Rust, else a copy of the prebuilt engine running the build;
- `game/game.json`: the game's name, its entry module, the bevy-ts version and the engine's identity;
- `game/scripts/`: every module of the entry's graph, compiled for release with its file name unchanged: a game module at its path relative to the project root, and a package module at `node_modules/<name>@<version>/<path in its package>`, `<name>` and `<version>` taken from the nearest `package.json` above it. Every import SHALL be rewritten to the relative path between the two shipped files, so no path of the build machine is shipped. Two different files that would ship at the same path SHALL fail the build naming both;
- `game/assets/`: a copy of the project's `assets/`, levels included.

The name SHALL be `bevy-ts.json`'s `name`, else the project folder's name. The entry SHALL be `--entry`, else `bevy-ts.json`'s `entry`, else the first of `src/game.tsx`, `src/game.ts`, `src/main.tsx`, `src/main.ts` that exists. `--out` SHALL default to `<project>/dist/<target>`. The folder SHALL be written beside `<out>` under another name and renamed onto `<out>` only when complete, so a failed build leaves any earlier `<out>` unchanged and leaves no partial folder. The command SHALL exit 0 only when the folder is complete.

#### Scenario: A project with no Rust
- **WHEN** `bevy-ts build examples/arena` runs on an Apple silicon Mac
- **THEN** `examples/arena/dist/aarch64-apple-darwin/` holds `arena` (a copy of the prebuilt engine), `game/game.json`, `game/scripts/src/game.tsx` and the other modules, and `game/assets/levels/`, and the command exits 0

#### Scenario: A project with Rust
- **WHEN** `bevy-ts build examples/thrusters` runs
- **THEN** the project engine is obtained in the release profile, from the caches or built by cargo, and copied into the folder as `thrusters`

#### Scenario: No entry
- **WHEN** the project has no `entry`, no `--entry` was given, and none of the four default files exists
- **THEN** the build fails naming the four files it looked for, and no folder is written

#### Scenario: A failed rebuild keeps the old folder
- **WHEN** a build into an existing `dist/<target>` fails on a script error
- **THEN** the existing `dist/<target>` is unchanged and no temporary folder remains

#### Scenario: A project with a linked package
- **WHEN** a project whose `src/game.ts` imports `@abyme/client/connect`, a pnpm link to `~/dev/abyme/web/packages/client` (version `0.0.0`) whose `connect.ts` imports `../../kernel/src/protocol.ts` from package `@abyme/kernel` (version `0.0.0`), is built
- **THEN** the folder holds `game/scripts/src/game.ts`, `game/scripts/node_modules/@abyme/client@0.0.0/src/connect.ts` and `game/scripts/node_modules/@abyme/kernel@0.0.0/src/protocol.ts`, each import between them is relative, and `bevy-ts play` runs the folder with `node_modules` deleted from the project
