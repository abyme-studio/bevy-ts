## Why

Stop does not stop today. Going back to edit after a Play leaves the world as the game left it, with `started` still true (edit-mode, "Back to edit before E4"). The studio is being rebuilt on Unity's frame (the bevy studio first-principles study, change E4, operator's go 2026-10-22). There, Stop throws the play world away and the scene is back as the source describes it. The studio's play mode (B4) needs that, and a stream that tells it the mode changed.

## What Changes

- **Stop rebuilds the edit world from source.** Every switch from play to edit, paused or not, throws the play world away and rebuilds it from the live generation and the files on disk. It keeps no copy of the world in memory: the engine holds nothing unsaved (`level_write` writes before it replies; unsaved TSX text is the studio's). The rebuild is the `restart` reload's path, re-applying the live generation instead of recompiling it.
- **What Stop resets:**
  - every entity TS spawned, every TSX node and tombstone;
  - which `Startup` systems ran, and `started` (false, so the next Play runs `Startup` again);
  - `Local` storage, TS resources (back to their `insertResource` values), TS states and pending messages;
  - V8, with a fresh isolate;
  - `Time<Virtual>`, `Time<Fixed>` and the script frame count, so the next Play starts at 0, as Unity's `Time.time` does.
- **Stop reopens the levels open at Play**, each with its opener, and closes any a client opened during play. Each level reconciles before Stop replies.
- **The selection follows by node id.** A selected level or TSX entity is selected again after Stop. One the game spawned is dropped. The editor camera, tools, `ui` and `gamepadUnfocused` are kept.
- **BREAKING (behaviour, not schema):** edit then play no longer resumes where play stopped. `bevy-ts/pause` stays the way to freeze a running game. The studio puts every game it finds into edit once (operator ruling 2026-10-20), so a game the operator started outside abyme and already played is rebuilt when the studio finds it.
- **A `restart` reload resets TS resources too.** Today they survive a restart, which is not "as if first". States and messages already reset.
- **Native crates reset their own state** in `OnExit(EngineMode::Play)`, the Bevy way. bevy-ts does not despawn or reset what Rust spawned. `docs/native.md` says how.
- **A Play can fail.** `viewport_mode { mode: "play" }` answers an error and stays in edit when there is no generation yet (`no-generation`), or when the newest compile failed (`compile-errors`), as Unity refuses Play with compiler errors. The error's message is a sentence the studio shows as is.
- **Protocol: `bevy-ts/viewport_mode+watch`.** It sends the mode (`mode`, `ui`, `gamepadUnfocused`, `paused`, `started`) on open and on every change, with a `cause`: `engine`, `client`, `restart` or `failed`. `viewport_mode`, `pause` and `step` are unchanged as calls. Additions only, the next free minor on main at landing (1.24.0: 1.20 to 1.23 were taken). Agreed with build@studio-edit-and-levels, 2026-10-22.
- **Out of scope:** the game camera while editing (a later stage, see design). E5's per-user editor state. The studio's play mode (B4, abyme).

## Capabilities

### New Capabilities

None.

### Modified Capabilities

- `edit-mode`: Stop rebuilds the world; Play can fail; the mode is streamed; levels and the selection come back after Stop; native crates reset on leaving Play.
- `hot-reload`: a `restart` reload also resets TS resources, states and pending messages.
- `ts-systems`: a resource keeps its value across a reload, but not across a restart or a Stop.

## Impact

- `crates/bevy-ts`: `generation.rs` keeps the live `ModuleSet` and gains a rebuild that re-applies it as a restart; `restart` despawns TS resource entities so `insert_resources` re-inserts them; Time and frame reset.
- `crates/bevy-ts-levels`: the set of open levels and their openers as a value, restoring it, and reconciling dirty levels now.
- `crates/bevy-ts-viewport`: `edit.rs` `set_mode` runs Stop and refuses a failing Play; `viewport_mode+watch`; the selection after Stop.
- `crates/bevy-ts-remote`: the mode watch when `pause` or `step` change `paused` or `started`.
- `protocol/bevy-ts.openrpc.json`: `bevy-ts/viewport_mode+watch`, `ViewportModeWatch`, `ViewportModeCause`, the Play error's data; the next free minor; regenerated Rust and TS types.
- `docs/viewport.md` (Play and Stop, the watch, for the studio), `docs/native.md` (resetting in `OnExit(Play)`), `docs/research/stop-cost.md` (the measurement).
- abyme: the studio's play mode (B4) subscribes to the watch and shows a failed Play's message. An engine older than this minor answers method-not-found for the watch.
- Needs the operator's go:
  - **Performance.** Every Stop costs a restart: a fresh isolate, the TS generation applied again and every open level remounted. Levels alone take 37 ms at 1k instances and 271 ms at 10k (`docs/research/levels-scale.md`). The change measures Stop at 1k and 10k before it lands.
  - **The game camera while editing**, which E4 as approved included. It is left out here: showing it needs a second camera on `RenderTarget::Image` and a second shared surface to abyme, which is new infrastructure.
- No new dependency. No new pattern: Stop reuses the restart path, `OnExit` and the Bevy state E1 added.
