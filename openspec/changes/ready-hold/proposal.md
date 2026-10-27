## Why

Under load, agent town's ready shots come out as one of two or three pictures. The shot's clock is
still 60 whole steps; what moves is the step at which the game's data arrives. The game waits for
abymed's units over a WebSocket. On the updates before they come, nothing is loading and no
pipeline waits, so bevy-ts counts them ready and steps time. The units arrive after k steps (k is
0, 1 or 2), and the walkers' clips get 60 − k (research: `findings.md` of ready-frames-under-load,
13 of 13 runs matched by k). bevy-ts sees loads and pipelines. It cannot see a reply that has not
come yet. Only the script knows what it waits for, so the script must be able to say so.

Separately, some of the lead's runs exited 75 (the stall watchdog). The research guessed a
synchronous macOS pipeline compile. That cannot be it: every bevy-ts engine since alpha.24 builds
the vendored bevy_render, which compiles pipelines in the background on macOS
(`vendor/bevy_render/PATCH.md`), and agent town runs alpha.36. The cause is unknown because agent
town's scripts dropped the `Update stopped:` line. The engine's part is to make that line hard to
lose.

## What Changes

- **A ready hold the script sets.** A new resource, `ReadyHold`, owned by `ReadyPlugin`
  (`bevy-ts-remote::screenshot`): a set of names. While any name is held, no frame is ready. So
  `BEVY_TS_SCREENSHOT` and `bevy-ts/screenshot` count no ready frames, and under the fixed step
  (`BEVY_TS_SCREENSHOT`, `BEVY_TS_FIXED_STEP=1`) Bevy's `TimeUpdateStrategy` stays
  `ManualDuration(Duration::ZERO)`. When the last name is released, frames count again, and time
  steps from the next ready frame.
- **The TS surface.** A built-in module `bevy-ts/ready`, as `bevy-ts/session` is built: a param
  `ReadyHold` with `hold(name)`, `release(name)` and `isHeld(name)`. A game holds in a
  `Startup` system and releases in the system that applies its data:

  ```ts
  import { ReadyHold } from "bevy-ts/ready";
  export function wait(ready: ReadyHold) { ready.hold("abymed"); }
  export function apply(ready: ReadyHold /*, ... */) { /* first records applied */ ready.release("abymed"); }
  ```

  With neither shot nor fixed step, the hold does nothing.
- **A hold never released fails, naming the hold.** It is no progress: after `ReadyTimeout` (60 s)
  with no progress, the env shot logs `<path>: not ready after 60 s with no progress (held by the
  script: abymed); no shot saved` and exits 1; `bevy-ts/screenshot` answers the same reason.
- **The stall line is also an error.** The watchdog keeps its raw `Update stopped:` line on stderr,
  and logs the same line at error level too, so a log filter on errors (agent town's `rror`)
  keeps it. The watchdog's limit and what counts as a stall do not change (design.md, Decisions).
- **For agent town (lead@engine-core), not in this change:** hold `"abymed"` from `connect()` until
  link has applied its first records; count `LABELS_FROM` in steps (`Time` elapsed, or frames with
  `delta_secs > 0`), not Update calls; keep the `Update stopped:` and exit lines in
  `scripts/shot-rounds.sh` and `frame.mjs`.

## Capabilities

### New Capabilities

None.

### Modified Capabilities

- `headless-run`: a held frame is not ready; a hold never released fails the env shot naming it.
- `remote-protocol`: `bevy-ts/screenshot` waits out a hold, and `BEVY_TS_FIXED_STEP=1` time does not
  step while held.
- `stall-watchdog`: the stall line is logged at error level too.

## Impact

- Operator's go: new public API, `ReadyHold` (Rust resource) and the `bevy-ts/ready` module (TS
  param `ReadyHold`: `hold`, `release`, `isHeld`). Cost: one small module, one set check per
  frame in `count_ready` and the method's wait. Alternative: no API, and each game gates its own
  animation start on its data (every game repeats it, and the shot's clock still counts the wait);
  or bevy-ts counts open WebSockets and JS async work as "loading" (does not close the race: a reply
  can land after those frames; research §"Can each wait be deterministic?").
- Code: `crates/bevy-ts-remote/src/screenshot.rs` (`ReadyHold`, `count_ready`, the method's wait,
  `not_ready`); `crates/bevy-ts-remote/src/stall.rs` (error log); new
  `crates/bevy-ts-cli/src/ready.rs`, `crates/bevy-ts-cli/ts/ready.{js,d.ts}`; `crates/bevy-ts-cli/src/cli.rs`
  (adds its plugin); `crates/bevy-ts-cli/src/lib.rs` (`pub mod ready`).
- Tests: `crates/bevy-ts-remote/src/screenshot.rs` unit tests; new `crates/bevy-ts-cli/tests/it/ready_hold.rs`.
- Docs: `docs/build.md` (the hold, beside the shot and the fixed step; the stall line as an error).
- Protocol: no schema change. The method's error text gains `held by the script: <names>`.
- Not in this change, open: the 1 px leaf at (300,374) seen in two of the lead's runs (design.md,
  Open Questions).
