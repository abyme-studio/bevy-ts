## Context

`crates/bevy-ts-levels/src/writer.rs` already plans `Add`, `Delete`, `Rename`, `Reparent` and `Restore` as text splices, each re-parsed and checked, with an inverse. What exists today, read from the code:
- `set` on an entity without that component appends a patch (`plan_set`, `add_entry`).
- `plan_reparent` removes the entity and appends it last; it returns no renames, so `edit.rs` does not `rekey`, and the entity respawns under its new node id.
- `plan_restore` ignores `index` and appends last, so undoing a delete moves the entity to the end.
- `Op::Add` requires `name`.
- `edit.rs` reconciles the affected levels before `level_write` replies, so a spawned entity exists when the reply is built.
- `tsx_tree` rows of a level root have `keyPath` equal to the name path from the file's top; prefab key segments continue it. `LevelInstance` holds level, name path and owners for each placed entity.
- Scene components come from the generation's manifest (`ComponentDecl.scene`).

The protocol was agreed with spec@bevy-outliner, spec@bevy-inspector and spec@bevy-assets (through the outliner) on 2026-10-22, under the operator's ruling of the same day: nothing Bevy's own editor lacks and is not discussing. bevy_editor_prototypes spawns an empty entity (`Name`, `Transform`, `Visibility`), drops a glTF as a `SceneRoot`, tracks added and removed components in `bevy_undo`, and its roadmap's stage 2 plans spawning, adding and removing components, and saving scenes to `.bsn`. It has no primitive spawn and no duplicate. Rename, delete and reparent already ship in `level_write`; this change only fixes reparent and restore.

## Goals / Non-Goals

**Goals:** the studio's outliner, inspector and asset browser can do every edit they offer through `level_write`, know which rows they may edit, and find what they added.

**Non-Goals:** built-in primitives (a game that wants a cube ships it as its own scene component); duplicate; editing an included level's entities from the including file (they write `room.bsn` by opening it); a removed marker for prefab-internal entities or components; a watch on the catalog.

## Decisions

1. **The writer stays text-only; the engine adds world checks.** `writer.rs` decides by the file alone (`not-found`, `exists` among siblings, `prefab-internal` for a path under an instance). Checks that need the world run in `edit.rs` before the writer: `add_component` is `exists` when the entity has the component; `remove_component` is `prefab-internal` when the entity's prefab expansion gives it (the expansion's root components, which compose already builds). Keeps the writer unit-testable on strings.

2. **Reparent reuses the rename path.** `plan_reparent` returns `(old path, new path)` in `renamed`, and `edit.rs` already calls `bevy_ts_tsx::rekey` for each, which moves nodes by key path whatever the parent. The reconcile that follows sets the new `ChildOf`. Alternative, despawn and respawn: loses state, which the studio would show as a flicker and a reset.

3. **Index means position among written siblings.** Unnamed and named entities alike count. `restore` and `reparent` insert before the sibling at that index, or append past the end, with the indent and separator copied as `append_child` does today.

4. **`created` comes from the batch's touched paths.** Each add-like op records its new name path; after reconcile, `edit.rs` looks up `node_id(root, path)` in `TsxTree`. No new selection API: the client calls `viewport_select { entities }`.

5. **Naming is in the writer.** Siblings are known from the parse, so the free name is picked during planning, and a later op in the same batch sees it.

6. **`remove_component`'s inverse carries raw text.** `add_component` gains an internal `text` (the patch's bytes). Rebuilding from values would fail for values `to_wvalue` cannot express and would reformat the patch.

7. **Tree provenance is filled by the levels crate.** `bevy-ts-tsx` gains an optional `level` slot on its tree node output and a hook the levels crate registers, reading `LevelInstance`; `written` is whether the writer's `find` resolves the name path in the outermost file. tsx stays ignorant of levels. `sidecar` on scene roots comes from the same hook, from `app.scene`'s `level` option or `levels/<scene>.bsn`.

8. **Sidecar creation on `level_write`.** When `file` is a scene's sidecar path and the file is missing, `edit.rs` starts from empty text with base hash `null`, writes, and opens the level as that scene's overlay, as the computed-edit path already does.

## Risks / Trade-offs

- [`written` costs a parse lookup per row per reconcile] → computed only for nodes whose `LevelInstance` changed, from the parsed file the level already holds.
- [Reparent across a prefab boundary] → refused `prefab-internal`; the studio greys the drop target using `written`.

## Migration Plan

Additions only; 1.10 clients are unchanged. `Op::Add`'s `name` becomes optional in JSON and in Rust (`Option<String>`).
