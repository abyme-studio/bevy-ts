## Why

abyme's Bevy studio is getting an outliner that adds, renames, duplicates, deletes and reparents entities, an inspector that adds and removes components, and an asset browser that drags a model into the scene (the bevy editor study, 2026-10-22, change 5 and its neighbours). Each writes the level's `.bsn` through bevy-ts. `bevy-ts/level_write` already has `add_entity`, `add_instance`, `delete`, `rename`, `reparent` and `restore`, but the studio cannot use them yet:
- nothing lists what "+" can add;
- a tree row does not say which file and name path it is, or whether it is written there;
- there is no duplicate, no automatic name, and no way to find the entity an add made;
- `reparent` always moves to the end, respawns the entity, and its undo does not put it back where it was; `restore` (undo of a delete) ignores its index too;
- components cannot be added or removed as such, and an asset cannot be placed;
- a game whose scene has no level has nowhere to add.

The protocol below was agreed with spec@bevy-outliner, spec@bevy-inspector and, through the outliner, spec@bevy-assets.

## What Changes

- **`bevy-ts/level_catalog`**: the project's scene components (name, file, line, props) and six built-ins (cube, sphere, plane, point light, directional light, camera), each with the `LevelOp` that adds it.
- **Built-in shapes**: the bevy-ts package ships `bevy_ts::Cube`, `bevy_ts::Sphere` and `bevy_ts::Plane` scene components, so a level can place a primitive mesh (`.bsn` cannot write a TSX `<mesh>`).
- **Tree rows say their level**: each `bevy-ts/tsx_tree` node a level placed gains `level: { file, namePath, written }`. Each `app.scene` root gains `sidecar`, its sidecar level's path.
- **New ops**: `duplicate {path, name?}`, `add_component {path, component, fields?}`, `remove_component {path, component}`, each with its inverse and the existing refusals.
- **Changed ops**:
  - `name` becomes optional on `add_entity`, `add_instance` and `duplicate`; the engine picks a free sibling name.
  - `add_entity` takes `asset` (`"models/x.glb#Scene0"`), written as `SceneRoot("…")`.
  - `reparent` takes `index`, keeps the entity and its state, and its inverse returns it to its old index.
  - `restore` honours its index.
- **`level_write` result** gains `created: [{ namePath, entity }]`, so a client can select what it added.
- **Sidecar adds**: `level_write` on a scene's sidecar path creates the file on first write, and adds go there.
- **Protocol: the next free minor when it lands** (lead@bevy-ts allocates; planned 1.12 to 1.14), additions only, generated into Rust and TS types as before.
- **Out of scope**: abyme's outliner, inspector and asset browser (teams bevy-outliner, bevy-inspector, bevy-assets); editing an included level's entities from the including file; a "removed" marker for prefab-internal entities or components.

## Capabilities

### New Capabilities
None.

### Modified Capabilities
- `level-writing`: the operation list (duplicate, add/remove component, asset, optional names, reparent index), the `created` result, the catalog, and adds into a sidecar level.
- `levels`: level provenance on `tsx_tree` rows; the built-in shape scene components.

## Impact

- `crates/bevy-ts-levels`: `writer.rs` (new ops, naming, reparent index and rekey, restore index), `protocol.rs` (op JSON, `level_catalog`, `created`), `edit.rs` (rekey on reparent, sidecar creation, prefab component check), `engine.rs` (tree row provenance).
- `crates/bevy-ts-tsx`: `tsx_tree` nodes carry an optional `level` filled by the levels crate; roots carry `sidecar`.
- `packages/bevy-ts`: three built-in scene components in the runtime, always in the generation's manifest. This adds names to every project's scene namespace under `bevy_ts::`. Operator's go asked: it is a new pattern (scene components shipped by the engine rather than the project).
- `protocol/bevy-ts.openrpc.json`: the minor lead@bevy-ts allocates at landing, regenerated types.
- Performance: none expected. Writes and catalog reads are on demand; the tree's `level` field is computed per reconcile from `LevelInstance`, which already exists.
- abyme: its three teams gate on `hello` reporting the version this lands as.
