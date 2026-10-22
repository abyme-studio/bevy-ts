## 1. Engine

- [x] 1.1 `Opener` (`ts`, `rust`, `editor`, `sidecar`) in place of `LevelRoot.from_ts`; `apply_section` closes only `ts` levels a generation drops
- [x] 1.2 The editor's `closed` set: `apply_section` skips its paths; `level_open` clears the path
- [x] 1.3 A synchronous open: read the file and its includes from disk, insert the assets, compose, and create and reconcile the root only on success; return the diagnostics otherwise
- [x] 1.4 `opened` (with `opener`) and `closed` events from `open_as` and `close`

## 2. Protocol

- [x] 2.1 `bevy-ts/level_open` and `bevy-ts/level_close` in `protocol.rs`, with path checks as `level_write`'s
- [x] 2.2 Contract: the two methods, `LevelOpenParams`/`LevelOpenResult`, `LevelCloseParams`/`LevelCloseResult`, `opener` on `LevelInfo`, `opened` and `closed` in `LevelWatchMessage`; the next free minor on main at landing; regenerated Rust and TS types; CI diff check

## 3. Tests and docs

- [x] 3.1 Tests in `crates/bevy-ts-levels/tests/levels.rs`, one per scenario: opening the arena (paused, no `app.level`), opening twice, a typo, a missing file, switching levels, closing a TS-declared level across a generation, closing an `add_level` level, the open and close in `tsx_tree+watch`, writing to an opened level, and `level+watch`'s `opened`, `loaded`, `closed`
- [x] 3.2 `docs/levels.md`: opening and closing a level, `opener`, and that the studio needs this minor
