## MODIFIED Requirements

### Requirement: Writes are operations on name paths
`bevy-ts/level_write` SHALL take a batch of operations on one level file, with the base content hash, and SHALL apply the whole batch or none of it. The operations are:
- `set`: name path, component, field path, value. On an entity with no patch of that component it adds one.
- `revert`: remove an override
- `set_prop`
- `revert_prop`: remove a scene prop or scene component field from an instance's `@Comp { … }`, and the braces when it was the only one
- `add_entity`: parent, optional name, patches, and optional `asset`
- `add_instance`: parent, optional name, and a prefab with props or an included level
- `duplicate`: path, optional name
- `add_component`: path, component, optional fields
- `remove_component`: path, component
- `delete`
- `rename`
- `reparent`: path, parent, optional index
- `restore`: parent, index, entity text (the inverse of a delete)

It SHALL return the new content hash, the changed line ranges, the inverse batch, and `created`: one `{ namePath, entity }` per entity the batch added, in order. The write SHALL reconcile the level before it replies, so `entity` is the spawned entity's bits, or null only when the level is not open. bevy-ts SHALL be the only component that writes `.bsn` files. Viewport `bsn` edits SHALL become these operations inside the engine.

An operation on an entity that is not written in the file (a prefab's inner entity, or an included level's) SHALL be refused with `prefab-internal`, except `set` and `revert`, which write overrides. A parent that is not written in the file SHALL be refused the same way.

#### Scenario: Setting a translation
- **WHEN** a client sends `set` on `EnemyB`, `Transform.translation.x`, `5.0`, with the current hash
- **THEN** the file's `x: 3.0` becomes `x: 5.0`, and the reply has the new hash, that line, and an inverse `set` to `3.0`

#### Scenario: Deleting a prefab-internal entity
- **WHEN** a client sends `delete` on `EnemyB/eye`
- **THEN** the batch is rejected with `reason: "prefab-internal"` and the file is unchanged

#### Scenario: Adding an instance and selecting it
- **WHEN** a client sends `add_instance` with scene `enemy::Enemy` at the top level and no name
- **THEN** `arena.bsn` gains `#Enemy` with `@enemy::Enemy`, and `created` is `[{ namePath: ["Enemy"], entity: <bits> }]` for the enemy now in the world

### Requirement: Scenes without a level get a sidecar level
When the editor answers `rejected` with `computed` for an entity of an `app.scene` root that no level placed, bevy-ts SHALL write the edit as an override in that scene's sidecar level, `levels/<scene>.bsn` or the path given by `app.scene(name, fn, { level })`. The sidecar SHALL be created on first write, and SHALL layer over the scene by `tsx` key path.

Each `app.scene` root in `bevy-ts/tsx_tree` SHALL carry `sidecar`, its sidecar level's path, whether or not the file exists. A `level_write` whose file is that path SHALL create the file on first write and open it as the scene's overlay. Entities added to a sidecar SHALL be top-level entities of the scene and fully editable. The scene's TSX entities SHALL stay refused for `rename`, `delete`, `reparent` and `duplicate` with `prefab-internal`.

#### Scenario: A computed position in a TSX scene
- **WHEN** an entity of scene `arena` has `at={spawnPoint(2)}` and is dragged
- **THEN** `levels/arena.bsn` is created with an override for that key path, and the entity keeps its new position after reloads

#### Scenario: Adding a light to a scene with no level
- **WHEN** the `main` scene has no level, its root's `sidecar` is `levels/main.bsn`, and a client sends `add_entity` with the point light built-in's op to that file
- **THEN** `levels/main.bsn` is created holding `#PointLight`, a light spawns in the scene, and its tree row has `level: { file: "levels/main.bsn", namePath: ["PointLight"], written: true }`

## ADDED Requirements

### Requirement: The engine names what a write adds
When `add_entity`, `add_instance` or `duplicate` has no name, the writer SHALL pick the first name free among the new entity's siblings, from a base:
- the scene component's last path segment (`Enemy`)
- the built-in's label without spaces (`PointLight`)
- an include's or asset's file stem (`room`, `crate`)
- for `duplicate`, the source's name
- otherwise the first patch's component, or `Entity`

The base SHALL lose its trailing digits, then take the first free of `Base`, `Base2`, `Base3`, … A given name that is taken SHALL be refused with `exists`.

#### Scenario: Duplicating twice
- **WHEN** a client duplicates `EnemyB` twice with no name
- **THEN** the file gains `#EnemyB2` and then `#EnemyB3`, each right after its source

### Requirement: Duplicate copies an entity's text
`duplicate` SHALL copy the entity's text, its children and overrides included and its leading comments excluded, to a new sibling right after it, renamed. Its inverse SHALL be a `delete` of the copy.

#### Scenario: Duplicating a room
- **WHEN** a client duplicates `RoomB`, which includes `room.bsn` and overrides `#Lamp`'s intensity
- **THEN** `#RoomB2` follows `#RoomB` with the same inclusion, `Transform` and `#Lamp` override, and a third room spawns

### Requirement: Reparent keeps the entity and its place
`reparent` SHALL move the entity to `index` among the parent's written children, or last when `index` is absent. A reparent to the same parent with an index SHALL reorder. It SHALL keep the entity and its ECS state, as `rename` does, by moving its node in the retained tree before the reload. Its inverse SHALL return the entity to its old parent at its old index. `restore` SHALL insert at its index. A move under the entity itself SHALL be refused with `invalid`. The parent SHALL always be a name path in the same file.

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
- a `.glb` or `.gltf` path: `SceneRoot("path#SceneN")`, with `#Scene0` when the path has no label
- a `.bsn` path: an inclusion, as `add_instance` with `include`
- any other type: refused with `invalid`

Its patches, such as a `Transform`, SHALL follow.

#### Scenario: Dropping a model
- **WHEN** a client sends `add_entity` with `asset: "models/crate.glb"` and a `Transform` patch with translation `[2, 0, 1]`
- **THEN** the file gains `#crate`, `SceneRoot("models/crate.glb#Scene0")` and the `Transform`, and the crate model spawns there

### Requirement: The catalog of what a level can add
`bevy-ts/level_catalog` SHALL answer:
- `scenes`: every scene component of the current generation that a level can place: name, file, line, and props (name, type, default, required)
- `builtins`: `cube`, `sphere`, `plane`, `point_light`, `directional_light` and `camera`, each with `id`, `label` and `op`, a `LevelOp` without `parent` and `name`

A client SHALL be able to send a built-in's `op` as given, adding `parent` and optionally `name`. The shapes SHALL be `add_instance` of `bevy_ts::Cube`, `bevy_ts::Sphere` and `bevy_ts::Plane`. The lights and the camera SHALL be `add_entity` with their component and a `Transform`. The catalog SHALL have no watch: it changes only when a TS reload applies.

#### Scenario: Listing the arena's catalog
- **WHEN** a client calls `bevy-ts/level_catalog` on the arena
- **THEN** `scenes` has `enemy::Enemy` at `src/enemy.tsx` with props `hp` (f32, 100) and `at`, and `builtins` has the six built-ins, `cube`'s op being `add_instance` of `bevy_ts::Cube`
