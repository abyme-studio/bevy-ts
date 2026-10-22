# Editing a level

A level is a `.bsn` file. bevy-ts is the only thing that writes one: an editor sends
`bevy-ts/level_write` a batch of operations on name paths, and bevy-ts splices them into the
file's own text, keeping its comments and layout. The whole batch applies or none of it.

```json
{ "file": "levels/arena.bsn", "base": "<content hash>", "ops": [
  { "op": "add_instance", "parent": [], "scene": "enemy::Enemy" }
] }
```

`base` is the hash the batch was made against (`level_list`, `level+watch`). When the disk has
moved on, the world follows the disk first, then the batch replays on the new text, or is refused
as a conflict. `base: null` writes a file that does not exist yet.

## Opening and closing a level

An editor opens a level as Unity opens a scene, with no `app.level` call (protocol 1.19; an
older engine answers method-not-found):

```json
{ "method": "bevy-ts/level_open", "params": { "file": "levels/room.bsn" } }
```

It reads the file and every file it includes from disk, composes and reconciles before it
replies `{ status: "opened", level }`, with `level` as `level_list` lists it. A failure opens
nothing: `{ status: "failed", reason, diagnostics }`, where `reason` is `invalid` (not a `.bsn`
path inside the asset root), `not-found`, or `load` (a file fails to parse or resolve, each
diagnostic with its file, line and column). Opening an open level answers it again and spawns
nothing. Opening never closes another level: the editor decides what to close, with
`bevy-ts/level_close { file }`, which answers `{ closed }`. Opening runs no system; whether the
game's systems act on the new entities is the mode's rule.

An opened level is a level like any other: `tsx_tree`, `level_node`, `level_write`, picking and
hot reload treat it as an `app.level` level. Each `level_list` entry says who opened it in
`opener`: `ts` (`app.level`), `rust` (`add_level`), `editor` (`level_open`) or `sidecar`. The
opener is who opened it: a level open already keeps its opener when the editor opens it too, and
one `level_open` opens is `editor`, an `app.level` level or a sidecar it reopens included. A level `level_close` closed stays
closed for the process until `level_open` names it again, whoever opened it: neither a TS save
nor a scene's sidecar check reopens it. `level+watch` streams `opened` (with `opener`) and
`closed`, whoever opens or closes the level, and `tsx_tree+watch` sends an opened level's nodes
as `added` and a closed one's as `removed`.

Under `bevy-ts dev --edit` the engine remembers the editor's levels across restarts, in
`<project>/.bevy-ts/editor.json` (git-ignored): `{ "version": 1, "open": [...], "closed": [...] }`,
where `open` is every level with opener `editor` and `closed` every level `level_close` closed. It
writes the file on every open or close that changes either set, whole each time. Before the first
frame it reads it back: the `closed` levels stay closed as if closed in this process, and each
`open` level opens again with opener `editor`, so the first `level_list` lists it. A restored level
composes once its files load and the first generation has been tried, and a scene's sidecar layers over its scene once the generation names the scene; one that no longer
composes stays open with its error. A missing or unreadable file, or another `version`, starts
with neither set; a remembered level whose file is gone is skipped with a warning and dropped from
the next write. Plain `dev`, `play` and `build` neither read nor write the file.

## Operations

| op | takes | inverse |
|---|---|---|
| `set`, `revert` | path, component, field path (`set`: value) | each other |
| `set_prop`, `revert_prop` | path, prop, value; `field` for a scene component field | each other |
| `add_entity` | parent, optional name, patches, optional `asset` | `delete` |
| `add_instance` | parent, optional name, `scene` with props and fields, or `include` | `delete` |
| `add_component` | path, component, optional fields | `remove_component` |
| `remove_component` | path, component | `add_component` with `text` |
| `delete` | path | `restore` |
| `rename` | path, to | `rename` |
| `reparent` | path, parent, optional index | `reparent` to the old index |
| `restore` | parent, index, text | `delete` |

