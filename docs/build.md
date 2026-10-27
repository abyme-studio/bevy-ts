# Building a game

`bevy-ts build` turns a project into a folder another person can run on the same platform. No
Rust toolchain is needed unless the project has Rust ([native.md](native.md)).

```sh
bevy-ts build [<project dir>] [--out <folder>] [--target <triple>] [--entry <file>] [--copy-sources] [--json]
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
- `--copy-sources` copies each of `bevy-ts.json`'s `assetSources` into the game; without it none
  is copied ([Asset sources](#asset-sources)).

## The folder

```
dist/aarch64-apple-darwin/
  arena                 the release engine, renamed after the game (arena.exe on Windows)
  game/
    game.json           { "name", "entry", "bevyTs", "engine", "net", "env", "assetSources" }
    scripts/src/…       every module of the entry's graph, compiled, at its path in the project
                        (a `.wgsl` a module imports ships as the shader file itself)
    scripts/node_modules/<name>@<version>/…
                        each npm package module the graph imports, at its path in its package
    assets/…            a copy of the project's assets/, levels included
    sources/<name>/…    with --copy-sources only: a copy of each asset source
```

- The engine is a copy of the prebuilt engine running the build, or, for a project with Rust, the
  project engine in the release profile (from the caches, or built by cargo).
- Either engine carries bevy-ts's patched `bevy_render`, so on macOS a game's first frames keep
  presenting while its render pipelines compile ([native.md](native.md#project-engines)).
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

## Asset sources

A source in `bevy-ts.json`'s `assetSources` ([assets.md](assets.md#asset-sources)) stays out of a
build: its files may be under a licence that forbids sharing them, and a built folder is one copy
from a repository or a shared drive. `game.json`'s `assetSources` names each folder instead: an
absolute or `~/` folder as written (`~/` is the playing machine's home), a relative one made
absolute on the build machine. A folder absent at build time is a warning in the build's log, and
the build goes on.

A built game registers each source before it starts. A folder that is not there stops it with exit
code 1, before a window opens, and one line:

```
bevy-ts play: asset source "stardew": /Users/me/dev/assets/stardew does not exist; put the folder there, or build the game with --copy-sources
```

Pass `--copy-sources` to ship the files, for a game played on a machine that lacks them, when
their licence allows: each source is copied to `game/sources/<name>/`, `game.json` names
`sources/<name>`, read from the game folder, and the build's log names each source and its file
count. The built folder then holds the files: share it as you would them. A source absent at build
time with `--copy-sources` fails the build naming it.

## Playing it

Start the executable. With `game/game.json` beside it, an engine plays that game, wherever it was
started from (a double-click works), and passes every argument it was started with to the game as
`process.argv.slice(2)` ([networking.md](networking.md)): `arena --server 10.0.0.5 dev` hands the
game `["--server", "10.0.0.5", "dev"]` and reads none of them as a command, so `dev`, `build` and
the other commands are not reachable from a shipped game. `bevy-ts play <folder> [--headless]
[-- <args>…]` plays the game folder named (the one holding `game.json`), for trying a build, with
the arguments after `--` as the game's.

Headless (`play --headless`, `dev --headless`) has no window: every camera renders into one 960x540
offscreen image. `BEVY_TS_SIZE=<W>x<H>` (as `2880x1620`) makes it that size instead; only a headless
run reads it. The UI, headless input and both shots follow the image. A value that is not
`<W>x<H>`, has a side of 0, or is past the GPU's largest texture stops the start with exit code 2
and a line starting `bevy-ts: BEVY_TS_SIZE=`. `bevy-ts/screenshot { path }` saves it as a PNG and answers once the file is
written, with its size and the frame it shows; for a chosen frame, `bevy-ts/pause` and
`bevy-ts/step` there first (rendering runs while paused). A shot waits until no render pipeline
is still compiling and no asset is still loading (a texture, a glTF and its scene), three updates
in a row, so nothing is caught as its fallback or missing: the pipelines a texture's material
needs once it loads are only queued in the render after; an asset that failed is named in a warning. A shot waits as long as
the run makes progress: a pipeline finished, or fewer assets loading than the fewest so far. After 60 s with no progress the
method fails, naming what never held: pipelines still waiting, assets still loading, or no frame rendered. So a cold build
that keeps compiling is waited out, and a missing shader is not. A run with no protocol (`play
--headless`) takes one shot with `BEVY_TS_SCREENSHOT=<path>`, at the 60th ready frame in a row: a
frame on which the script has started, no pipeline is waiting and nothing is loading. After 60 s with no progress it
saves nothing, logs an error naming the path and what is not ready, and the run exits with code 1; it fails the same way
when no frame renders within 5 s of its capture, no image is read back within 60 s, or Bevy captured but wrote nothing.
`bevy-ts play`, `bevy-ts dev` and `bevy-ts skeleton` exit with the code of the game's `AppExit`. Both shots write a temp file beside the path
(`.<name>.tmp-…`) and rename it onto the path, so a reader polling the path sees no file or a whole PNG, never part of one. Under it, time runs on a fixed step, through Bevy's
`TimeUpdateStrategy::ManualDuration`: 1/60 s a ready frame and none on a frame that is not ready.
The shot's clock is always 60 steps: a ready frame before one that was not (a load the script had
not asked for yet) stepped time too, so the steps stop at 60. And every camera draws with Bevy's
`NoIndirectDrawing`: Bevy's GPU culling places each instance with an atomic add, so which of two
instances at the same depth is drawn last changes from frame to frame (a leaf of one tree over
another's, in a few pixels). So the shot is the same picture, byte for byte, at any machine load.
`BEVY_TS_FIXED_STEP=1` gives a run driven over the protocol the same clock and drawing; a
`bevy-ts/screenshot` without it can differ in such pixels. `BEVY_TS_PIPELINE_TRACE=1` prints when the pipelines are ready
([native.md](native.md#project-engines)). Every bound above runs inside Update, so a frame that never
finishes (a GPU driver that stops taking the engine's work, a script that never returns) would hang
the run. The stall watchdog bounds it: a frame open for 30 s of wall time prints one line to
stderr, starting `Update stopped:`, with the frame, how long ago it started and where it is stuck
(the frame's systems, waiting for the render world, or the render). Every protocol request still
open, a waiting `bevy-ts/screenshot` included, is answered with that line as a `-32603` error.
Then a headless run (so every `BEVY_TS_SCREENSHOT` run) exits with code 75 (`EX_TEMPFAIL`: it may
pass if run again); a window or `--viewport` only warns, and says when Update resumed; `--inspect`
has no watchdog. `BEVY_TS_STALL_SECS=<seconds>` sets the limit, `0` turns it off. Game UI draws into
that image at its size: headless gives `IsDefaultUiCamera` to the highest-order camera, as Bevy
does for a window. A game's own `IsDefaultUiCamera` on another camera, or a root's
`UiTargetCamera`, wins.

Walked 2026-10-22 on the operator's M5 Max (macOS 26.4): `bevy-ts build examples/cube`, then a
Finder double-click on `dist/aarch64-apple-darwin/cube` opened a window with the cube bobbing. A
game needs a camera of its own: the studio's editor camera is not shipped. The examples place
theirs in their TSX scene.

A shipped game loads its scripts once, as one generation: no compiler, no file watcher, no remote
protocol, no inspector, none of the studio's viewport tools, bevy-ts in non-dev mode, and assets read from `game/assets/` unwatched.
Nothing listens on a port. `game.json` carries the project's `net` and `env`
([networking.md](networking.md)) as written, `${NAME}` entries unexpanded: the game expands them
from its own environment when it starts, so a player's `ABYME_URL` is the one that counts. A
`game.json` without them allows no host and shows no variable. An engine refuses a `game.json`
built by another bevy-ts version and names both.

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
  (`libudev1`), fontconfig (`libfontconfig1`, for text fallback fonts; without it the game does not
  start, `docs/fonts.md`), a Vulkan driver (`mesa-vulkan-drivers` or the GPU vendor's), and X11 or
  Wayland client libraries (`libx11-6`, `libxkbcommon0`, `libwayland-client0`).
- macOS: the engine is not signed or notarised. A downloaded folder is quarantined, and Gatekeeper
  refuses to open it; the player right-clicks the executable and picks Open once, or runs
  `xattr -dr com.apple.quarantine <folder>`. A folder built on the same Mac is not quarantined.

Not yet: cross builds, a macOS `.app` bundle, icons, signing and notarisation, installers, and
minified scripts.

## Testing

The CLI's tests write scratch folders under `$TMPDIR/bevy-ts-cli-tests` and remove each when its test
ends, pass or fail. `BEVY_TS_KEEP_TEST_DIRS=1` keeps them, and each prints its path, to look at what a
failing test left.
