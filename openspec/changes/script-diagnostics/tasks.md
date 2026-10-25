All cargo runs through gate-slot. New tests are modules of their crate's one test binary
(`crates/<crate>/tests/it/<name>.rs`), per docs/testing.md.

## 1. Strings across exports

- [x] 1.1 `crates/bevy-ts/src/host.rs`: `Kind::Str`; `HostValue` for `String` (so `Option<String>`), `&str` and `Option<&str>` through the derive; a string is its UTF-8 length then its bytes in the slots (design decision 3)
- [x] 1.2 `glue`: `H.str` encodes a string argument into the slots, `H.text` decodes a result; `declaration` types them `string` / `string | undefined`
- [x] 1.3 `crates/bevy-ts-native-derive`: accept `&str` arguments and `String` results; `HostRecord` with a `String` field fails to compile naming the field (a `compile_fail` doc test)
- [x] 1.4 Tests in `crates/bevy-ts/src/host.rs` unit tests: round trip of ASCII, non-ASCII and empty strings, `Option` both ways, a string longer than the scratch
- [x] 1.5 `docs/native.md`: strings in the list of argument and result types

## 2. performance.now

- [x] 2.1 `crates/bevy-ts-v8`: host functions reading a `std::time::Instant` taken once per process; `performance.now` and `performance.timeOrigin` in `js/core.js`
- [x] 2.2 `packages/bevy-ts` declarations: `performance` with `now()` and `timeOrigin` only
- [x] 2.3 Test in `crates/bevy-ts-v8/tests/it`: monotonic, sub-millisecond, not reset by a reload

## 3. bevy-ts/diagnostics

- [ ] 3.1 `crates/bevy-ts-cli/src/diagnostics.rs`: `DiagnosticsStore`, `Diagnostics` (wrapping Bevy's `Diagnostics` param) and `FrameCount` exports; module JS with `Diagnostic` and the path constants; d.ts with the `App` augmentation for `registerDiagnostic`; registered in every engine as `Cameras` is
- [ ] 3.2 `registerDiagnostic`: manifest section `diagnostics`; on load add, keep, replace or disable script-owned paths; fail the load naming a path a Rust plugin owns
- [ ] 3.3 `bevy-ts.json` `diagnostics`: parse, refuse unknown names listing the accepted ones; `base_app` adds each listed plugin unless added; render plugins only when rendering, none under `bevy-ts types`
- [ ] 3.4 `RenderAssetDiagnosticPlugin<T>`: the table of render asset types the engine prepares, named as Bevy prints them
- [ ] 3.5 `SystemInformationDiagnosticsPlugin`: with the operator's go, enable `sysinfo_plugin` on the engine's `bevy`; without it, refuse the name saying the engine was built without `sysinfo_plugin`
- [ ] 3.6 `crates/bevy-ts-cli/src/build.rs`: write `diagnostics` into `game.json`; playing adds them
- [ ] 3.7 Test module `crates/bevy-ts-cli/tests/it/diagnostics.rs`, a headless engine on `common::free_port()`: frame time read from TS equals the Rust read; unknown path reads `undefined`; `FrameCount` 0 in `Startup` and 2 on the third update; sorted paths; a script diagnostic registered, measured, read from TS and Rust; reload keeps history, changed settings reset it, dropped path disabled; registering a Rust-owned path fails the load; measuring an unregistered path records nothing; off by default; a misspelt plugin exits 1 naming it; built and played keeps the list. Waits are on conditions with deadlines, never fixed times

## 4. Stage timing, bench, docs

- [ ] 4.1 `crates/bevy-ts/src/systems.rs` `run_stage`: when `ScriptStageDiagnosticsPlugin` is present, time the stage and add `bevy_ts/<stage>/elapsed_cpu`; no clock read otherwise
- [ ] 4.2 Test in `diagnostics.rs`: a 2 ms busy `Update` system gives an average of at least 2 ms; a stage with no TS systems records nothing
- [ ] 4.3 `bench/diagnostics`: frame and main-world time at 10k and 100k entities with no plugin and with each plugin alone, 3 runs per arm; numbers into design.md's cost table and the report to lead@bevy-ts
- [ ] 4.4 `docs/diagnostics.md`: the coverage table (every Bevy 0.20 diagnostic source, what TS reaches, its test; draw, batch and visible-entity counts as "Bevy lacks it"), the switch, the costs, headless pacing, an example of a budget reading counts and `elapsed_cpu`
- [ ] 4.5 `docs/research/bevy-coverage.md`: the `DiagnosticsStore`/`FrameCount` row moves to covered, naming `docs/diagnostics.md` and the tests; plan item 11 marked done
- [ ] 4.6 `bevy-ts types` output includes the module's declarations; regenerate checked-in types if any change
