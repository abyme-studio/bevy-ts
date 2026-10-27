# Writing a test

How a new test is written here. Where tests live and what they cost to build is
`docs/build-cache.md`, "Test binaries". The rules come from abyme's
`docs/research/test-suite-health.md`.

## Where it goes

- A new test file is a module of its crate's one binary: `crates/<crate>/tests/it/<name>.rs`, and a
  `mod <name>;` line in `tests/it/main.rs`. Never a new `tests/<name>.rs`: that links a binary of its
  own, a copy of Bevy and V8.
- Every module shares one process, and their tests run at once. Nothing a test does may reach
  another's:
  - an engine's port comes from `common::free_port()`, never from the process id;
  - a scratch folder is a `common::TestDir` (or `common::project`, which makes one): named for the
    test, the process and a counter, so no other test or worktree shares it, and removed when it
    drops. Never a path under the temp dir of your own;
  - a viewport test that renders starts its app from `common::render_app()`, which runs render apps
    one at a time;
  - no test sets an environment variable or the working folder.
- A test adds no watchdog of its own. An app from `common::render_app()`, `common::app()` or
  `base_app` carries the engine's stall watchdog (`bevy_ts_remote::stall`): a frame open for 30 s
  (a render the GPU never finishes, a script that never returns) exits the binary with code 75 and
  an `Update stopped:` line naming the test's thread. Time between updates does not count. A test
  of the watchdog itself replaces `StallSettings` with a short limit and `StallAction::Call`, never
  an environment variable, and blocks a system until its `Call` releases it.
- A type the test declares has the path `it::<module>::<Type>`: the crate is the binary, `it`.

## The rules

1. Test a behaviour once, at the lowest layer that shows it: a reconcile rule in `bevy-ts-tsx`, not
   through an engine `bevy-ts-cli` spawns. Name the layer that owns it before you add a second.
2. Never wait a fixed time. Wait for the condition, with a deadline (`common::wait_for`, an
   `until` loop). The deadline must hold on a machine at load 30: a settle check counts quiet frames
   and time, as `levels_protocol`'s does.
   A test that saves through `bevy-ts dev`'s watcher waits for the reload report before it saves:
   a report (`applied` or `failed`) means that compile's files are watched. Never sleep first.
3. Find things by what they mean: an entity by its component, `Name` or key path; a protocol answer
   by its method and fields. Never by an entity's index or spawn order.
4. A golden output holds a contract, not a migration. A protocol answer is checked against the
   protocol's schema (a `validate` helper, as the protocol and viewport tests have) and asserted
   on what matters, not compared whole.
5. A bug that depends on the engine is tested in that engine: a render bug in an app that renders,
   not under `MinimalPlugins`.
6. A bug fix comes with the test that would have caught it, at the layer it escaped.
7. A step that fails, fails: `unwrap` or assert it, never `let _ =` a result the test depends on.
8. A new bench or spike lives outside the gate's build: in `bench/`, which the workspace excludes.
   The `#[ignore]` benches already in `tests/it` (`levels_bench`, `stop_bench`, ...) build with
   every test run until they move.

A release bench runs as `cargo static test --release …`, so it links Bevy statically and times what
ships; a plain `cargo test --release` of `tests/it` does not compile (docs/build-cache.md, Bevy as a
dylib for tests).
