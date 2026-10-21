## ADDED Requirements

### Requirement: `bevy-ts build` writes a runnable game folder
`bevy-ts build [<dir>] [--out <folder>] [--target <triple>] [--entry <file>] [--json]` SHALL take as the project root `<dir>` (default the working directory) or its nearest ancestor holding `bevy-ts.json`, `tsconfig.json` or `package.json`, as `bevy-ts dev` does, and write a game folder holding:
- `<name>` (`<name>.exe` on Windows): the release engine, which is the project engine built in the release profile for a project with Rust, else a copy of the prebuilt engine running the build;
- `game/game.json`: the game's name, its entry module, the bevy-ts version and the engine's identity;
- `game/scripts/`: every module of the entry's graph, compiled for release, at its path relative to the project root with its file name unchanged;
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

### Requirement: Scripts are compiled for release
The build SHALL compile the entry's module graph as `bevy-ts dev` does (same resolution, declarations and errors), but with the production JSX transform and with source maps that carry no source text. Script errors SHALL fail the build; no folder SHALL be written.

#### Scenario: No TS source in the folder
- **WHEN** the arena is built
- **THEN** no file under the game folder contains the TS source text of `src/game.tsx`

#### Scenario: A type-stripping error
- **WHEN** `src/enemy.tsx` has a syntax error at line 3, column 7
- **THEN** the build reports a problem with that file, line and column and fails

### Requirement: Four steps, reported as events
A build SHALL run four steps in order: `Compiling scripts`, `Preparing engine`, `Copying assets`, `Writing game`. With `--json`, stdout SHALL carry only JSON objects, one per line:
- `{"event":"step","name":<string>,"index":<1-based>,"total":4}` when a step starts;
- `{"event":"progress","done":<n>,"total":<n>}` during `Preparing engine` while cargo builds, where `done` counts crates cargo has finished and `total` is the package count `cargo metadata` resolves, and `done` never exceeds `total`;
- `{"event":"log","line":<string>}` for each line a step prints, cargo's output included;
- `{"event":"problem","message":<string>,"file":<absolute path>?,"line":<1-based>?,"column":<1-based>?}` for each script error;
- last, exactly one of `{"event":"done","out":<absolute folder>,"run":<absolute executable>}` or `{"event":"failed","message":<string>}`.

Without `--json`, steps, logs and problems SHALL go to stderr as text, and on success stdout SHALL hold only the executable's absolute path.

#### Scenario: A successful build in JSON
- **WHEN** `bevy-ts build examples/arena --json` runs
- **THEN** every stdout line parses as JSON, four `step` events arrive with `index` 1 to 4, and the last line is `done` whose `run` is the folder's executable

#### Scenario: A failed build in JSON
- **WHEN** a script has two errors
- **THEN** stdout holds `step` 1, two `problem` events, and a last `failed` event, and the command exits 1

#### Scenario: Cargo's progress
- **WHEN** a project engine is built by cargo under `--json`
- **THEN** `progress` events arrive with a rising `done`, and cargo's own lines arrive as `log` events, not on stdout as raw text

### Requirement: Targets
`bevy-ts build --targets` SHALL list the five platforms bevy-ts ships engines for: `aarch64-apple-darwin`, `x86_64-apple-darwin`, `aarch64-unknown-linux-gnu`, `x86_64-unknown-linux-gnu`, `x86_64-pc-windows-msvc`, each with `id`, a human `label`, `host` (whether it is the running engine's target), `ready` and `why` (`null` when ready). With `--json` it SHALL print one line, `{"targets":[...]}`. Only the host target SHALL be ready; the others SHALL say `"cross builds are not supported yet"`. `--target` SHALL default to the host, and a build for a target that is not ready SHALL fail with its `why` before writing anything.

#### Scenario: Listing targets on an Apple silicon Mac
- **WHEN** `bevy-ts build --targets --json` runs on an Apple silicon Mac
- **THEN** it prints one JSON line with five targets, of which only `aarch64-apple-darwin` has `host: true` and `ready: true`

#### Scenario: A target that is not ready
- **WHEN** `bevy-ts build --target x86_64-pc-windows-msvc --json` runs on a Mac
- **THEN** the only line is `{"event":"failed","message":"x86_64-pc-windows-msvc: cross builds are not supported yet"}` and the command exits 1
