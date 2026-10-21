## Context

Every bevy-ts engine, prebuilt or project, runs one entry, `cli::run_engine`. Its only game command is `dev`: it builds a `Compiler` from the entry, a watcher thread sends `DevEvent::Compiled(ModuleSet)` to the app, `BevyTsPlugin` runs with `dev(true)`, assets are watched (`headless::asset_plugin`), and the remote plugin serves the protocol. `bevy-ts build` (`supervise::build`) only obtains the release project engine (`engine::obtain(plan, true)`, cargo's output inherited on stderr) and prints its path.

The V8 loader resolves relative imports against module URLs and tries `.ts`, `.tsx` and `index` names (`bevy-ts-v8/src/loader.rs`), so a module set keeps working if each module keeps its project-relative path and file name. Levels are assets (`app.level("levels/arena.bsn")` under `assets/`). The studio picks the entry from `src/game.tsx`, `src/game.ts`, `src/main.tsx`, `src/main.ts` (abyme `parts.server.ts` `ENTRIES`).

The event format was agreed with `spec@bevy-build` on 2026-10-22.

## Goals / Non-Goals

**Goals:**
- One command that turns a project into a folder another person can run on the same platform.
- Progress the studio can show as steps, a bar while cargo builds, logs, and clickable problems.
- The shipped game runs the same engine code as `bevy-ts dev`, minus dev tooling.

**Non-Goals:**
- Cross builds. Each engine is built on its native runner in CI; doing it on one machine needs either downloading other platforms' engine packages or cross toolchains, a later change.
- A macOS `.app`, icons, signing, notarisation, installers.
- Bundling scripts into one file, minifying, or a snapshot.

## Decisions

1. **The folder is the engine plus `game/`.** The executable is the release engine renamed after the game; `game/` beside it holds `game.json`, `scripts/` and `assets/`. The same binary still runs `dev`, `types` and the rest; only a bare start changes. Alternative: append the game to the executable as a payload. Rejected: platform-specific, and a folder is what Godot and Bevy games already ship.

2. **A bare start plays `game/` beside the executable.** `std::env::current_exe()`'s directory, never the working directory, so a double-click works. `bevy-ts play <folder>` is the same path with a named folder, for tests and for trying a build without copying it. With no `game/`, the bare start keeps today's usage and exit 2.

3. **Scripts ship as the compiled module set, file by file.** `Compiler::compile_graph` already yields the modules; the build writes each module's code to `game/scripts/<path relative to the root>` with its original name (`src/enemy.tsx` holding JS). The player reads them into a `ModuleSet` whose paths are those files, and sends one `DevEvent::Compiled` on the existing channel. Nothing new is needed in the runtime or the loader. Alternative: one bundled file. Rejected: needs a bundler; the loader already handles a module graph.

4. **Release compile.** A `Compiler` flag for release: production JSX (`dev = false`), and the source map rewritten with `sources` relative to the root and no `sourcesContent`, kept inline so stack traces still name the TS file and line. The game ships no TS text.

5. **Non-dev runtime.** The player builds the app with `base_app` and `BevyTsPlugin::dev(false)`, no `BevyTsRemotePlugin`, no tsx or levels protocol methods, no inspector, and an asset plugin with `watch_for_changes_override: Some(false)`. `base_app` takes a `dev` argument instead of hard-coding `.dev(true)`. `dev(false)` has run only in unit tests so far; the shipped-game test is its first end-to-end run (risk below).

6. **The engine.** No Rust: copy `current_exe()` (the prebuilt engine running the build). With Rust: `engine::obtain` with `Profile::Release`, as today. In a project with Rust the build runs in the prebuilt binary, which is the supervisor; it never runs inside the project engine.

7. **Events through one reporter.** `build.rs` owns a `Reporter` with two modes, JSON lines on stdout or text on stderr. `engine::obtain` takes an output sink instead of inheriting stdio. Under a build, cargo runs with `--message-format=json-render-diagnostics`: `compiler-artifact` messages count `done`, rendered diagnostics and stderr lines become `log`. `total` is the number of packages in `cargo metadata --format-version 1`'s resolve, read once before the build; `done` is clamped to it. `bevy-ts dev`'s supervisor keeps today's plain output through the same sink.

8. **Atomic folder.** Write into `<out>.partial-<pid>` beside `<out>`, then remove the old `<out>` and rename. On failure remove the partial folder. Same filesystem, so the rename is cheap.

9. **Targets.** A table of the five npm platforms (id, label). `host` is `engine::TARGET`. Only the host is ready. The list is the one place a later cross-build change makes others ready.

10. **`bevy-ts.json` gains `name` and `entry`.** Both optional. `entry` is relative to the project root.

## Risks / Trade-offs

- [`dev(false)` is little exercised] → the shipped-game integration test plays the arena for a few seconds headless and checks the TS systems ran; any dev-only assumption it trips is fixed in this change.
- [Production JSX may differ from dev JSX for tsx] → the arena test covers the TSX path; if tsx needs dev JSX source sites, keep dev JSX and drop only the source text.
- [cargo's package count is not its unit count] → `total` is called approximate in the spec; `done` is clamped so a bar never passes 100%.
- [Linux games need the system libraries Bevy links (ALSA, udev, Vulkan)] → `docs/build.md` lists them; not packaged.
- [macOS quarantines a downloaded unsigned binary] → documented; signing is a later change.
- [BREAKING: `bevy-ts build` no longer prints a project engine's path] → nothing in bevy-ts or abyme's Bevy app calls it today (searched 2026-10-22); `docs/native.md` is updated.
