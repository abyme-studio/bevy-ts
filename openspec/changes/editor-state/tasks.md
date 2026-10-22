## 1. Engine

- [ ] 1.1 `Levels.state_file: Option<PathBuf>` (default `None`) and a serde type for `{ version, open, closed }` in `crates/bevy-ts-levels/src/engine.rs`
- [ ] 1.2 A `Startup` restore before `open_pending`: read the file (missing, malformed or unknown version: warn, empty), load `closed`, prune `closed` paths that are not files, `open_now` each `open` path, warn and skip failures
- [ ] 1.3 A `Last` writer: build the remembered set (opener `editor` roots, `closed`), and when it differs from the last written, write `editor.json.tmp` and rename it; create `.bevy-ts/` if missing; a write error warns
- [ ] 1.4 Both systems do nothing when `state_file` is `None`

## 2. CLI

- [ ] 2.1 `run_dev` in `crates/bevy-ts-cli/src/cli.rs`: under `--edit`, set `state_file` to `<compiler.root>/.bevy-ts/editor.json`

## 3. Tests and docs

- [ ] 3.1 Tests in `crates/bevy-ts-levels/tests/levels.rs`, one per scenario: reopening the room, a closed game level stays closed across a generation, a deleted remembered level is skipped and pruned, a malformed and an unknown-version file, no `state_file` reads and writes nothing
- [ ] 3.2 A test that the write is whole: the file parses after each open and close, and no `.tmp` is left
- [ ] 3.3 `docs/levels.md`: under `dev --edit` the engine remembers opened and closed levels in `.bevy-ts/editor.json` and reopens them; plain `dev` does not
