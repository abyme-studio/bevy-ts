## 1. The reset path (`crates/bevy-ts`)

- [x] 1.1 `LiveSet`: `apply` keeps a clone of the module set it applied; `generation::rebuild(world)` re-applies it as a restart, and answers an error when there is none
- [x] 1.2 The restart branch despawns TS resource entities before the restart handlers, so `insert_resources` inserts them again; Rust resources untouched
- [x] 1.3 A restart whose apply fails leaves the empty generation, the error on the log and `reload+watch`, as a failed first load does
- [x] 1.4 `reset_clock(world)`: a fresh paused `Time<Virtual>` keeping `max_delta` and relative speed, `Time<Fixed>`'s overstep cleared, `ScriptFrame` at 0

## 2. Levels (`crates/bevy-ts-levels`)

- [x] 2.1 `open_set(world)` (path and opener) and `restore_open(world, &set)`: close what is not in it, open what is missing with its opener, sidecars over their scene
- [x] 2.2 `settle(world)`: reconcile every dirty level now, as `open_now` does

## 3. Play and Stop (`crates/bevy-ts-viewport`)

- [x] 3.1 `enter_play` records `open_set` in `EditState`
- [x] 3.2 `set_mode` play → edit: `StateTransition` (native `OnExit(Play)`), `enter_edit`, `rebuild`, `restore_open`, `settle`, `reset_clock`, the selection followed by node id with `origin: "engine"`, then the answer
- [x] 3.3 `set_mode` edit → play refuses first with `no-generation` or `compile-errors` (newest `ReloadLog` entry `failed`), a JSON-RPC error whose message is `file:line: message` and whose `data` has `reason`; nothing changes
- [x] 3.4 `viewport_mode+watch`: on open with `cause: "engine"`; a compare in `Last` against `mode_json` for `client` and `restart`; `failed` from 3.3; one message per Stop

## 4. Protocol

- [x] 4.1 Contract: `bevy-ts/viewport_mode+watch`, `ViewportModeWatch` (`ViewportModeResult` plus `cause`), `ViewportModeCause` (`engine`, `client`, `restart`, `failed`), the Play error's `data` (`reason`: `no-generation`, `compile-errors`); the next free minor on main at landing (1.24.0: 1.20 to 1.23 were taken), told to lead@bevy-ts; regenerated Rust and TS types; CI diff check

## 5. Tests

- [x] 5.1 `crates/bevy-ts-viewport/tests/edit.rs`, one per scenario: toggling to edit and back, Play-Stop-Play reruns `Startup`, Stop from paused, stopping the arena (destroyed enemy back, `Score` reset, `started: false`), a save during play then Stop (no compile), the editor camera kept, time starts at 0
- [x] 5.2 Levels after Stop: a level opened during play closes, one closed during play reopens, openers kept, `level+watch` events
- [x] 5.3 Selection after Stop: enemy `b` followed; a TS-spawned bullet dropped
- [x] 5.4 A failing Play: compile error before Play (`compile-errors`, nothing ran), then fixed and played; `no-generation`
- [x] 5.5 `viewport_mode+watch`: open, Play, Stop, step before start, a refused Play, a `restart` reload in edit
- [x] 5.6 `crates/bevy-ts/tests`: a restart resets `Score` and drops a pending message (hot-reload scenarios); a forced failing re-apply leaves the empty generation (fake backend)
- [x] 5.7 A native crate resetting in `OnExit(EngineMode::Play)`: its counter is 0 at the rebuild; an uncleaned Rust entity survives
- [x] 5.8 `examples/rapier` headless under `--edit`: play, Stop, play gives the same body positions frame for frame

## 6. Measure

- [x] 6.1 `crates/bevy-ts-cli/tests/stop_bench.rs` (`--ignored`, release), the levels bench's level at 1k and 10k instances: Stop's time split into `OnExit`, rebuild (isolate, apply), level settle; and the next Play's `Startup`
- [x] 6.2 `docs/research/stop-cost.md` with the table; if 10k is over 1 s, name the follow-up (the incremental reconcile `levels-scale.md` names)

## 7. Docs

- [x] 7.1 `docs/viewport.md`: Play and Stop (what Stop resets, what it keeps, levels and selection), a failing Play, `viewport_mode+watch`, and for the studio: the one-time switch to edit rebuilds a played game; the minor to check
- [x] 7.2 `docs/native.md`: reset a resource and despawn spawned entities in `OnExit(EngineMode::Play)`, with an example
- [x] 7.3 Workspace gate once, last: build, tests (viewport's render tests one at a time, as the level-open gate does), clippy
