## ADDED Requirements

### Requirement: A reload reconciles every scene against what it last applied
On each applied generation, every scene root SHALL be re-evaluated and reconciled by key path under the scene-reconciler rules, after the generation's component declarations and migrations are applied and before its first system runs.

#### Scenario: Recolouring a prefab
- **WHEN** the `<mesh color>` inside `Enemy` changes and the file is saved
- **THEN** every enemy recolours in the generation's first frame, and every enemy keeps its `Transform` and `Health` values

### Requirement: initial values are never re-applied by a reload
A reload SHALL NOT write an `initial*` field on a component that already exists on the entity, whether or not its declared value changed.

#### Scenario: Changing an initial value
- **WHEN** damage has left enemy `b` at `hp = 180` and the source changes `initialHp={250}` to `initialHp={400}`
- **THEN** after the reload `b`'s `hp` is still 180

### Requirement: Simulation-despawned nodes stay despawned
When an entity tsx spawned is despawned by anything other than tsx, the reconciler SHALL tombstone its node, and later reloads SHALL NOT respawn it. `bevy-ts/reload` with `restart: true` SHALL clear every tombstone and the retained tree, so every scene mounts as if first. A node whose key path changes is a new node and SHALL spawn.

#### Scenario: A killed enemy across a reload
- **WHEN** a system despawns enemy `a` and an unrelated edit is saved
- **THEN** `a` is not respawned, and `bevy-ts/tsx_tree` lists it as tombstoned

#### Scenario: A restart
- **WHEN** `bevy-ts/reload` is called with `restart: true`
- **THEN** enemy `a` exists again with `hp` from its `initialHp`

### Requirement: Identity follows key paths across edits
A node SHALL keep its entity across a reload when its key path is unchanged, even if its source lines moved, its prefab function was renamed, or its components changed. A node whose key path disappears SHALL be despawned with its descendants.

#### Scenario: Renaming a prefab function
- **WHEN** `Enemy` is renamed to `Grunt` and every `<Enemy key=...>` becomes `<Grunt key=...>`
- **THEN** every enemy keeps its entity id and state

### Requirement: Bindings and handlers are rebuilt per generation
A generation SHALL re-register every binding, handler and subscription it creates, and the previous generation's ids SHALL be dropped when it is applied. A bound value that evaluates equal to its last applied value SHALL NOT be written.

#### Scenario: Editing a handler
- **WHEN** the Spawn button's `onPointerClick` body changes and reloads
- **THEN** the next click runs the new body, and the button entity is unchanged

### Requirement: A failing scene fails the generation
A scene function that throws, or a description the reconciler rejects (unknown component, a field given both ways, duplicate keys), SHALL fail the generation under bridge's failed-reload rule: no entity is spawned, despawned or written, and the previous generation's bindings and handlers keep working. The error SHALL carry the element's TS location.

#### Scenario: A typo in a component field
- **WHEN** a save writes `<Health mx={100} />`
- **THEN** the reload fails naming `mx` and `enemy.tsx` with its line, the HUD keeps updating, and the next good save applies normally
