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

## Scenes without a level

An `app.scene` root has a sidecar level, `levels/<scene>.bsn` or its `level` option, named in its
tree root's `sidecar` whether or not the file exists. A `level_write` to that path creates the
file on first write and layers it over the scene by key path. Entities added there are the
scene's own top-level entities and fully editable; the scene's TSX entities take overrides but
stay TSX's to rename, delete and move (`prefab-internal`).

## Not here

No built-in primitives and no duplicate. Bevy's own editor (bevy_editor_prototypes, its roadmap,
BSN) has neither: it spawns an empty entity, drops a glTF as a `WorldAssetRoot`, and adds and removes
components. A game that wants a cube ships it as its own scene component, which the catalog then
lists.
