## Context

Levels open today in three ways (`crates/bevy-ts-levels/src/engine.rs`): `app.level` through the TS `levels` section (`apply_section`, `from_ts: true`), `add_level` from Rust at `Startup` (`open_pending`), and a scene's sidecar (`open_sidecar`). `open_as` makes a `LevelRoot` with an asset handle; `watch` composes and reconciles it once the asset is loaded. `close` unmounts the root. `level_write` already reads a file from disk, replaces the asset and reconciles before it replies (`edit.rs`, `load_text` then `reconcile_level`).

The studio is rebuilt on Unity's frame (study change E2, operator's go 2026-10-22). Unity and Godot are the reference there. Both open a scene file in the editor without running the game, and Unity can hold several scenes open at once.

Agreed with:
- spec@dev-edit (E1): E1 owns the `--edit` boot and which systems run. E2 owns opening and closing a level in either mode.
- build@bevy-game-shell: the method shape. The unsaved dot stays the studio's (abyme's editor's dirty text for the file); nothing about it goes in the protocol. No "active level" in the engine.

## Goals / Non-Goals

**Goals:** open and close a `.bsn` from the protocol, synchronously, with the same reconcile, writing and reload rules as any level; say who opened each level; stream opens and closes.

**Non-Goals:** the edit-mode boot (E1); placeholders for missing scene components (E3); remembering last-open levels (E5); an active level; a "new level" method (`level_write` with `base: null` already creates a file, which `level_open` then opens).

## Decisions

- **One `Opener` replaces `from_ts`.** `LevelRoot.opener` is `Ts`, `Rust`, `Editor` or `Sidecar`. `apply_section` closes only `Ts` levels a generation drops, as `from_ts` does now. `level_list` serializes it. Alternative: a second `from_editor` flag. Rejected: two booleans for one fact.
- **Opening an open level keeps its opener.** An `app.level` level the studio also opens stays `ts`, so a generation that drops it still closes it. Alternative: promote it to `editor`. Rejected: then the TS code and the editor disagree on who owns it, and E5 would remember a level the game opens anyway.
- **The editor's close is kept in a `closed` set.** `level_close` adds the path; `apply_section` and `open_sidecars` skip paths in it; `level_open` removes it. Without it the next TS save would reopen a level the studio closed. The set lives for the process. Alternative: refuse to close `ts` levels. Rejected: in `dev --edit` the game's own `app.level` levels load (E1), and the studio must be able to show only the one it opened.
- **Open is synchronous.** `level_open` reads the root file and its includes from disk depth-first, inserts each as a `BsnLevel` asset as `load_text` does, then composes and reconciles before it replies, as `level_write` does. The reply can then carry the entry, or the failure, with no second round trip. A cycle or a missing include is a `load` failure with the include's location.
- **A failed open opens nothing.** The root is created only after compose succeeds. An already-open level whose file goes bad keeps `level-hot-reload`'s rule (stays as it was, `failed` on `level+watch`); that is a reload, not an open.
- **Opening runs nothing.** The methods add no system and no schedule run. In edit mode the world is paused (E1's state), so the reconcile's spawns are the only writes. In play mode an opened level joins the running game, as an `app.level` level does.
- **Paths are asset paths.** Relative to the asset root, `/`-separated, ending in `.bsn`, with no `..` that leaves the root: the same rule `level_write` resolves `file` with.
- **Events.** `Levels::emit` sends `opened` (with `opener`) from `open_as` and `closed` from `close`, so every way of opening reports, not just the protocol's.

## Risks / Trade-offs

- [A level open twice, once on its own and once included by another, shows twice] → That is what the user asked for: two roots. The Hierarchy shows both, each under its own root. Same as `app.level` today.
- [A level using a scene component the current generation lacks fails to open] → Reported as `load` with the location. E3 turns that into a placeholder.
- [A synchronous open holds the frame on a large level] → It costs what an `app.level` load costs: 37 ms at 1,000 instances, 271 ms at 10,000 (`docs/research/levels-scale.md`). It happens once, on the operator's click, in edit mode.
- [E1 bumps the protocol too] → This takes the next free minor on main at landing (planned 1.18), rebases and regenerates types. lead@bevy-ts names the number.
