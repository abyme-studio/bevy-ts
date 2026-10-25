Every cargo job runs through `~/dev/abyme/scripts/gate-slot`.

## 1. Cause

- [x] 1.1 Find where watching arms: `dev.rs` (notify, the script graph), `supervise.rs` (native crate, not under test), Bevy's `file_watcher` (armed at `AssetPlugin` build; the levels tests reload by hand)
- [x] 1.2 Probe notify on macOS (`/tmp/watchprobe`): 0 of 100 writes after `watch()` missed; 88 of 100 runs got a stale event for a write before it

## 2. Fix

- [x] 2.1 `dev.rs` watches before it reports, for every compile, and compiles again when a newly watched folder's file changed in the gap
- [x] 2.2 `Compiler::hashes`; `dev.rs` drops events whose file or missing path did not change
- [x] 2.3 Unit tests `dev::tests`: a late report is no change; an install is

## 3. Tests wait on the report

- [x] 3.1 `packages.rs`, `reload.rs`: no sleep before a save
- [x] 3.2 `levels.rs` `a_bad_file_changes_nothing_and_names_its_place`: `save_good` waits for the fixed text, not 20 frames
- [x] 3.3 `docs/testing.md`: how a test waits for the watcher
- [ ] 3.4 Under load (`watcher-ready-soak`, 8 copies at once, 5 rounds): main's watcher without the sleeps, then this one
