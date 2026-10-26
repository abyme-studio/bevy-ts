## Why

A TS system cannot run a command later. "Spawn the wave in 3 seconds" needs a `Timer` in a
`Local` and a second system today. Bevy 0.20 has `commands.delayed().secs(s)`; it is the last
open `bevy_time` item (script-time's stage 4, `docs/time.md` "Not yet built", coverage row 95).
Its blocker, writable-native-params, has landed.

## What Changes

- `commands.delayed()` on every TS `Commands` (systems, observers, one-shot systems) returns a
  `DelayedCommands` with Bevy's two methods: `secs(s)` and `duration(s)`. Each returns a
  `Commands` whose commands run `s` seconds later. Every method of `Commands` and
  `EntityCommands` works on it, `delayed()` too.
- The delay runs on Bevy's own path: each queue is a Bevy `DelayedCommandQueue` entity,
  `submit_at` = `Time` elapsed + delay, and Bevy's `check_delayed_command_queues` fires it in
  `PreUpdate`. So the clock, pause and relative speed are Bevy's: the game clock.
- `spawn` on a delayed `Commands` returns the entity at once, as Bevy's does; later commands,
  delayed or not, can name it.
- A delay that is negative, `NaN` or infinite throws at the call, naming the method.
- A queue belongs to the generation that made it: a Stop drops it, and so does a reload, with a
  warning.
- `DelayedCommandQueue` reads in TS as the reflected component it is today: `submit_at` in
  seconds, the queue not visible, never inserted from TS.
- An internal command op, `delayed` (33). No protocol change.

## Capabilities

### New Capabilities
None.

### Modified Capabilities
- `ts-systems`: delayed commands on the `Commands` param.

## Impact

- `crates/bevy-ts-v8/js/core.js` (and its synced copy `packages/bevy-ts/runtime/core.js`):
  `Commands.delayed()`, `DelayedCommands`, a delayed `Commands` writing its own buffer, the
  `delayed` op at the run's end.
- `crates/bevy-ts/src/commands.rs`: `OP_DELAYED`, `Op::Delayed`, its decode and apply through
  Bevy's `DelayedCommandQueue`.
- `crates/bevy-ts/src/generation.rs`: a restart silences and despawns TS queues.
- `packages/bevy-ts/index.d.ts`: `delayed()` on `Commands`, `DelayedCommands`.
- Tests: `crates/bevy-ts-cli/tests/it/delayed_commands.rs` (new), `commands.rs` unit tests,
  `packages/bevy-ts/test/delayed_commands.ts` (new).
- Docs: `docs/time.md`, `docs/ecs.md`, `docs/research/bevy-coverage.md`;
  `openspec/changes/script-time` points stage 4 here.
- Operator go: none needed. No AI prompt or tool, no new dependency (Bevy's `bevy_time` is
  already linked), no service, no data access, nothing in production, no protocol change.
  Performance: a run that never calls `delayed()` pays one branch at its end.
