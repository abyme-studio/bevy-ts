## Why

The operator ruled (2026-10-19) that TSX defines prefabs, what a thing is, and that levels, where things are, and per-instance overrides live in `.bsn` text files the viewport writes, as Godot writes `.tscn` and Unity its scenes. `tsx` reconciles TSX scenes and `viewport` routes edits by provenance, but no `bsn` route exists: Bevy 0.20 ships no `.bsn` loader (upstream merged only a file parser, `bevy_bsn`, on 2026-10-15, with no loader or writer). Until a level file exists, a drag in the viewport persists only when the editor finds a literal in TSX. This change gives bevy-ts its own `.bsn` loader and writer, kept as close to upstream BSN as possible so that adopting Bevy's loader means deleting ours.

## What Changes

- A `.bsn` reader for upstream's file grammar (the `bevy_bsn` subset from Bevy PR #26043) plus four named extensions: comments, prefab instances `@game::Enemy { … }`, quoted names `#"…"`, and child overrides that merge by name. A differential test against vendored `bevy_bsn` keeps the extension-free part identical to upstream.
- Levels: `app.level("levels/arena.bsn")` loads a file as a reconciled root. Each top-level entity is a named entity. It can instance a TSX prefab by registered name with props, layer component overrides after it, override the prefab's keyed children by name, and include another level (`:"levels/room.bsn"`).
- TSX prefabs gain an optional registered form, `prefab("game::Enemy", props, fn)`, with a typed props schema so a text file can call them. Plain-function prefabs keep working inside TSX.
- Identity: an entity's id is the level path plus its `#Name` path. Names are unique among siblings. The writer names an unnamed entity the first time it writes to it. Renames go through a writer operation, so the entity is kept.
- A lossless, span-keeping parse and a writer that edits by splicing text. A one-field edit changes one line. Comments, order and hand formatting are kept, and new entries copy the file's indentation.
- `.bsn` edits arrive as `viewport` `bsn` routes and as `bevy-ts/level_write` operations from the protocol: set field, revert override, set prop, add instance, add entity, delete, rename and reparent.
- Hot reload: a `.bsn` file saved by anyone applies to the running world under `tsx`'s state rules. Override values are controlled, simulation state survives, a bad file is rejected whole, and tombstones hold.
- Conflicts: the disk wins. Each write carries the content hash it was based on. On a mismatch bevy-ts reloads the disk version, then replays the edit by name path or rejects it with `conflict`. It never does a text merge.
- Assets: string values in `Handle` fields are asset paths, as upstream defines them. `./` and `../` resolve against the level file. A missing asset is a warning with file, line and column, and the level still loads.
- Protocol 1.2 (additions only): `bevy-ts/level_list`, `level_node`, `level_write`, `level_apply` and `level+watch`. tsx provenance gains `override`.
- Open with the operator: whether a viewport edit inside a prefab instance ever rewrites TSX, or only an explicit "Apply to prefab" does. Each edit carries both targets, and a project setting picks one. Both are specced.
- Out of scope: saving play-mode state into a level, `.bsn`-defined prefabs as a separate concept from levels, Rust expressions or closures in files (`{expr}`, `on(…)`, `~` templates), list and map values, asset GUIDs, and a binary format.

## Capabilities

### New Capabilities
- `bsn-format`: the `.bsn` subset bevy-ts reads and writes, the named extensions, value and asset-path rules, conformance with upstream, and the switch to Bevy's loader.
- `levels`: loading a level as a reconciled root, prefab instances by name with props and overrides, nested prefabs and included levels, entity identity, provenance and protocol reads.
- `level-writing`: the writer's operations, minimal formatting-preserving diffs, the `bsn` edit route and the prefab-edit policy, and conflicts with the file on disk.
- `level-hot-reload`: what a `.bsn` change, a TS reload and a failure each do to level entities.

### Modified Capabilities

None. `tsx` and `viewport` are not archived. What this change needs from them is written as requirements in design.md, under "Depends on tsx and viewport".

## Impact

- New crate `bevy-ts-levels`, depending on `bevy-ts`, `bevy-ts-tsx` and `bevy-ts-remote`. It holds the vendored `bevy_bsn` syntax module (MIT/Apache-2.0), the lossless parser, the `BsnLevel` asset loader, level roots, the writer and the protocol methods.
- npm package: `prefab()` and `app.level()` in `bevy-ts`.
- `protocol/bevy-ts.openrpc.json`: `bevy-ts/level_*` methods. This is a minor bump to 1.2, after viewport's 1.1.
- Needs additions to `tsx`: registered prefabs, external roots, `override` provenance and pinned bound fields. It also changes `viewport`'s route payload: the owner chain and the prefab-edit policy. Both are listed in design.md.
- New dependency: none beyond Bevy's own. The parser is hand-written, as upstream's is, and `nom` comes in only through the vendored differential test module.
