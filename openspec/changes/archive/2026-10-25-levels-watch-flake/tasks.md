Every cargo job runs through `~/dev/abyme/scripts/gate-slot`.

## 1. Cause

- [x] 1.1 Reproduce under load: `levels-watch-soak` (8 copies at once, 5 rounds) fails 3 of 40, all at `levels_protocol.rs:518`, the delete's batch holding seq 7 (the `A2` subtree added) and seq 8 (removed)
- [x] 1.2 Find why: a stale reload's `LoadedWithDependencies` (PreUpdate) reaches `watch` before its `Modified` (PostUpdate), and only `Modified` was checked against the disk

## 2. Fix

- [x] 2.1 `watch` checks the disk on both events
- [x] 2.2 `bevy-ts-levels` test `writes::a_reload_older_than_a_write_changes_nothing`: fails without 2.1, passes with it
- [x] 2.3 The soak after the fix: 0 of 160 (16 copies at once, 10 rounds). Weak alone: soaks with debug output and no fix also passed 0 of 320, so the race is rare outside the first run's load; 2.2 is the proof
