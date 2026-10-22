## Why

A level shows only when the game's TS calls `app.level()`. The studio is being rebuilt on Unity's frame (the bevy studio first-principles study, change E2, operator's go 2026-10-22). In that frame the editor opens a level file and shows it, as Unity opens a scene. The engine needs a way to open and close a `.bsn` from the protocol, with no TS call.

## What Changes

- **`bevy-ts/level_open`** `{ file }` opens a `.bsn` under the asset root as a level root `level:<file>`. It reads the file and every file it includes from disk, composes and reconciles before it replies. It answers `{ status: "opened", level }` with the `level_list` entry, or `{ status: "failed", reason, diagnostics }`. A failed open leaves nothing open. Opening an open level answers it again and respawns nothing.
- **`bevy-ts/level_close`** `{ file }` closes an open level: its entities despawn. It answers `{ closed }`. A closed level stays closed until `level_open` names it again, whoever opened it: a TS generation or a sidecar check does not reopen it.
- **Several levels may be open.** Opening one never closes another. The studio decides what to close.
- **`level_list` entries gain `opener`**: `ts`, `rust`, `editor` or `sidecar`.
- **`level+watch` gains `opened` and `closed`**, so a Hierarchy follows without polling. `tsx_tree+watch` sends an opened level's nodes as `added` and a closed one's as `removed`, as it does for any reconcile.
- **An opened level is a level like any other.** `tsx_tree`, `level_node`, `level_write`, `level_apply`, picking and `viewport_select` work on it unchanged, and it hot-reloads by `level-hot-reload`'s rules.
- **Opening runs nothing.** `level_open` runs no system and no `Startup` of its own. Whether the world runs is the mode's rule: in edit mode nothing of the game runs (E1, `dev --edit`).
- **Nothing unsaved lives in the engine.** `level_write` puts every batch on disk before it replies, so closing never drops an edit. The studio's unsaved dot is its own.
- **Protocol: the next free minor on main when this lands** (planned 1.18), additions only.
- **Out of scope, with seams left:**
  - E1, the `--edit` boot (team dev-edit). It owns the edit app state and which systems run.
  - E3, placeholders for missing scene components. Until then such a level fails to open with `reason: "load"`.
  - E5, the last-open levels per user. It reads `opener: "editor"` from `level_list`.
  - The studio's level picker and Hierarchy (abyme, with build@bevy-game-shell).

## Capabilities

### New Capabilities

None.

### Modified Capabilities

- `levels`: a level opens and closes over the protocol; `level_list` says who opened each level.
- `level-writing`: `level+watch` streams `opened` and `closed`.

## Impact

- `crates/bevy-ts-levels`: `engine.rs` (an `Opener` in place of `from_ts`, a closed-by-editor set that `apply_section` respects, a synchronous open), `protocol.rs` (two methods, `opener`, two events), `tests/levels.rs`.
- `protocol/bevy-ts.openrpc.json`: the next free minor, regenerated Rust and TS types. lead@bevy-ts names the version at landing; dev-edit (E1) may bump too.
- `docs/levels.md`: opening and closing a level.
- abyme: the studio's game shell (build@bevy-game-shell, then an abyme team for the studio half of E2) calls these. An engine older than this minor answers method-not-found.
- Performance: none at rest. An open reads files and reconciles once, as an `app.level()` load does.
- Needs the operator's go: nothing on the ask-first list. No new dependency, no new pattern (the methods follow `level_write`), no performance cost, nothing irreversible.
