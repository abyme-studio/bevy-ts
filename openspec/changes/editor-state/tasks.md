## 1. Engine

- [x] 1.1 `Levels.state_file: Option<PathBuf>` (default `None`) and a serde type for `{ version, open, closed }` in `crates/bevy-ts-levels/src/engine.rs`
- [x] 1.2 A `Startup` restore before `open_pending`: read the file (missing, malformed or unknown version: warn, empty), load `closed`, prune `closed` paths that are not files, open each `open` path as `editor` (deferred, as `open_pending`; `open_now` cannot compose before the first generation), warn and skip paths that are not files; `reconcile_dirty` waits for the first live generation
- [x] 1.3 A `Last` writer: build the remembered set (opener `editor` roots, `closed`), and when it differs from the last written, write `editor.json.tmp` and rename it; create `.bevy-ts/` if missing; a write error warns
- [x] 1.4 Both systems do nothing when `state_file` is `None`

## 2. CLI

- [x] 2.1 `run_dev` in `crates/bevy-ts-cli/src/cli.rs`: under `--edit`, set `state_file` to `<compiler.root>/.bevy-ts/editor.json`

## 3. Tests and docs

- [x] 3.1 Tests in `crates/bevy-ts-levels/tests/levels.rs`, one per scenario: reopening the room, a closed game level stays closed across a generation, a deleted remembered level is skipped and pruned, a malformed and an unknown-version file, no `state_file` reads and writes nothing
- [x] 3.2 A test that the write is whole: the file parses after each open and close, and no `.tmp` is left
- [x] 3.3 `docs/levels.md`: under `dev --edit` the engine remembers opened and closed levels in `.bevy-ts/editor.json` and reopens them; plain `dev` does not
