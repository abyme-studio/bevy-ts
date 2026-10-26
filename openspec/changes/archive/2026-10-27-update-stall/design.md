## Context

`research.md` traced the hang: Metal's submit thread blocks in the driver, wgpu's queue fills at 4096 command buffers, the render thread waits on encoders, and the main thread waits in `renderer_extract`. `app.update()` never returns. Every bound we have runs inside Update: the screenshot's 5 s, 60 s and 60 s timeouts (`crates/bevy-ts-remote/src/screenshot.rs`), the `BEVY_TS_SCREENSHOT` "shooting anyway" warning, and the tests' `common::until`. None fires.

Tests already have a wall-clock bound, `common::watchdog` (`crates/bevy-ts-cli/tests/it/common/mod.rs:46`): a thread per test that exits the binary after `BOUND`. It is opted into test by test (`ui_widgets`, `ui_placement`, `screenshot_ready`'s `Hang`, and on `ts-heap-fields-2`, f39e9268b, `ambient_light`). An engine run has nothing.

## Goals / Non-Goals

**Goals:**
- One watchdog, in the engine, that bounds a stalled frame by wall time in every run and every test, and says plainly that Update stopped and where.
- A protocol client gets an answer instead of an open socket.
- A regression test that makes the stall on purpose, with no GPU load.

**Non-Goals:**
- Fixing the driver stall or the GPU page faults (`research.md`, fix 3).
- Counting headless renders as heavy runs in gate-slot (`research.md`, fix 2): abyme's change, not bevy-ts's.
- Changing the screenshot method's own timeouts.

## Decisions

### 1. One owner: `bevy_ts_remote::stall`

A new module `crates/bevy-ts-remote/src/stall.rs` owns the concept: the beats, the watchdog thread, the report, the answers to open requests. It lives in `bevy-ts-remote` because it must reach the protocol mailbox, and that crate already depends on the render world (`screenshot.rs`). `common::watchdog` and `screenshot_ready`'s `Hang` are deleted, so there is one watchdog, not two.

Alternative: keep `common::watchdog` for tests and add an engine one. Rejected: two owners of "this run is hung", and a test still hangs when its author forgets the line.

### 2. Beats, not a frame counter

The research proposed a counter bumped in `First`. A counter cannot tell a stalled frame from an app no one is updating: a test that holds its app idle for 30 s would be killed. So the plugin records four beats in a shared `Arc` of atomics (frame number and nanoseconds since the watchdog's start):

- `First` begins: frame N's main part opens.
- `ExtractSchedule` (on the main thread, after the render world came back): the main part closes, the render part opens. With no render world, the end of `Last` closes the main part.
- The end of the render world's `Render` schedule (after `RenderSystems::Cleanup`): the render part closes.

The watchdog thread wakes every `limit / 10` (at least 100 ms) and compares. A part open for the limit is a stall. The stuck place follows from the beats:

| Open part | Says |
|---|---|
| main, `Last` not reached | the frame's systems have not finished: a system or a script is not returning |
| main, past `Last` | waiting for the render world: the render of frame N-1 has not finished |
| render | in the render: the GPU is not taking this engine's work |

The second row is the sampled hang (pipelined rendering); the third is the same hang without pipelining. The decision is a pure function of the beats and `now`, so it is unit tested in `stall.rs` with made-up instants.

The clock starts at the first `First`: startup (V8, plugin `finish`, adapter request) does not count.

### 3. What it does: by display, set in one place

`StallPlugin { limit, action }`, `action` one of `Exit(75)`, `Warn` and `Call(Arc<dyn Fn(&Stall) + Send + Sync>)` (for tests). Both are copied into a `StallSettings` resource read when the thread starts at the first frame, so a test may replace them after `base_app`.

`base_app` (`crates/bevy-ts-cli/src/cli.rs`) adds it: `Exit(75)` for `Display::Headless`, `Warn` for `Window` and `Viewport`, nothing with `--inspect`. Exit code 75 is `EX_TEMPFAIL`: the run may pass if run again, which a land check can tell from a test failure. A window warns only: macOS stops frames to a hidden window, and a person can see a frozen window. Under `Warn` the next beat after a stall logs "Update resumed after N s".

`BEVY_TS_STALL_SECS` is read in `base_app`, beside `shot_from_env`: a whole number of seconds, `0` for none. A bad value fails the start, naming the variable.

The report names `std::thread::current().name()` from when the plugin was built: under `cargo test` that is the test's name, which the old `common::watchdog` took as an argument.

### 4. Open requests are answered from the watchdog side

Bevy's HTTP transport puts each request in a channel (`BrpSender`) and waits on a per-request result channel; only a system in Update reads the mailbox and answers. When Update stops, the socket waits for ever.

The plugin puts a forwarder between them. In a startup schedule of its own, inserted between `PreStartup` (where Bevy makes the mailbox) and `Startup` (where the HTTP server clones `BrpSender`), it swaps `BrpSender`'s sender (`DerefMut`) for one of its own channel's. A forwarder thread receives each `BrpMessage`, keeps a clone of its result sender, and passes it on to Bevy's mailbox. On a stall the watchdog sends the stall error (`-32603`, the report line) on every kept sender whose channel is not closed and holds no answer yet; while the stall lasts, the forwarder answers new requests the same way instead of passing them on. Before `Exit`, the thread waits until every answered channel is closed (the transport read it), at most 1 s. Kept senders are dropped once closed.

Alternatives: read Bevy's `RemoteWatchingRequests` (its field is private); drain and re-send `BrpReceiver` from a system (it runs in Update, which is what stopped, and can reorder a full channel). A per-method timeout in the screenshot handler runs in Update too.

### 5. Tests

- `stall.rs` unit tests: the decision function, for each row above and for idle time between frames.
- `crates/bevy-ts-cli/tests/it/stall.rs` (a module of `it`, per `docs/testing.md`): an app from `common::render_app()`, headless plugins, `StallPlugin` with a 1 s limit and `Call` that sends the stall on a channel and then releases a gate. A system in `Render` (or in `Update`) blocks on that gate once the test sets a flag. The test updates until the channel has the stall, with `common::until`'s deadline. The stall is deterministic: nothing depends on the GPU. It asserts the stuck place, the frame, and that it fired after the limit.
- The protocol: the same app with `BevyTsRemotePlugin` on `common::free_port()`. The test posts `bevy-ts/screenshot` over HTTP, blocks the render, and asserts the response is the `Update stopped:` error. A second post made while stalled gets the same.
- No test exits the binary: every watchdog test sets `Call`.

## Risks / Trade-offs

- [A frame that is slow but not stuck, past 30 s] → 30 s is far past any frame we have seen (a synchronous pipeline compile under load took seconds); `BEVY_TS_STALL_SECS` raises it.
- [The forwarder adds a thread hop to every protocol request] → one channel send, microseconds; no request is copied.
- [A Bevy upgrade moves the mailbox or the HTTP startup] → the swap asserts that `BrpSender` exists when it runs and the protocol test fails if requests stop arriving.
- [`exit` skips destructors] → the same as the old `common::watchdog`; a stalled render cannot shut down cleanly anyway.
- [`ts-heap-fields-2` adds a `common::watchdog` call] → whichever lands second drops it.

## Migration Plan

None: no protocol schema or script API change. Rollback is a revert.

## Open Questions

None.
