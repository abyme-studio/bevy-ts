## Why

`levels_protocol::watch_deltas_keep_a_client_in_tsx_tree_order` failed 3 of 40 copies run 8 at once (`scripts/levels-watch-soak.sh`): the delete step's watch batch re-added the renamed `A2` subtree (seq 7) before removing it (seq 8). The engine, not the test, was wrong: for one frame the world went back to an older version of the file.

The cause: the asset watcher reloads every file the writer writes. A reload that read the file before the writer's next write lands after it, with the older text. Bevy sends that reload's `LoadedWithDependencies` in PreUpdate (`bevy_asset` `server/mod.rs:209`) and its `Modified` only in PostUpdate (`lib.rs:679-682`, `Assets::asset_events`). The levels `watch` system checked the disk against the asset only on `Modified`, so on `LoadedWithDependencies` it reconciled the older text. On the next frame the `Modified` check loaded the disk's text, and the frame after reconciled it back. An editor watching saw the entity vanish and come back.

## What Changes

- `watch` (`crates/bevy-ts-levels/src/engine.rs`) checks the disk on `LoadedWithDependencies` as on `Modified`: a reload whose text is not the disk's loads the disk's text and reconciles nothing.
- A test in `bevy-ts-levels` replays that event order with no timing: write, put the older text back as the reload does, send `LoadedWithDependencies`; no frame shows the old text.
- `scripts/levels-watch-soak.sh` and its `.abyme/processes.ts` entry: the cli test, many copies at once, for proving a flake gone under load.

## Capabilities

### Modified Capabilities

- `level-hot-reload`: a reload older than the disk changes nothing in the world.

## Impact

- `crates/bevy-ts-levels/src/engine.rs` (`watch`), `crates/bevy-ts-levels/tests/it/levels.rs` (one test).
- `scripts/levels-watch-soak.sh`, `.abyme/processes.ts`.
- No protocol change. `level+watch` stops sending the `external` the bounce back to the disk's text sent.
