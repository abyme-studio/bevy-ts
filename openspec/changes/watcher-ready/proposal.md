## Why

Tests slept 100 ms before saving through `bevy-ts dev`'s watcher (`packages.rs`, `reload.rs`), hoping it was armed, and a levels test ran a fixed 20 frames to drain late errors. Under load those guesses fail.

Two causes in the watcher (`crates/bevy-ts-cli/src/dev.rs`), found with a probe of notify 8.2.1 on macOS:

- A reload was reported before the folders its new files sit in were watched. A save in that gap was lost.
- FSEvents reports writes made just before a watch starts: 88 of 100 probe runs got an event for a write from before `watch()`. The watcher took it for a save and compiled again, so a test saw one generation more than it saved. No write after `watch()` returned was missed (0 of 100, also after a second `watch()` restarts the stream).

Bevy's own asset watcher is armed while the app builds (`AssetPlugin`), and the levels tests reload by hand, so neither needs a signal.

## What Changes

- The watcher watches a compile's folders before it sends its report. A reload's report (`Applied` or `Failed` in `ReloadLog`, `bevy-ts/reload+watch`) is the signal that its files are watched.
- The watcher drops an event whose file still has the text it compiled (`Compiler::hashes`, a hash of each source as it read it), or whose missing path has not appeared or moved its nearest folder.
- When a folder is watched only after its files were read, the watcher checks them once and compiles again if one changed in that gap.
- Tests: no sleep before a save; `levels.rs` waits for the fixed file's text, not 20 frames. Unit tests of the stale check in `dev.rs`.
- `docs/testing.md` says how a test waits for the watcher. `scripts/levels-watch-soak.sh` takes other tests and a prebuilt binary; `scripts/watcher-ready-soak.sh` runs main's watcher and this one under load.

## Capabilities

### Modified Capabilities

- `hot-reload`: a reload report means its files are watched, and a late report of an older write reloads nothing.

## Impact

- `crates/bevy-ts-cli/src/dev.rs`, `crates/bevy-ts-cli/src/compile.rs` (`hashes`, `content_hash`).
- Tests in `crates/bevy-ts-cli/tests/it/{packages,reload}.rs`, `crates/bevy-ts-levels/tests/it/levels.rs`.
- No protocol change. Each compile hashes the sources it reads, and each `package.json` once more.
