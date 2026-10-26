## Context

TS commands are a binary arena JS writes during a run (`crates/bevy-ts-v8/js/core.js:4892`,
`class Commands`) and Rust decodes and applies right after it (`runtime.rs:49` `apply_output`,
`commands.rs:308` `decode`, `:544` `apply`). Every `Commands` method is an op through
`Commands.op()`, including tsx's `mount` (a plugin op). `spawn` takes a real id from a pool
allocated with `EntityAllocator::alloc`; a used id leaves the pool only when the run's commands
apply.

Bevy 0.20's delayed commands (`bevy_time/src/delayed_commands.rs`): `Commands::delayed()` gives a
`DelayedCommands` holding a `HashMap<Duration, CommandQueue>` (:17). `duration(d)` and `secs(f32)`
(:24-39) return a `Commands` rebound to the queue for that `Duration`. On drop (:107) it queues
one command that reads `Time` (`Time<()>`), adds its elapsed time to each delay and spawns each
queue as a `DelayedCommandQueue` entity (:42-59). `TimePlugin` runs `check_delayed_command_queues`
in `PreUpdate` (:146-159; `lib.rs:88`): a queue with `submit_at <= elapsed` is appended to its
`Commands` and its entity despawned. `silence_delayed_command_queues_on_exit` runs in `Last` on
`AppExit` (:163). Entity ids are allocated at once (:84-96).

Neighbours: script-time owns the clocks and named this as its stage 4 (`docs/time.md:118`).
writable-native-params owns native params' deferred commands. reflect-coverage owns the
`DelayedCommandQueue` row of the no-default list (`reflect_value.rs:93`).

## Goals / Non-Goals

**Goals:** Bevy's whole delayed-commands API from TS, under Bevy's names, on Bevy's own queue
component and system, so timing matches Bevy's by construction.

**Non-Goals:** a choice of clock per delay (Bevy has none: "a limitation of the system").
Cancelling a queue (Bevy has none; despawning its entity from Rust works as in Bevy). Keeping a
queue across a reload. `TransformHelper` (see Open Questions).

## Coverage

