# level-writing Specification

## Purpose
How a level changes: the viewport, agents and the protocol edit `.bsn` files through one writer that splices text by name path, keeps formatting and comments, verifies its own output, routes each edit to the right owner (`levels.prefabEdits`), moves overrides inward with Apply, and lets the file on disk win over a pending edit.

## Requirements

### Requirement: Writes are operations on name paths
`bevy-ts/level_write` SHALL take a batch of operations on one level file, with the base content hash, and SHALL apply the whole batch or none of it. The operations are:
- `set`: name path, component, field path, value. On an entity with no patch of that component it adds one.
- `revert`: remove an override
- `set_prop`
- `revert_prop`: remove a scene prop or scene component field from an instance's `@Comp { … }`, and the braces when it was the only one
- `add_entity`: parent, optional name, patches, and optional `asset`
- `add_instance`: parent, optional name, and a prefab with props or an included level
- `add_component`: path, component, optional fields
- `remove_component`: path, component
- `delete`
- `rename`
- `reparent`: path, parent, optional index
- `restore`: parent, index, entity text (the inverse of a delete)

It SHALL return the new content hash, the changed line ranges, the inverse batch, and `created`: one `{ namePath, entity }` per entity the batch added, in order. The write SHALL reconcile the level before it replies, so `entity` is the spawned entity's bits, or null only when the level is not open. bevy-ts SHALL be the only component that writes `.bsn` files. Viewport `bsn` edits SHALL become these operations inside the engine.

An operation on an entity that is not written in the file (a prefab's inner entity, or an included level's) SHALL be refused with `prefab-internal`, except `set` and `revert`, which write overrides. A parent that is not written in the file SHALL be refused the same way. So SHALL an instance as the parent of `add_entity`, `add_instance`, `reparent` or `restore`: its children are its prefab's, and a child added there could not be deleted.

#### Scenario: Setting a translation
- **WHEN** a client sends `set` on `EnemyB`, `Transform.translation.x`, `5.0`, with the current hash
- **THEN** the file's `x: 3.0` becomes `x: 5.0`, and the reply has the new hash, that line, and an inverse `set` to `3.0`

#### Scenario: Deleting a prefab-internal entity
- **WHEN** a client sends `delete` on `EnemyB/eye`
- **THEN** the batch is rejected with `reason: "prefab-internal"` and the file is unchanged

#### Scenario: Adding an instance and selecting it
- **WHEN** a client sends `add_instance` with scene `enemy::Enemy` at the top level and no name
- **THEN** `arena.bsn` gains `#Enemy` with `@enemy::Enemy`, and `created` is `[{ namePath: ["Enemy"], entity: <bits> }]` for the enemy now in the world

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

The `tsx` edit SHALL point at the element that sets the field (`<Transform translation={…} />`), not the entity element it is in, with the source chain above it for a passed-through prop. A field only a level sets has no TSX owner, and its apply SHALL be refused.

An `@prop` (field `@hp`, component the scene component) SHALL move the same way. Into a `.bsn` owner it is a `set_prop` there and a `revert_prop` in its own file, in one transaction. Into TSX the edit SHALL be the prop's default in the scene's `static scene({ hp = … })` destructuring (`target: "default"`), and the batch that follows SHALL be the `revert_prop`. A prop the scene computes its default for, or does not destructure, SHALL be refused.

#### Scenario: Apply to prefab
- **WHEN** the operator applies enemy `b`'s eye override to the prefab and the editor writes `enemy.tsx`
- **THEN** the `#eye` override lines leave `arena.bsn`, and both enemies' eyes take the new value

#### Scenario: The edit names the element that sets the field
- **WHEN** the eye's `translation` is overridden, and `enemy.tsx` has `<entity key="eye">` on line 16 and `<Transform translation={…} />` on line 17
- **THEN** the edit's element is line 17, prop `translation`

#### Scenario: Applying an @prop
- **WHEN** grunt `b`'s `@hp: 250.0` is overridden in `arena.bsn` and the operator applies it to the prefab
- **THEN** the edit sets the default of `hp` in `Grunt`'s `static scene({ hp = 100 })` to `250`, and after the editor answers `written`, `arena.bsn` reads `@enemies::Grunt`

### Requirement: Scenes without a level get a sidecar level
When the editor answers `rejected` with `computed` for an entity of an `app.scene` root that no level placed, bevy-ts SHALL write the edit as an override in that scene's sidecar level, `levels/<scene>.bsn` or the path given by `app.scene(name, fn, { level })`. The sidecar SHALL be created on first write, and SHALL layer over the scene by `tsx` key path.

Each `app.scene` root in `bevy-ts/tsx_tree` SHALL carry `sidecar`, its sidecar level's path, whether or not the file exists. A `level_write` whose file is that path SHALL create the file on first write and open it as the scene's overlay. Entities added to a sidecar SHALL be top-level entities of the scene and fully editable. The scene's TSX entities SHALL stay refused for `rename`, `delete` and `reparent` with `prefab-internal`.

#### Scenario: A computed position in a TSX scene
- **WHEN** an entity of scene `arena` has `at={spawnPoint(2)}` and is dragged
- **THEN** `levels/arena.bsn` is created with an override for that key path, and the entity keeps its new position after reloads

