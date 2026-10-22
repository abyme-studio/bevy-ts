## Context

The open set lives in the engine: `Levels` in `crates/bevy-ts-levels/src/engine.rs` holds `roots` (each with an `Opener`), `declared` (the generation's `app.level` paths) and `closed` (what `level_close` closed). `open_now` serves `level_open`: a path is `ts` if declared, `sidecar` if a scene's sidecar, else `editor`. Any open removes the path from `closed`. `open_pending` is a `Startup` system. `run_dev` in `crates/bevy-ts-cli/src/cli.rs` sets edit mode under `--edit` and knows the project root; `bevy-ts-levels` knows only `asset_root`.

`.bevy-ts/` is the engine's per-project state folder (engine builds, `types/native.d.ts`). `bevy-ts new` git-ignores it, and so does this repo's root `.gitignore`.

The studio opens no level on start, and its Open level closes every other `ts`, `rust` and `editor` level (build@studio-edit-and-levels).

## Goals / Non-Goals

**Goals:** a restarted `dev --edit` engine shows the levels the user left: the ones they opened, minus the ones they closed. No protocol change.

**Non-Goals:** tab order, the focused tab, a lock between two editors, any effect outside `dev --edit`.

## Decisions

- **The engine owns it** (agreed with lead@abyme). It already holds the set, and the studio would otherwise need a new method to replay it. The studio reads `level_list` on start, as today.
- **Persist `closed` as well as `open`.** With `open` alone, a restart reopens the game's `app.level` levels the user closed, so the studio shows a different set than it left. `closed` already means "stay closed for the process", and restoring it extends that to the next process under `--edit`.
- **`open` holds only opener `editor`.** Code reopens `ts` and `rust` levels, and the scene reopens `sidecar` ones.
- **No new opener value.** Restore opens each path with opener `editor`. The studio's close-others relies on `editor`.
- **Restore opens as `add_level` does, not through `open_now`.** Under `dev` the first TS generation lands after `Startup`, from the compile thread. Before it the compose schema knows no component, so `open_now` at `Startup` would fail every remembered level and prune it. Restore inserts the root and lets `watch` compose it once its files load. `reconcile_dirty` waits until the first generation has been tried, applied or failed, which also stops an `add_level` level that uses a TS component from logging a spurious failure at boot. A first compile that fails ends the wait, so native-only levels still compose.
- **The path comes from the CLI.** `Levels.state_file: Option<PathBuf>`, `None` by default; `run_dev` sets it to `<project>/.bevy-ts/editor.json` only under `--edit`. `None` turns off both restore and write, so plain `dev`, `play`, `build` and tests are untouched.
- **Restore is a `Startup` system ordered before `open_pending`.** It loads `closed` first, so `open_pending` and the first `apply_section` skip those paths, then opens each `open` path. Remote requests are served in `Update`, so the studio's first `level_list` sees the restored levels.
- **Write on change, in `Last`.** The system compares the remembered set (sorted `open`, sorted `closed`) with the last one written and writes only on a difference: `editor.json.tmp`, then `rename`. Not on exit: the studio kills the engine, and a crash skips exit hooks. One write per click is cheap.
- **Fail soft.** Missing, malformed or unknown-version file: warn on stderr, start empty. A level whose file is gone: skip, warn, and leave it out, so the next write prunes it. One that no longer composes stays open with its error, which `level_list` shows the studio. A `closed` path that is not a file is pruned too. A write error warns and does not stop the engine.
- **Two editors: last write wins.** The rename keeps the file whole. Unity and Godot do the same.

## Risks / Trade-offs

- [A remembered `editor` level that code now declares opens as `editor` before `declared` is known, and keeps that opener, since an open level keeps its opener] → Harmless: the studio treats `ts` and `editor` alike for close-others. It becomes `ts` once closed and reopened. Noted, not fixed.
- [A restored level hides a `load` failure from the studio, which sees only stderr] → Same as an `app.level` that fails today. A `level+watch` `failed` event could carry it later with no new method.
- [Two editors overwrite each other's set] → Accepted; rare, and the file stays valid.
- [Restoring `closed` keeps a game level closed on the first Play after restart] → That is what a closed level already does in-process (the `level_close` rule).

## Migration Plan

None. An engine without this change ignores the file; one with it and no file starts empty.