The reply has the new hash, the changed lines, the inverse batch (an editor's undo), and
`created`: `{ namePath, entity }` for each entity the batch added, so the editor can select it
with `viewport_select`.

- **Names.** With no name, the writer picks the first free sibling name from the scene
  component (`Enemy`), the include's or asset's file (`room`, `crate`), the first patch, or
  `Entity`, without trailing digits: `Enemy`, `Enemy2`, `Enemy3`. A given name that is taken is
  refused with `exists`.
- **Assets.** `asset: "models/crate.glb"` writes `WorldAssetRoot("models/crate.glb#Scene0")` (a
  label in the path is kept); a `.bsn` asset is included like `add_instance`'s `include`; any
  other type is `invalid`.
- **Components.** A component is named as `LevelOverride.component` names it, or by any path a
  patch resolves; a new patch is written in the shortest form that resolves. `add_component` is
  `exists` when the entity has the component, from the file or its prefab. `remove_component`
  removes the whole patch; it is `prefab-internal` when the prefab gives the component, since
  `.bsn` has no removed marker.
- **Reparent and restore** go to `index` among the parent's written children, or last. Both
  keep comments with the entity, so a move or a delete and its inverse give the file back byte
  for byte. A reparent keeps the entity and its state, as a rename does.
- **A batch does not reuse what it changed.** An op may not name, or land on, a path an earlier
  op of the same batch deleted, renamed, moved or added, or anything under it, nor a positional `#n` in a
  list an earlier op added to or took from: that batch is `invalid`; split it. A component
  removed then added on the same entity is fine, so `[remove_component Foo, add_component Foo
  { … }]` replaces a patch. The world checks (`exists`, a prefab's component) see the world before
  the batch. A file no open level shows has no world to check: its `add_component` and
  `remove_component` are `invalid` until a level that places it is open. An included file is
  checked through each level that includes it, by what comes from the file or below it, not by
  the including level's overrides.
- **What a prefab gives is the prefab's.** An entity under an instance (a prefab's or an
  included level's), and an instance's children list, are refused with `prefab-internal` for
  everything but `set` and `revert`, which write overrides. Edit the prefab, or open the included
  level and write it.

## What to add

`bevy-ts/level_catalog` lists the scene components of the running generation a level can place,
with file, line and props. Beside them an editor offers an empty entity (`add_entity` with no
base) and a dragged asset (`add_entity` with `asset`). It changes only when a TS reload applies.

Each `bevy-ts/tsx_tree` node a level placed carries `level: { file, namePath, written }`;
`written` is false for what a prefab or an included level gives, so an editor greys what it
cannot rename, delete or move. `level_node` lists `patches`, the components the file adds that
the prefab does not give. A level's nodes come in file order, each parent before its
children, so a `level_write` reparent index can be counted from the tree. Every row names its
`root` and `parent`, its nearest ancestor row. `tsx_tree+watch` keeps that order without a
re-read: `removed` lists whole subtrees, and `ordered` gives the full child order of each
sibling list that gained, lost or reordered a child, a move within the file included.

## Missing code: placeholders

A level that names a component whose code is gone, renamed, broken or not built yet still loads
under `bevy-ts dev`, as Unity loads a scene with a missing script (protocol 1.21; an older engine
fails the load). The entity spawns under its name path with everything that did resolve, and
carries one reflected component, `bevy_ts_levels::MissingComponents` (reflected as
`bevy_ts_levels::engine::MissingComponents`, the path a `bevy_remote` query names), with an entry per
placeholder. The missing type is not on the entity, so no query matches it and no system runs for
it. Each entry has a `reason`:

- `unregistered`: no live code registers the type, or the class is no longer a scene component.
- `failed`: its `scene()` threw (that instance only), or the TS build failed and its empty
  generation is live. A typo of a native name, as written (`Transfrom`), stays `unregistered`
  then: the build did not cause it.
- `mismatch`: the code is live but saved fields or props do not fit. The component applies with
  what fits; the entry lists only the misfits, each with why in `misfit`. An unset required prop
  makes the whole instance a placeholder.

Under a missing scene instance, a `Children [ … ]` entry with no base of its own (no include, no
`@Comp`) may be a merge target, so it is kept on the entry as `overrides`, not spawned; one with a
base spawns. When the code comes back, the next reconcile adds the component to the same entity,
expands it in place, merges the kept overrides by name and drops the entry.

The file is the store: the writer splices text, so save, rename, reparent, delete and its undo
keep a placeholder's text byte for byte. `level_write` refuses `set`, `revert`, `set_prop` and
`revert_prop` on a placeholder (`missing`), except a `set_prop` that sets an unset required prop,
and any write of a type no live code registers (`unknown`); `remove_component` takes a placeholder by the name the file writes, and its inverse
puts the text back. `tsx_tree` rows carry `level.missing` (`{ component, reason }`, with
`scene: true` on the entity's own scene component and none on a patch, 1.23), `level_node`
carries `missing` (`MissingComponent`: `fields` as saved, `location`, `expected` where the code
was expected, when this process saw it, and `scene` as on the row), its `patches` list a
placeholder patch by the name the file writes (1.23), and each entry is a `warning:` diagnostic on
`level_list`, `level_open`, `level+watch`'s `loaded` and the terminal.

Tolerance stops at broken files: a parse error, a duplicate name, an inclusion cycle, a missing
include and an override of a bound field still fail the level whole. A shipped game
(`dev(false)`) keeps the strict loader, so a placeholder there is the error it was;
`bevy-ts levels check` and `bevy-ts build` are unchanged.

This is the one place bevy-ts goes past Bevy: Bevy fails a whole scene on an unregistered type,
and no upstream work proposes otherwise. It lives only in bevy-ts's own `.bsn` loader, in dev
mode, and goes with that loader when bevy-ts adopts Bevy's.

## Scenes without a level

An `app.scene` root has a sidecar level, `levels/<scene>.bsn` or its `level` option, named in its
tree root's `sidecar` whether or not the file exists. A `level_write` to that path creates the
file on first write and layers it over the scene by key path. Entities added there are the
scene's own top-level entities and fully editable; the scene's TSX entities take overrides but
stay TSX's to rename, delete and move (`prefab-internal`). A sidecar path `app.level` declares
is a level of its own instead, on every mount and when `level_open` reopens it: it does not layer
over its scene.

## Not here

No built-in primitives and no duplicate. Bevy's own editor (bevy_editor_prototypes, its roadmap,
BSN) has neither: it spawns an empty entity, drops a glTF as a `WorldAssetRoot`, and adds and removes
components. A game that wants a cube ships it as its own scene component, which the catalog then
lists.
