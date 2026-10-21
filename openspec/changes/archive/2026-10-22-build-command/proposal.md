## Why

A bevy-ts game cannot be shipped. `bevy-ts build` today only builds a project engine in the release profile and prints its path; a project with no Rust gets "it runs on the prebuilt engine" and nothing else. Every engine only runs `bevy-ts dev`: it compiles the TS sources at start, watches them, and serves the protocol. abyme's Bevy studio needs a Build dialog over a command that makes a game someone else can run (the bevy editor study, section 9), and that reports its progress as events the dialog can show. abyme's half is the team `bevy-build`; the event format below is agreed with it.

## What Changes

- **BREAKING:** `bevy-ts build [<dir>] [--out <folder>] [--target <triple>] [--entry <file>] [--json]` builds a runnable game folder, not just a project engine:
  - `<out>/<name>` (`.exe` on Windows): the release engine, the project engine or a copy of the prebuilt one, renamed after the game;
  - `<out>/game/`: `game.json`, the compiled scripts and the project's `assets/` (levels included).
  - `--out` defaults to `<project>/dist/<target>`. The folder is built beside `<out>` and renamed over it at the end, so a failed build leaves the old one untouched. Exit 0 only when the folder is whole.
  - The game's name is `bevy-ts.json`'s new `name`, else the project folder's name. The entry is `--entry`, else `bevy-ts.json`'s new `entry`, else the first of `src/game.tsx`, `src/game.ts`, `src/main.tsx`, `src/main.ts`.
- **Progress as events.** With `--json`, stdout is one JSON object per line and nothing else:
  - `{"event":"step","name","index","total"}` at each step's start (1-based; four steps);
  - `{"event":"progress","done","total"}` while cargo builds a project engine (approximate total);
  - `{"event":"log","line"}` for anything a step prints, cargo's output included;
  - `{"event":"problem","message","file"?,"line"?,"column"?}` per script compile error;
  - last, `{"event":"done","out","run"}` or `{"event":"failed","message"}`.
  - Without `--json`: steps and logs as text on stderr, the executable's path on stdout.
- **`bevy-ts build --targets [--json]`** lists the five npm platforms, each with `id`, `label`, `host`, `ready` and `why`. Only the host target builds in this change; the others are `ready: false`, `why: "cross builds are not supported yet"`, and `--target` naming one fails with that message.
- **A shipped game plays.** An engine started with no arguments, with `game/game.json` beside it, plays that game: it loads the compiled scripts once, without a compiler, a file watcher, the remote protocol, or bevy-ts's dev checks, and reads assets from `game/assets` unwatched. `bevy-ts play <game folder>` does the same for a named folder.
- **Out of scope:**
  - cross builds (other platforms' engines or cross toolchains);
  - a macOS `.app` bundle, icons, code signing, notarisation and installers;
  - minifying or obfuscating scripts;
  - abyme's Build dialog (team `bevy-build`).

## Capabilities

### New Capabilities
- `game-build`: `bevy-ts build`: the game folder it writes, its steps and their events, and the targets it lists.
- `shipped-game`: how an engine plays a built game folder, with no compiler, watcher or protocol.

### Modified Capabilities
None. `project-engine` already says `bevy-ts build` builds the release engine; a built game ships it.

## Impact

- `crates/bevy-ts-cli`:
  - a new `build.rs` replacing `supervise::build`: the steps, the events, the folder and its atomic rename;
  - `engine.rs`: `obtain` reports cargo's output and progress through the build's events instead of inheriting stderr;
  - `compile.rs`: a release compile (production JSX, source maps without source text);
  - `cli.rs`: `play`, the no-argument start, and the new `build` options in the usage;
  - `headless.rs`: an unwatched asset plugin for a shipped game;
  - `project.rs`: `name` and `entry` in `bevy-ts.json`.
- `crates/bevy-ts`: `BevyTsPlugin` runs with `dev(false)` for the first time outside tests; the shipped-game test covers it.
- Docs: a new `docs/build.md`; `docs/native.md`'s line on `bevy-ts build`; the README's commands.
- No new dependency, service or infrastructure. cargo's `--message-format=json` and `cargo metadata` are already part of the toolchain a project engine needs.
- Performance: none at dev time. A shipped game skips the compiler, the watchers and the protocol, so it does less than `bevy-ts dev`.
- abyme: the `bevy-build` team builds the Build dialog over these events and `--targets`; nothing in abyme changes here.
