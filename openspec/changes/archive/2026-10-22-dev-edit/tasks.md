## 1. Engine mode state

- [x] 1.1 Add `bevy_ts::EngineMode` (`Play` default, `Edit`) as a Bevy `States` in `crates/bevy-ts`, registered by `BevyTsPlugin`
- [x] 1.2 `Generations.started` (true by default); gate `run_startups` on it; a `restart` reload in edit sets it false
- [x] 1.3 Clear `ObserverQueue` in `First` while `EngineMode::Edit`
- [x] 1.4 `bevy-ts/pause { paused: false }` fails before start (`crates/bevy-ts-remote/src/lib.rs`)

## 2. The --edit boot

- [x] 2.1 `--edit` in `DevArgs`, the usage line and `parse_dev`; refuse with `--headless`
- [x] 2.2 `run_dev`: initial state `Edit`, `started` false, clock paused before `app.run()`
- [x] 2.3 `bevy-ts-viewport`: `Startup` enters edit when the initial state is `Edit`; `set_mode` drives `NextState<EngineMode>`
- [x] 2.4 First `enter_play` and first `step` set `started` and run the due `Startup` systems before the stages
- [x] 2.5 Confirm `supervise.rs` passes `--edit` to a project engine and its restarts

## 3. Protocol

- [x] 3.1 `ViewportModeResult.started` in `protocol/bevy-ts.openrpc.json`, the next free minor on main at landing (1.18.0 or 1.19.0); tell lead@bevy-ts which; regenerate Rust and TS types
- [x] 3.2 `mode_json` answers `started`

## 4. Tests

- [x] 4.1 `crates/bevy-ts-viewport/tests/edit.rs`: boot in edit runs no TS system or `Startup`; first play runs `Startup` once; edit then play does not rerun it; step before start runs it and stays in edit
- [x] 4.2 Edit-time triggers do not fire on play
- [x] 4.3 A native system gated on `in_state(EngineMode::Play)` stays still in edit
- [x] 4.4 A reload before the first Play adds a `Startup` that runs at play
- [x] 4.5 `crates/bevy-ts-cli/tests`: `--edit` with a window shows the arena's level from the editor camera, enemies at their level transforms; `--edit --headless` exits 2 (run under `--viewport`: nothing opens on the operator's screen)
- [x] 4.6 `pause false` before start fails

## 5. Docs

- [x] 5.1 `docs/viewport.md`: "An engine that is always up" launches with `--edit`; the studio note on `started` and the `BEVY_TS` gate
- [x] 5.2 `docs/native.md`: gating native systems on `EngineMode`
- [x] 5.4 Examples: the arena's sun and the cube with its sun move into their TSX scenes, so edit shows them (lead@bevy-ts, 2026-10-22)
- [x] 5.3 Workspace gate once, last: build, tests, clippy
