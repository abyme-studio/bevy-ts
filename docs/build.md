# Building a game

`bevy-ts build` turns a project into a folder another person can run on the same platform. No
Rust toolchain is needed unless the project has Rust ([native.md](native.md)).

```sh
bevy-ts build [<project dir>] [--out <folder>] [--target <triple>] [--entry <file>] [--json]
bevy-ts build --targets [--json]
```

- The project root is `<project dir>` (default: the working directory) or its nearest ancestor
  holding `bevy-ts.json`, `tsconfig.json` or `package.json`.
- The name is `bevy-ts.json`'s `name`, else the root folder's name.
- The entry is `--entry` (relative to the working directory), else `bevy-ts.json`'s `entry`
  (relative to the root), else the first of `src/game.tsx`, `src/game.ts`, `src/main.tsx`,
  `src/main.ts`.
- `--out` defaults to `<project>/dist/<target>`. The build replaces it, so it refuses an `--out`
  that holds the project, lies in its `assets/`, is a file, or is an existing folder that is not an
  earlier build (no `game/game.json`).
- A game cannot be named `game` (any case): its executable would collide with `game/`.

## The folder

```
dist/aarch64-apple-darwin/
  arena                 the release engine, renamed after the game (arena.exe on Windows)
  game/
    game.json           { "name", "entry", "bevyTs", "engine" }
    scripts/src/…       every module of the entry's graph, compiled, at its path in the project
                        (a `.wgsl` a module imports ships as the shader file itself)
    scripts/node_modules/<name>@<version>/…
                        each npm package module the graph imports, at its path in its package
    assets/…            a copy of the project's assets/, levels included
```

- The engine is a copy of the prebuilt engine running the build, or, for a project with Rust, the
  project engine in the release profile (from the caches, or built by cargo).
- Scripts are compiled as under `bevy-ts dev`, with the production JSX transform and source maps
  that keep only the file's path in the project (`src/enemy.tsx`), never its text. No path of
  the build machine is shipped. A thrown error
  still names the TS file and line.
- Package modules ship under `scripts/node_modules/<name>@<version>/`, `<name>` and `<version>`
  from the nearest `package.json` above each file, never under pnpm's `.pnpm/` folders. Every
  import is rewritten to the relative path between the two shipped files, so the folder plays
  with the project's `node_modules` gone. Two different files at one shipped path fail the build
  naming both ([scripts.md](scripts.md)).
- The folder is written as `<out>.partial-<pid>` beside `<out>` and renamed onto it only when
  whole. A failed build leaves an earlier `<out>` untouched and no partial folder. The exit code is
  0 only when the folder is whole.

## Playing it

Start the executable. With `game/game.json` beside it, an engine started with no arguments plays
that game, wherever it was started from (a double-click works). `bevy-ts play <folder>
[--headless]` plays the game folder named (the one holding `game.json`), for trying a build.

Walked 2026-10-22 on the operator's M5 Max (macOS 26.4): `bevy-ts build examples/cube`, then a
Finder double-click on `dist/aarch64-apple-darwin/cube` opened a window with the cube bobbing. The
arena has no camera of its own (the studio supplies one under `bevy-ts dev`), so a built arena
shows black until the game spawns a camera.

A shipped game loads its scripts once, as one generation: no compiler, no file watcher, no remote
protocol, no inspector, none of the studio's viewport tools, bevy-ts in non-dev mode, and assets read from `game/assets/` unwatched.
Nothing listens on a port. An engine refuses a `game.json` built by another bevy-ts version and
names both.

## Events

With `--json`, stdout holds only JSON objects, one per line. abyme's Build dialog reads them.

| event | fields | when |
| --- | --- | --- |
| `step` | `name`, `index` (1-based), `total` (4) | a step starts |
| `progress` | `done`, `total` | while cargo builds a project engine |
| `log` | `line` | anything a step prints, cargo's output included |
| `problem` | `message`, `file`?, `line`?, `column`? | each script error; `file` absolute, line and column 1-based |
| `done` | `out`, `run` | last, on success: the folder and its executable, absolute |
| `failed` | `message` | last, on failure |

The steps are `Compiling scripts`, `Preparing engine`, `Copying assets` and `Writing game`.
`progress.total` is the number of packages in the engine crate's `cargo metadata` resolve, so it
is approximate; `done` counts the crates cargo has finished and never passes it.

Measured 2026-10-22 on an M5 Max, `bevy-ts build examples/thrusters --json` with a cold release
cache: 2,496 lines, all JSON; 652 `progress` events rising from `0/763` to `651/763`, then
`Copying assets`, `Writing game` and `done`; 1,839 `log` lines, cargo's included. The bar stops
near 85%, not 100%: a package count overstates the crates cargo builds for one target. The built
`thrusters` played headless, its native plugin and TS system both running.

Without `--json`, steps, logs and problems go to stderr as text, and on success stdout holds
only the executable's path.

## Targets

`bevy-ts build --targets --json` prints one line, `{"targets":[…]}`, each target with `id`,
`label`, `host`, `ready` and `why` (`null` when ready):

| id | label |
| --- | --- |
| `aarch64-apple-darwin` | macOS (Apple silicon) |
| `x86_64-apple-darwin` | macOS (Intel) |
| `aarch64-unknown-linux-gnu` | Linux (ARM64) |
| `x86_64-unknown-linux-gnu` | Linux (x64) |
| `x86_64-pc-windows-msvc` | Windows (x64) |

Only the running engine's own target builds; the others say `cross builds are not supported
yet`, and `--target` naming one fails with that message before anything is written.

## What a player's machine needs

- The same platform as the build: an Apple silicon build does not run on an Intel Mac.
- Linux: the libraries Bevy links, which the game does not carry: ALSA (`libasound2`), udev
  (`libudev1`), a Vulkan driver (`mesa-vulkan-drivers` or the GPU vendor's), and X11 or Wayland
  client libraries (`libx11-6`, `libxkbcommon0`, `libwayland-client0`).
- macOS: the engine is not signed or notarised. A downloaded folder is quarantined, and Gatekeeper
  refuses to open it; the player right-clicks the executable and picks Open once, or runs
  `xattr -dr com.apple.quarantine <folder>`. A folder built on the same Mac is not quarantined.

Not yet: cross builds, a macOS `.app` bundle, icons, signing and notarisation, installers, and
minified scripts.