| Bevy 0.20 | TS | Proved by |
|---|---|---|
| `DelayedCommandsExt::delayed(&mut Commands)` | `commands.delayed()` on every `Commands` | `delayed_commands.rs::bevys_test_matches_rust` |
| `DelayedCommands` | `DelayedCommands` (interface in `index.d.ts`) | `test/delayed_commands.ts` |
| `DelayedCommands::secs(f32)` | `secs(s)`, `s` rounded to `f32` (`Math.fround`), then Bevy's `Duration::from_secs_f32` | `bevys_test_matches_rust` |
| `DelayedCommands::duration(Duration)` | `duration(s)`, seconds as every `Duration` (`docs/time.md`), `Duration::try_from_secs_f64` | `bevys_test_matches_rust` |
| the returned `Commands<'w, '_>` | a TS `Commands`: every method, `entity(e)` and `delayed()` | `every_command_kind_can_wait`, `nested_delays_add_up` |
| queues deduped by `Duration` (:17) | per `DelayedCommands`, by the `Duration` Rust makes | `bevys_test_matches_rust` (queue count); `commands.rs::delayed_queues_merge_by_duration` |
| `#[must_use]` on `secs`/`duration` | no TS equivalent; an unused `Commands` queues nothing | — |
| `submit` on drop (:42-59), `submit_at = Time.elapsed() + delay` | at the run's end; Rust builds the same `DelayedCommandQueue` against `Time` when the run's commands apply | `bevys_test_matches_rust`, `fixed_update_delays_count_from_the_fixed_clock` |
| immediate entity allocation (:84-96) | `spawn` returns the entity at once, from the pool | `spawn_now_despawn_later` |
| `DelayedCommandQueue { submit_at, queue }` | the reflected `DelayedCommandQueue` (already `bevy.js:456`): `submit_at` in seconds, `queue` not visible (Bevy's `reflect(ignore)`); inserting it from TS fails, as today (no default) | `bevys_test_matches_rust` (a TS query reads `submit_at`) |
| `check_delayed_command_queues` in `PreUpdate` | Bevy's own, untouched | every timing test |
| the default clock (`Time` in `PreUpdate` = `Time<Virtual>`) | same: pause, relative speed and the editor's pause all hold a delay | `delays_follow_the_game_clock` |
| `silence_delayed_command_queues_on_exit` | Bevy's own on `AppExit`; bevy-ts also silences TS queues on a restart | `a_stop_drops_pending_queues` |
| `Duration::from_secs_f32` panics (negative, non-finite, overflow) | `secs`/`duration` throw at the call, naming the method and value | `a_bad_delay_throws` |

Test files are under `crates/bevy-ts-cli/tests/it/` unless named in a crate's `src`.

## Decisions

**Bevy's queue, bevy-ts's ops.** A delayed queue is a Bevy `DelayedCommandQueue` whose
`CommandQueue` holds one command: apply these decoded TS ops. Bevy's system decides when. Rejected:
a TS-side timer list checked by bevy-ts each frame. It would copy Bevy's timing, clock choice and
order, and drift from them; the parity test would be testing a port. Rejected: calling Bevy's
`commands.delayed()` from Rust at apply time. It works, but the queue entity must carry
`TsSpawned` for a restart to find it, and Bevy's `submit` spawns a bare `DelayedCommandQueue`.
bevy-ts does what `submit` does in four lines: read `Time`'s elapsed time, add the delay, spawn
`(DelayedCommandQueue { submit_at, queue }, TsSpawned)`. The fields are `pub`.

**One op, nested.** `OP_DELAYED = 33`: `n u16`, then per queue `kind u8` (0 `secs`, 1
`duration`), `value f64`, `len u32`, that many bytes of ordinary ops. Rust turns each `(kind,
value)` into a `Duration` with Bevy's `from_secs_f32` or `try_from_secs_f64`, merges queues with
equal `Duration`s in first-use order (Bevy's dedupe, exact: two `f64`s that round to one `f32`
share a queue), and decodes each body with `decode` into `Op`s now, so a bad body fails the run's
commands as any bad op does. A body may hold `OP_DELAYED` again: nested delays add up, as Bevy's
do, because the inner queue is spawned when the outer one fires. Ops of a queue are `Send`
(checked when `Op::Delayed` is added), as a `CommandQueue` command needs.

**JS: a delayed `Commands` is a `Commands` with its own buffer.** `commands.delayed()` returns a
`DelayedCommands` holding its parent and a list of `(kind, value, queue)`. `secs(s)` and
`duration(s)` check `s` (finite, `>= 0`, else a `RangeError` naming the method and value; Rust's
range check is the backstop), and return the queue for `(kind, value)`, made on first use: a
`DelayedQueue extends Commands` whose `op()` appends to its own byte list and whose `alloc()`,
`reserve()`, `kinds` and `res` are the parent's. So a delayed spawn uses the run's pool, and its
id leaves the pool when the run's commands apply, as any spawn's. At `end()`, the root `Commands`
appends one `OP_DELAYED` per `DelayedCommands`, in the order `delayed()` was called; a nested
`DelayedCommands` appends to its parent queue's buffer instead. A run that never calls `delayed()`
pays one length check. Each object records the run it was made in; a call after that run throws
"delayed commands are used in the run that made them".

**Clock: whatever `Time` is when the run's commands apply.** As in Bevy: in `Update` that is
`Time<Virtual>`'s elapsed time; in `FixedUpdate`, `Time<Fixed>`'s, which lags the virtual clock
by under one timestep. The check is always against `Time` in `PreUpdate`, the virtual clock. So a
pause (the game's, or the editor's: `ScriptClock` pauses `Time<Virtual>`) holds every delay, and a
relative speed of 0.5 doubles it in wall time. bevy-ts adds no clock rule.

**Firing.** The queue's command runs inside Bevy's command apply in `PreUpdate`, with `&mut
World`. It reads `CommandKinds` and `NativeRecipes` from the world, then calls `commands::apply`
and logs each warning to `ScriptLog` under "delayed commands of <system label>". bevy-ts's own
queues (`PendingRuns`, the observer queue) drain at the next bevy-ts stage boundary, so a delayed
`runSystem`, `trigger` or `observe` acts there, as a Rust command's would.

**A queue belongs to its generation.** Ops carry generation indices (message, event and state
indices into `TsPayloads`, plugin opcodes) and TS layout bytes, all of which a reload may change.
The command records `Generations::live` when it is made; firing under another generation applies
nothing and logs one warning, "delayed commands from generation 4 dropped by a reload (3
commands)". A restart despawns `TsSpawned` entities already; `Applying::reset` first silences
each TS queue's drop warning (`CommandQueue::silence_drop_warning`), since a Stop resets
`Time<Virtual>` (`reset_clock`) and a queue counted on the old clock must not survive it. Ids
reserved by a dropped queue's spawns stay allocated and unused, as when Bevy drops a queue.
Rejected: firing a stale queue anyway (wrong indices or layouts write the wrong thing). Rejected:
re-resolving ops by name at fire time (a second decoder for a dev-only edge).

**Visibility.** Queue entities are ordinary entities, as in Bevy: a TS `Query<[Entity]>` sees
them, a query for `DelayedCommandQueue` reads `submit_at`. They are not `ScriptHidden`: Bevy hides
nothing, and a game may want to count pending waves.

## Risks / Trade-offs

- [A reload drops pending delayed commands] → a warning names it; a Stop/Play or a restart clears
  state anyway. Keeping them is a later change if it bites.
- [Queues due on one frame have no order] → Bevy's answer (its `HashMap` drain); within a queue,
  call order holds. The parity test counts, it does not order.
- [`secs` rounds through `f32`] → as Bevy; `duration(s)` is exact to the nanosecond.
- [A delayed `cloneAndSpawn` copies the source when the queue fires, not when called] → as Bevy's.

## Migration Plan

Additive. No protocol change, no data migration. Rollback is a revert.

## Open Questions

- `TransformHelper` is one method (`compute_global_transform(entity)`, `bevy_transform/src/
  helper.rs:28`) and fits an `#[export(stands_for = "TransformHelper")]` param, but it is a
  transform item, not time or commands. It goes in its own small change, not this one.
