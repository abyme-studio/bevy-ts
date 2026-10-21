## 1. Project and compile

- [x] 1.1 `project.rs`: optional `name` and `entry` in `bevy-ts.json`, with parse tests
- [x] 1.2 `compile.rs`: a release compile (production JSX, source map with root-relative `sources` and no `sourcesContent`); test that the arena's compiled modules hold no TS source text
- [x] 1.3 Entry and name resolution (`--entry`, `entry`, the four defaults; `name`, else the folder name), with a test for the "no entry" error naming the four files

## 2. Playing a game folder

- [x] 2.1 `base_app` takes `dev` instead of hard-coding `.dev(true)`; `headless` gains an unwatched asset plugin
- [x] 2.2 `play.rs`: read `game.json` (refuse a different `bevyTs`), load `game/scripts/` into a `ModuleSet`, send one `DevEvent::Compiled`, run with `dev(false)`, no remote, no protocol methods, no inspector
- [x] 2.3 `cli.rs`: `bevy-ts play <folder>`, and a bare start that plays `game/` beside `current_exe()` or prints the usage and exits 2
- [x] 2.4 Integration test: a hand-made game folder for the cube plays headless for a few seconds, its TS system runs, and nothing listens on 15702

## 3. Engine output through a sink

- [x] 3.1 `engine::obtain` takes an output sink; `bevy-ts dev`'s supervisor and `types` keep today's stderr output through it
- [x] 3.2 Under a build, run cargo with `--message-format=json-render-diagnostics`, count `compiler-artifact` messages as `done`, read `total` from `cargo metadata`'s resolve, clamp, and pass other lines on as logs

## 4. `bevy-ts build`

- [x] 4.1 `build.rs`: the `Reporter` (JSON lines on stdout, or text on stderr with the executable path on stdout) and the four steps, replacing `supervise::build`
- [x] 4.2 Steps: compile scripts (problems with absolute file, 1-based line and column), prepare the engine (copy `current_exe()` or `obtain` release), copy `assets/`, write `game.json` and the executable
- [x] 4.3 Write into `<out>.partial-<pid>` and rename onto `<out>`; remove the partial folder on failure
- [x] 4.4 `--targets [--json]`: the five platforms, host ready, the others with "cross builds are not supported yet"; `--target` validation before any step
- [x] 4.5 Usage text in `cli.rs` for `build` and `play`

## 5. Tests

- [x] 5.1 Integration: `bevy-ts build examples/arena --json --out <tmp>`; every line parses, four steps, last `done`; then `bevy-ts play` the folder headless and see the arena's systems run
- [x] 5.2 Integration: a script with two errors gives two `problem` events and `failed`, exit 1, an earlier `<out>` unchanged and no partial folder left
- [x] 5.3 Integration: `--targets --json` and a non-host `--target`
- [x] 5.4 Build `examples/thrusters` once by hand and record the `progress` events seen (cargo is too slow for the suite)
- [ ] 5.5 Start a built arena by double-click from Finder on the operator's Mac and record it in `docs/build.md`

## 6. Docs

- [x] 6.1 `docs/build.md`: the command, the folder, the events, targets, and what a player's machine needs (Linux libraries, macOS quarantine)
- [x] 6.2 `docs/native.md` and the README's command list point at `docs/build.md`
- [x] 6.3 Tell `spec@bevy-build` if anything in the event format moved during implementation (nothing moved)
