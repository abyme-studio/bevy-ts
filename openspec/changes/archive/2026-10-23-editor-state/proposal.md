## Why

`bevy-ts dev --edit` starts with no level the editor opened. Each time the studio starts the engine, the user opens their levels again. Unity and Godot reopen the last open scenes from per-user state the VCS ignores. This is E5 of the bevy studio first-principles study, on top of `dev --edit` (E1) and `level_open` (E2).

## What Changes

- **`dev --edit` remembers the editor's levels** in `<project>/.bevy-ts/editor.json`: `{ "version": 1, "open": [...], "closed": [...] }`. `open` is every level with opener `editor`. `closed` is every level `level_close` closed.
- **It reopens them on boot.** Before the first frame it loads `closed`, then opens each `open` path with opener `editor`; each composes once the first generation is live. They come back with opener `editor`. The studio's first `level_list` already lists them.
- **It writes on every change** to the remembered set, to a temporary file then a rename. A crash leaves the old file or the new one, never half of one.
- **Nothing fails the boot.** A missing, malformed or unknown-version file: warn, start empty. A remembered level whose file is gone: skip it, warn, and drop it from the next write. One that no longer composes stays open with its error.
- **Only `dev --edit`.** Plain `dev`, `play` and `build` neither read nor write the file.
- **No protocol change.** `level_list` and `level+watch` already say what is open.
- **Out of scope:** tab order and the focused tab (the studio's). Locking between two editors on one project: the last write wins.

## Capabilities

### New Capabilities

None.

### Modified Capabilities

- `levels`: `dev --edit` remembers and reopens the levels the editor opened and closed.

## Impact

- `crates/bevy-ts-levels`: `engine.rs` (`Levels.state_file: Option<PathBuf>`, a `Startup` restore before `open_pending`, a `Last` writer), `tests/levels.rs`.
- `crates/bevy-ts-cli/src/cli.rs`: `run_dev` sets `state_file` to `<project>/.bevy-ts/editor.json` under `--edit` only.
- `docs/levels.md`: one paragraph by `level_open`/`level_close`.
- `.bevy-ts/` is already git-ignored in new projects (`bevy-ts new`) and in this repo. Nothing new is committed by users.
- abyme's studio: nothing to change. It reads `level_list` on start as today.
- Performance: none at rest. One small file write per open or close.
- Needs the operator's go: nothing on the ask-first list. No new dependency (`serde_json` is already in `bevy-ts-levels`), no protocol change, no new pattern (`.bevy-ts/` is already the engine's per-project state folder). The file is the user's own editor state, local and ignored.
