## 1. The store

- [x] 1.1 `crates/bevy-ts/src/session.rs` (new): `SessionBytes` resource (`HashMap<String, Box<[u8]>>`, running byte total), `KEY_MAX_BYTES = 1024`, `STORE_MAX_BYTES = 64 << 20`; `put` checks both limits before changing anything and errors name the key (cut to 64 chars) and the sizes; `get`, `size`, `delete`, `clear`, `count`, `bytes`; unit tests `put_replaces_counts_without_old`, `limits_leave_store_unchanged`
- [x] 1.2 `crates/bevy-ts/src/lib.rs`: `pub mod session`, `pub use session::SessionBytes`, `.init_resource::<session::SessionBytes>()` beside `SystemMemory`
- [x] 1.3 `crates/bevy-ts/src/generation.rs`: in the restart arm of `apply_set`, insert `SessionBytes::default()` beside `SystemMemory::default()`; the heap-restart arm leaves it

## 2. bevy-ts/session

- [x] 2.1 `crates/bevy-ts-cli/src/session.rs` (new): `ScriptSessionPlugin`, the `SessionParam<'w>` (`ResMut<SessionBytes>`) exported as `SessionBytes` in `bevy-ts/session` with `put`, `size`, `read`, `has`, `delete`, `clear`, `count`, `bytes` per design decision 3; `ts_module`
- [x] 2.2 `crates/bevy-ts-cli/ts/session.js` (new): `get(session, key)` over `size` + `read`
- [x] 2.3 `crates/bevy-ts-cli/ts/session.d.ts` (new): `get`, doc comments for the limits and lifetime
- [x] 2.4 `crates/bevy-ts-cli/src/lib.rs` `pub mod session`; add the plugin in `cli.rs` and `types.rs` where `ScriptDiagnosticsPlugin` is added; `bevy-ts types` writes the param into `native.d.ts`

## 3. Tests (`crates/bevy-ts-cli/tests/it/session.rs`, new, `mod session;` in `tests/it/main.rs`)

- [x] 3.1 `put_reload_get`: `Startup` puts 64 KB of a known pattern under `"heimdall/memory"`; the project's `game.ts` is rewritten with a changed `Update` that logs `get`'s length and checksum; `reload(&mut app, false)`; the log shows the same length and checksum, and a module-level counter in the same file shows 0 again
- [x] 3.2 `survives_a_failed_reload`: put, save a syntax error, reload (failed), save a good file, reload; the value is there
- [x] 3.3 `restart_and_stop_empty_it`: put, `reload(&mut app, true)`, `count()` is 0; put again, `bevy_ts::generation::rebuild`, `count()` is 0
- [x] 3.4 `rust_and_ts_share_it`: a Rust system puts `[1, 2, 3]` under `"rust/seed"`, a TS system `get`s it; a TS put is read back from `world.resource::<SessionBytes>()`
- [x] 3.5 `methods`: `read` into a short buffer returns the length and copies nothing; `get`'s array is a copy; `has`, `delete`'s result, `count`, `bytes`, `clear`
- [x] 3.6 `limits_and_types`: an empty key and a 1,025-byte key throw; a put over 64 MiB throws naming the key and leaves `count()`/`bytes()` unchanged; a `Float32Array` value throws naming `SessionBytes.put`
- [x] 3.7 `two_modules_share_a_key`: `ui.ts` puts, `camera.ts` reads, same frame

## 4. Bench

- [x] 4.1 `bench/session-bytes/` (new): `project/game.ts` with the arms in design's measure plan, `bench.mjs`, `summary.mjs`, `README.md`
- [x] 4.2 `.abyme/processes.ts`: `session-bytes-bench`, release build through `~/dev/abyme/scripts/gate-slot`, then `bench.mjs`
- [x] 4.3 Run it; commit `results/after.jsonl` and the summary in the README; a miss of the pass line goes to the lead

## 5. Docs

- [x] 5.1 `docs/scripts.md`: "Session bytes" (written with the proposal; check it against what was built)

## 6. Gate

- [x] 6.1 `openspec validate session-bytes --strict`
- [ ] 6.2 The repository's check for the batch, scoped to what changed, through gate-slot
