## 1. Rust: the delayed op

- [x] 1.1 `crates/bevy-ts/src/commands.rs`: `OP_DELAYED = 33` and its row in the module's format table; `Op::Delayed { queues: Vec<(Duration, Vec<Op>)> }`; `decode` reads `n u16` × (`kind u8`, `value f64`, `len u32`, body), makes each `Duration` with `Duration::from_secs_f32` (kind 0) or `try_from_secs_f64` (kind 1), an out-of-range value an `Err` naming `secs`/`duration`, merges equal `Duration`s in first-use order and decodes each body recursively; a compile-time check that `Op: Send`
- [x] 1.2 `commands.rs` `apply_one`: `Op::Delayed` reads `Time`'s elapsed time and `Generations::live`, and spawns per queue `(DelayedCommandQueue { submit_at: elapsed + delay, queue }, TsSpawned)`, the queue one command that, when fired, applies the ops with the world's `CommandKinds` and `NativeRecipes` and logs warnings to `ScriptLog` under "delayed commands of <label>"; under another live generation it applies nothing and logs "delayed commands from generation <n> dropped by a reload (<k> commands)"
- [x] 1.3 `crates/bevy-ts/src/generation.rs` `Applying::reset`: before the `TsSpawned` despawn, silence the drop warning of each `DelayedCommandQueue` with `TsSpawned`
- [x] 1.4 `commands.rs` unit tests: `delayed_queues_merge_by_duration` (two `secs` that round to one `f32`, and `secs(1)` with `duration(1)`, make one queue each way Bevy would), `a_nested_delay_decodes`, `a_bad_delay_fails_decode`

## 2. JS and types

- [x] 2.1 `crates/bevy-ts-v8/js/core.js`: `OP_DELAYED`; `Commands.delayed()` returning a `DelayedCommands` (`secs(s)`, `duration(s)`; `s` finite and `>= 0`, else `RangeError` naming the method and value; `secs` sends `Math.fround(s)`); a `DelayedQueue extends Commands` writing its own buffer and borrowing the parent's `alloc`, `reserve`, `kinds`, `res`; `end()` appends one `OP_DELAYED` per `DelayedCommands` in `delayed()` order, a nested one into its parent queue; use after the run throws "delayed commands are used in the run that made them"; then `node scripts/sync-runtime.mjs` in `packages/bevy-ts`
- [x] 2.2 `packages/bevy-ts/index.d.ts`: `delayed(): DelayedCommands` on `Commands`; `interface DelayedCommands { secs(seconds: number): Commands; duration(seconds: number): Commands }`, each with a one-line doc naming Bevy's method and that the clock is the game clock
- [x] 2.3 `packages/bevy-ts/test/delayed_commands.ts` (under `tsc -p test`): `commands.delayed().secs(1).entity(e).despawn()` types; `@ts-expect-error` on `secs("1")` and on `delayed().spawn`

## 3. Tests

New `crates/bevy-ts-cli/tests/it/delayed_commands.rs`, `mod delayed_commands;` in `main.rs`, per `docs/testing.md` (no fixed waits; frames driven by `TimeUpdateStrategy::ManualDuration`).

- [x] 3.1 `bevys_test_matches_rust`: Bevy's `delayed_queues_should_run_with_time_plugin_enabled` calls from a TS `Startup` system in one app and from Bevy's own Rust system in another, both at 0.2 s frames; the `Dummy` count equal on each of 10 frames, and the `DelayedCommandQueue` count equal after frame 0; a TS query reads each queue's `submit_at` as Rust does
- [x] 3.2 `spawn_now_despawn_later`: `secs(1).spawn` returns `e`; `secs(2).entity(e).despawn()`; `e` absent, then present, then gone; a non-delayed `entity(e).insert` before 1 s warns that `e` does not exist
- [x] 3.3 `delays_follow_the_game_clock`: a game pause of `Time<Virtual>`, `setRelativeSpeed(0.5)`, and the editor's `bevy-ts/pause` each hold the delay, against the same delay from Rust
- [x] 3.4 `fixed_update_delays_count_from_the_fixed_clock`: a delay queued in `FixedUpdate` from TS and from Rust fire on the same frame
- [x] 3.5 `nested_delays_add_up`, `every_command_kind_can_wait` (`write`, `trigger`, `runSystem`, `withChildren`, `spawnBatch` on a delayed `Commands`, each seen after the delay and not before; `setState` is not public on a TS `Commands`, a `NextState` param writes it, so it has no delayed form)
- [x] 3.6 `a_bad_delay_throws` (`secs(-1)`, `secs(NaN)`, `duration(Infinity)`: the message names the method and value, a caught throw keeps the run's other commands), `a_throwing_run_drops_its_delayed_commands`, `delayed_commands_kept_past_the_run_throw`
- [x] 3.7 `a_stop_drops_pending_queues` (no queue after Play, no drop warning) and `a_reload_drops_pending_queues` (the one warning, nothing applied)

## 4. Docs

- [x] 4.1 `docs/time.md`: a "Delayed commands" section (the API, the game clock, pause and speed, `FixedUpdate`, spawn now, Stop and reload drop, `DelayedCommandQueue` in queries); drop it from "Not yet built"
- [x] 4.2 `docs/ecs.md` "Commands": `delayed` in the list, a line pointing at `docs/time.md`
- [x] 4.3 `docs/research/bevy-coverage.md` row 95 and item 3 at line 194: delayed commands covered, with the tests
- [x] 4.4 `openspec/changes/script-time`: stage 4 in `proposal.md`, `design.md`'s coverage row and `tasks.md` §8 point at this change

## 5. Gate

- [x] 5.1 `openspec validate delayed-commands --strict`
- [ ] 5.2 The repo's full check green (cargo through `~/dev/abyme/scripts/gate-slot`); no protocol change
