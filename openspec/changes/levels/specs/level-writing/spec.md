## ADDED Requirements

### Requirement: Writes are operations on name paths
`bevy-ts/level_write` SHALL take a batch of operations on one level file, with the base content hash, and SHALL apply the whole batch or none of it. The operations are:
- `set`: name path, component, field path, value
- `revert`: remove an override
- `set_prop`
- `add_instance`: a prefab with props, or an included level
- `add_entity`
- `delete`
- `rename`
- `reparent`

It SHALL return the new content hash, the changed line ranges and the inverse batch. bevy-ts SHALL be the only component that writes `.bsn` files. Viewport `bsn` edits SHALL become these operations inside the engine.

#### Scenario: Setting a translation
- **WHEN** a client sends `set` on `EnemyB`, `Transform.translation.x`, `5.0`, with the current hash
- **THEN** the file's `x: 3.0` becomes `x: 5.0`, and the reply has the new hash, that line, and an inverse `set` to `3.0`

#### Scenario: Deleting a prefab-internal entity
- **WHEN** a client sends `delete` on `EnemyB/eye`
- **THEN** the batch is rejected with `reason: "prefab-internal"` and the file is unchanged

### Requirement: Writes are minimal and keep formatting
The writer SHALL edit by splicing text into the original bytes, never by re-emitting the file:
- Setting an existing value SHALL change only that value's bytes.
- A new field SHALL go after the last field of its patch, in that patch's style (one line or several).
- A new patch or entity SHALL go on lines of its own, with indentation and separator style copied from its nearest sibling, or four spaces when the file has none.
- A deletion SHALL remove the entity's span, its leading comments and one adjacent `--`.

All other bytes SHALL be unchanged. New values SHALL use these forms:
- floats in the shortest form that round-trips, always with a `.`
- type paths in the shortest form the registry resolves unambiguously, unless the file already writes that type in full
- only the leaf fields that differ from the value below the override

#### Scenario: A one-field edit in git diff
- **WHEN** the viewport moves enemy `b` along x and the edit is written
- **THEN** `git diff` shows exactly one changed line, and every comment and blank line in the file is where it was

#### Scenario: A new override on a single-line patch
- **WHEN** `Transform { translation: Vec3 { x: 3.0 } }` gets `z = -2.0`
- **THEN** the line becomes `Transform { translation: Vec3 { x: 3.0, z: -2.0 } }`

### Requirement: The writer verifies its own output
After splicing, the writer SHALL re-parse the result and check that the new tree differs from the old one by exactly the batch. If it differs in any other way, the writer SHALL NOT write, and SHALL reject the batch with `reason: "internal"` and both trees' difference. Files SHALL be written atomically, through a temporary file and a rename.

#### Scenario: A splice that would swallow a line
- **WHEN** a splice would land a new field after a trailing `//` comment on the same line
- **THEN** the writer either places it on its own line or rejects with `internal`; it never writes a file whose tree lost a field

### Requirement: An unnamed entity is named when first written
When an operation targets an entity with no name, the writer SHALL insert a name, unique among its siblings and derived from its prefab or first component, in the same write. A `rename` SHALL rename the node in the retained tree before the file change is reconciled, so the entity and its state are kept.

#### Scenario: Editing an unnamed crate
- **WHEN** the viewport moves the third unnamed `Crate` entity
- **THEN** the write inserts `#Crate3` and the changed value: two changed lines, and the entity does not respawn

### Requirement: Edits route by owner chain and the prefab-edit policy
For a field of a level-placed entity, the viewport's `bsn` route SHALL carry the owner chain: the TSX element, then every level frame up to the outermost. It SHALL also carry the chosen target. The project setting `levels.prefabEdits` SHALL choose that target:
- `override`, the default: every edit to a level-placed entity, the prefab's inner entities included, writes an override into the outermost level file. TSX is rewritten only by "Apply to prefab".
- `source`: an edit to a prefab-internal field whose TSX prop is a literal routes `tsx` to the editor, and every instance changes. Otherwise it falls back to `override`.

The instance root's `Transform` SHALL route `bsn` to the outermost level under both policies.

#### Scenario: Dragging an eye under override
- **WHEN** `prefabEdits` is `override` and the operator drags enemy `b`'s `#eye`
- **THEN** `arena.bsn` gains `Children [ #eye Transform { … } ]` under `#EnemyB`, and `enemy.tsx` is untouched

#### Scenario: Dragging an eye under source
- **WHEN** `prefabEdits` is `source`, the eye's translation is a literal in `enemy.tsx`, and the operator drags it
- **THEN** the edit routes `tsx`, the editor rewrites `enemy.tsx`, and both enemies' eyes move after the reload

### Requirement: Apply moves an override one owner inward
`bevy-ts/level_apply` (`entity`, field, target frame) SHALL move an override from its file to the next owner inward. When that owner is another `.bsn` file, the writer SHALL write the inner file and remove the outer override in one transaction. When it is TSX, bevy-ts SHALL emit a `tsx` route edit, and SHALL remove the `.bsn` override only after the editor answers `written`. If the answer is `rejected`, the override SHALL stay and the apply SHALL report the reason.

#### Scenario: Apply to prefab
- **WHEN** the operator applies enemy `b`'s eye override to the prefab and the editor writes `enemy.tsx`
- **THEN** the `#eye` override lines leave `arena.bsn`, and both enemies' eyes take the new value

### Requirement: Scenes without a level get a sidecar level
When the editor answers `rejected` with `computed` for an entity of an `app.scene` root that no level placed, bevy-ts SHALL write the edit as an override in that scene's sidecar level, `levels/<scene>.bsn` or the path given by `app.scene(name, fn, { level })`. The sidecar SHALL be created on first write, and SHALL layer over the scene by `tsx` key path.

#### Scenario: A computed position in a TSX scene
- **WHEN** an entity of scene `arena` has `at={spawnPoint(2)}` and is dragged
- **THEN** `levels/arena.bsn` is created with an override for that key path, and the entity keeps its new position after reloads

### Requirement: The disk wins over a pending edit
Each write SHALL compare the file on disk with the batch's base hash:
- If they match, the writer SHALL write.
- If they differ, the writer SHALL apply the disk version to the world as a reload, then replay the batch by name path on the new tree.
  - If every target and field still exists and the disk did not change the same field, the writer SHALL write and report `rebased`.
  - Otherwise it SHALL reject with `conflict`, the reason and the disk value, and the viewport SHALL restore the disk value within one frame.

The writer SHALL never perform a text merge, and SHALL never overwrite a change it has not parsed.

#### Scenario: An agent edits another entity mid-drag
- **WHEN** an agent saves a change to `#EnemyA` while enemy `b` is dragged, and the drag is released
- **THEN** enemy `a`'s change applies first, enemy `b`'s edit is written on top, and the result is `rebased`

#### Scenario: The target is deleted on disk
- **WHEN** an agent deletes `#EnemyB` from the file during the drag
- **THEN** the edit is `conflict` with `reason: "target removed"`, and enemy `b` despawns as the file says

### Requirement: Write notifications
`bevy-ts/level+watch` SHALL stream, for each level:
- `loaded`
- `failed`, with diagnostics
- `written`, with hash, lines and the operation id
- `external`, when a change not made by the writer is detected
- `rebased`
- `conflict`

The `written` message SHALL serve as viewport's `bsn` write confirmation.

#### Scenario: abyme keeps an open buffer fresh
- **WHEN** `arena.bsn` is open in abyme's editor and the viewport writes it
- **THEN** abyme receives `written` with the new hash and lines, and can reload its buffer
