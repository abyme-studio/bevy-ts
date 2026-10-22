## Why

The engine starts running. Edit mode then pauses that live world, after the game's `Startup` and some `Update` frames have run. Unity and Godot open a project the other way: the scene loads from disk and no game code runs until Play. The studio is being rebuilt on Unity's frame (operator, 2026-10-22; bevy studio first-principles study, change E1). This change is the engine's half of that boot.

## What Changes

- **`bevy-ts dev --edit`** starts the engine in edit mode. The TS loads and registers its components, systems and levels. No TS system, observer or `Startup` runs. `Time<Virtual>` is paused from the first frame, so nothing in `FixedUpdate` steps. The levels the game names with `app.level()` load and show from the editor camera.
- **A Bevy state, `bevy_ts::EngineMode`** (`Play`, `Edit`), mirrors the mode. A native crate gates its game systems with `run_if(in_state(EngineMode::Play))`, as Bevy does. `OnEnter`/`OnExit` are the seam E4 (Play snapshots, Stop restores) hooks.
- **First Play starts the game.** The first `viewport_mode { mode: "play" }` runs the game's `Startup`, then simulates. Until E4, going back to edit pauses as today, with no restore. `step` before the game has started runs `Startup` first. `pause { paused: false }` before it has started fails.
- **Triggers raised in edit mode are dropped**, not queued for Play, so a drag in edit never fires game observers later.
- **`ViewportModeResult.started`** (protocol, additions only): whether the game's `Startup` has run in this world. `viewport_mode {}` reads it. No new method.
- A Rust-rebuild restart of an `--edit` engine comes back in edit mode, as Unity's "Stop Playing and Recompile".
- **Out of scope:**
  - opening a level with no `app.level()` call: `level_open`/`level_close`, team level-open (E2);
  - Play snapshots and Stop restores (E4);
  - levels that survive missing code (E3);
  - the studio's launch line and its Play button (abyme).

## Capabilities

### New Capabilities

None.

### Modified Capabilities

- `edit-mode`: booting in edit mode; the `EngineMode` state; first Play runs `Startup`; edit drops triggers; `started` in the mode result.
- `shared-surface`: simulation runs from start unless `--edit`; the protocol version that adds `started`.
- `hot-reload`: `Startup` waits for the game to start.
- `remote-protocol`: `pause { paused: false }` fails before the game has started.

## Impact

- `crates/bevy-ts-cli`: `cli.rs` (`--edit`, refused with `--headless`), `dev.rs` (edit before the first frame), `supervise.rs` (flag passes through to a project engine and its restarts).
- `crates/bevy-ts`: `EngineMode` state, `Generations.started`, `run_startups` gated, observer queue cleared in edit.
- `crates/bevy-ts-viewport`: `edit.rs` (`set_mode` drives `EngineMode`, first Play runs `Startup`), `methods.rs`, `tests/edit.rs`; `crates/bevy-ts-cli/tests` for the boot.
- `crates/bevy-ts-remote`: `pause`/`step` before start.
- `protocol/bevy-ts.openrpc.json`: `ViewportModeResult.started`, the next free minor on main at landing: 1.18.0 if this lands before level-open, else 1.19.0 after a rebase. Regenerated Rust and TS types.
- `docs/viewport.md` ("An engine that is always up", the studio note), `docs/native.md` (gating native systems), the CLI usage line.
- Behaviour change for existing edit mode, needs the operator's go: TS triggers raised while in edit mode are dropped instead of firing on resume.
- abyme, agreed with spec@studio-edit-and-levels: the studio adds `--edit` to its launch line in `apps/bevy/plugins/bevy-ts/server.ts`, gated on the pinned `BEVY_TS`, and keeps its one `viewport_mode { mode: "edit" }` after `hello` for every game (operator ruling 2026-10-20), a no-op on an `--edit` engine. The game shell's Play/Stop/Pause/Step need no change.
- No performance cost: edit runs fewer systems than today's paused world.