#### Scenario: Adding a light to a scene with no level
- **WHEN** the `main` scene has no level, its root's `sidecar` is `levels/main.bsn`, and a client sends `add_entity` named `PointLight` with a `PointLight` patch to that file
- **THEN** `levels/main.bsn` is created holding `#PointLight`, a light spawns in the scene, and its tree row has `level: { file: "levels/main.bsn", namePath: ["PointLight"], written: true }`

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

### Requirement: The engine names what a write adds
When `add_entity` or `add_instance` has no name, the writer SHALL pick the first name free among the new entity's siblings, from a base:
- the scene component's last path segment (`Enemy`)
- an include's or asset's file stem (`room`, `crate`)
- otherwise the first patch's component, or `Entity`

The base SHALL lose its trailing digits, then take the first free of `Base`, `Base2`, `Base3`, … A given name that is taken SHALL be refused with `exists`.

#### Scenario: Adding two enemies
- **WHEN** a client sends `add_instance` of `enemy::Enemy` twice with no name, at a top level that has `#EnemyA` and `#EnemyB`
- **THEN** the file gains `#Enemy` and then `#Enemy2`

### Requirement: Reparent keeps the entity and its place
`reparent` SHALL move the entity to `index` among the parent's written children, or last when `index` is absent. `index` SHALL count those children after the entity is taken out, so it is the entity's final place: with `[A, B, C]`, moving `A` with index 1 gives `[B, A, C]`. A reparent to the same parent with an index SHALL reorder. It SHALL keep the entity and its ECS state, as `rename` does, by moving its node in the retained tree before the reload. Its inverse SHALL return the entity to its old parent at its old index. `restore` SHALL insert at its index. A move under the entity itself SHALL be refused with `invalid`. The parent SHALL always be a name path in the same file.

#### Scenario: Reordering by index
- **WHEN** the top level is `A`, `B`, `C` and a client reparents `A` to the top level with index 1
- **THEN** the order is `B`, `A`, `C`, and `A` keeps its entity bits

#### Scenario: Undoing a reparent
- **WHEN** `A` is the first of three top-level entities, a client reparents it under `C`, then sends the inverse
- **THEN** `A` is under `C` with the same entity bits, then first at the top level again, and the file equals the original

#### Scenario: Undoing a delete in the middle
- **WHEN** a client deletes the second of three entities, then sends the inverse `restore`
- **THEN** the entity is second again and the file equals the original

### Requirement: Components are added and removed as patches
`add_component` SHALL append a patch, `Comp` or `Comp { field: value, … }`, in the entity's style, and the component SHALL take its defaults for fields not given. It SHALL be refused with `exists` when the entity already has the component, from the file or from its prefab. Its inverse SHALL be `remove_component`.

Both SHALL take `component` as a string: the component's name as `LevelOverride.component` gives it (`PointLight`, `enemy::Health`), or any path a `.bsn` patch resolves (a full type path, or a short name only one type has). They SHALL match a patch by the type it resolves to, not by its text, and a new patch SHALL write the shortest form the registry resolves unambiguously.

`remove_component` SHALL remove the entity's whole patch of that component from the file. It SHALL be refused with `not-found` when the file has no such patch, and with `prefab-internal` when the entity's prefab gives the component, since `.bsn` has no removed marker. Its inverse SHALL be `add_component` with `text`, the removed patch's bytes, so every field comes back as it was.

#### Scenario: Adding and removing a component
- **WHEN** a client sends `add_component` on `#Crate` with `NotShadowCaster`, then that write's inverse
- **THEN** the file gains a `NotShadowCaster` line and the crate stops casting shadows, then the line goes and the component with it

#### Scenario: Adding a component the entity has
- **WHEN** a client sends `add_component` on `#PointLight` with `PointLight`
- **THEN** the batch is refused with `exists` and the file is unchanged

#### Scenario: Removing a prefab's component
- **WHEN** a client sends `remove_component` on `EnemyB` with `Health`, which `enemy::Enemy` gives
- **THEN** the batch is refused with `prefab-internal` and the file is unchanged

### Requirement: An asset is placed by its path
`add_entity` with `asset` SHALL write the patch that spawns it:
- a `.glb` or `.gltf` path: `WorldAssetRoot("path#SceneN")`, with `#Scene0` when the path has no label
- a `.bsn` path: an inclusion, as `add_instance` with `include`
- any other type: refused with `invalid`

Its patches, such as a `Transform`, SHALL follow.

#### Scenario: Dropping a model
- **WHEN** a client sends `add_entity` with `asset: "models/crate.glb"` and a `Transform` patch with translation `[2, 0, 1]`
- **THEN** the file gains `#crate`, `WorldAssetRoot("models/crate.glb#Scene0")` and the `Transform`, and the crate model spawns there

### Requirement: The catalog of what a level can add
`bevy-ts/level_catalog` SHALL answer `scenes`: every scene component of the current generation that a level can place, with name, file, line, and props (name, type, default, required). It SHALL list no built-in entities: an empty entity is `add_entity` with no base, and an asset is `add_entity` with `asset`. The catalog SHALL have no watch: it changes only when a TS reload applies.

#### Scenario: Listing the arena's catalog
- **WHEN** a client calls `bevy-ts/level_catalog` on the arena
- **THEN** `scenes` has `enemy::Enemy` at `src/enemy.tsx` with props `hp` (f32, 100) and `at`, and nothing else is listed
