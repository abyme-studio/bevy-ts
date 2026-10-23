# scene-reconciler Specification

## Purpose
Scenes are declared per generation, and a keyed reconciler in Rust owns the retained tree. It diffs each new entity description against the last one applied and writes the world only where it changed, in batches and never per frame, touching only components it inserted and spawning through BSN templates.

## Requirements

### Requirement: Scenes are declared per generation
`app.scene(name, fn)` SHALL declare a scene root. Each time a generation is applied, bevy-ts SHALL call every declared scene function during the generation's load and send each root's description to Rust with the generation. Two roots with one name in a generation SHALL fail the load. A root missing from a new generation SHALL be unmounted, despawning the entities it made.

#### Scenario: First mount
- **WHEN** `game.tsx` declares `app.scene("arena", () => <Enemy key="a" at={[0, 0, 0]} />)` and loads
- **THEN** one enemy entity exists in the first frame of that generation

### Requirement: The retained tree lives in Rust
`bevy-ts-tsx` SHALL keep the retained tree as ECS state: a `TsxNode` component on every entity it spawned (node id, key path, element types, source chain, components it inserted, last applied controlled values, prop provenance) and a `TsxTree` resource (roots, node id to entity, tombstones, subscriptions, template cache). JS SHALL hold no copy of it between calls.

#### Scenario: The tree outlives JS state
- **WHEN** a generation is applied after the previous isolate state was thrown away
- **THEN** the reconciler still knows every mounted node and its last applied values

### Requirement: Diff description against description, write only changes
Reconciling a root SHALL compare the new description with the last applied one, matched by key path, and SHALL write the world only for: nodes added (spawn), nodes removed (despawn with descendants), component elements added (insert) or removed (remove, if tsx inserted them), and controlled fields whose declared value changed. It SHALL NOT compare against or reset world values a description did not change.

#### Scenario: Nothing changed
- **WHEN** a generation whose scene descriptions are identical to the last applied ones is applied
- **THEN** no entity is spawned, despawned, inserted, removed or written, and no change tick moves

### Requirement: Controlled and initial fields
A controlled field SHALL be written when its component is inserted and whenever its declared value differs from the last applied one. An initial field SHALL be written only when its component is inserted by tsx. A field given neither way SHALL take the declared default on insert and SHALL NOT be written afterwards.

#### Scenario: A system moved a controlled field
- **WHEN** a system changes enemy `a`'s `Transform.translation` and the next reload leaves `translation={at}` unchanged
- **THEN** the enemy stays where the system put it

#### Scenario: The declared value changes
- **WHEN** the source changes `max={250}` to `max={400}` on enemy `b` and reloads
- **THEN** `b`'s `Health.max` becomes 400 and its `hp` is unchanged

### Requirement: Components tsx did not insert are never touched
The reconciler SHALL remove or write only components it inserted on its own entities. Components added by systems, the protocol or other plugins SHALL survive every reconcile, and entities systems spawned SHALL never be matched to nodes.

#### Scenario: A system tags an enemy
- **WHEN** a system inserts `game::Stunned` on enemy `a` and the scene reloads
- **THEN** `a` still has `game::Stunned`

### Requirement: Application is batched and never per frame
All world writes of one reconcile SHALL be applied in one step at the generation's frame boundary or, for runtime changes, in one step after the host call that produced them. The reconciler SHALL run only on generation apply, on runtime ops (mounts, unmounts, patches) and on `restart`, never as a per-frame pass.

#### Scenario: A still scene
- **WHEN** a scene with 1,000 reconciled entities runs 600 frames with no reload, binding change or event
- **THEN** the reconciler does no work in any of those frames

### Requirement: BSN templates for spawning
Spawning a description subtree SHALL go through Bevy 0.20's BSN scene machinery where it can be built from runtime data, so asset intrinsics (`<mesh>`, `<material>`), `Name` and observers behave as BSN templates. Static subtrees (no bindings, no handlers) SHALL be cached by content hash and reused for later spawns. Updates after spawn SHALL be field patches applied by `bevy-ts-tsx`. If BSN cannot be built from runtime data, spawning SHALL fall back to direct insertion with the same observable results.

#### Scenario: Spawning the same prefab twice
- **WHEN** two `<Enemy>` elements with identical props are mounted
- **THEN** the second spawn reuses the cached template built for the first

### Requirement: Mounting from systems and handlers
A TS system (through its `Commands` param) or a handler SHALL be able to mount a description at runtime with `commands.mount(element, { parent? })` and unmount with `commands.unmount(node)`, applied in order with that run's other commands. Runtime-mounted nodes SHALL be keyed under a runtime root and SHALL survive reloads like scene nodes, until unmounted or despawned.

#### Scenario: A handler spawns an enemy
- **WHEN** a button's `onPointerClick` calls `commands.mount(<Enemy key="e9" at={[0, 0, -6]} />)`
- **THEN** after that frame's host call an enemy entity exists with a `TsxNode` whose key path is under the runtime root

### Requirement: A slot's reconcile costs what it changes
All mounts and unmounts one host call returns for a `<Show>` or `<For>` slot SHALL apply as one reconcile of that slot. What that reconcile may despawn SHALL be found from the nodes under the groups it was given, never by walking every node of the slot's root. A spawned node's native-layout components (TS components and Rust components with a native layout) SHALL be inserted in one step, with its `ChildOf` and `TsxNode`, so the entity moves archetype once for them.

#### Scenario: Many groups in one call
- **WHEN** one host call mounts 1,000 groups into one `<For>` slot
- **THEN** the slot is reconciled once, and the nodes it checks for despawn are the nodes under those 1,000 groups, whatever else the root holds

#### Scenario: One group among many
- **WHEN** a slot already holds 10,000 groups and one host call mounts one more
- **THEN** the reconcile checks only the new group's nodes for despawn, and no other group's entity is written

### Requirement: Applying a root at key paths
`bevy-ts-tsx` SHALL apply a root at given key paths only: for each path, the subtree at that path SHALL be reconciled against a given resolved subtree (`reconcile`), or despawned and spawned again from it (`respawn`), or despawned when none is given. Nodes outside the given subtrees SHALL NOT be written, and their change ticks SHALL NOT move. A spawned subtree SHALL be placed under its described parent in description order. A respawned node SHALL clear its tombstone. The root's retained tree, its kept resolved tree and `tsx_tree+watch` SHALL be updated for the given paths as a whole-root reconcile updates them. This is the one primitive for partial applies: a Stop's restore and a level write's reconcile both use it.

#### Scenario: Respawning one enemy
- **WHEN** a root holds enemies `a`, `b` and `c`, and the root is applied at `b` with `respawn`
- **THEN** `b` and its children are new entities with their described values, in `b`'s place among its siblings, and `a` and `c` keep their entities and change ticks

#### Scenario: Reconciling one path
- **WHEN** enemy `b`'s resolved subtree changed one controlled field, and the root is applied at `b` with `reconcile`
- **THEN** only that field is written, under the controlled and initial field rules

#### Scenario: A tombstoned node
- **WHEN** enemy `a` is tombstoned and the root is applied at `a` with `respawn`
- **THEN** `a` exists again and its tombstone is gone

### Requirement: An external root keeps its resolved tree
After applying a root a Rust plugin describes (an external root, such as a level), `bevy-ts-tsx` SHALL keep the resolved tree it applied, shared and not copied, until the root is applied again or unmounted. A partial apply SHALL replace only the subtrees it was given. A restart SHALL drop every kept tree.

#### Scenario: Kept across a frame
- **WHEN** a level root has been applied and no reload has happened since
- **THEN** its kept resolved tree describes every node `tsx_tree` lists for it

### Requirement: The game-start baseline
When the game starts (plugin-api, "Start notification"), `bevy-ts-tsx` SHALL record a baseline for each external root then applied: the world's change tick, the archetype of each of its entities and of the entities below them that are not nodes (down to the next node), and the root's apply count. A root applied or unmounted after the baseline SHALL have none. A restart or a Stop SHALL drop every baseline. Given a root with a baseline, tsx SHALL answer the root's changed nodes: those whose entity, or an entity below it that is not a node, has a component added or changed after the baseline tick, an archetype other than the recorded one, or is gone. Changes tsx made itself through a binding count as changes. A component Bevy derives every frame from other state, which a respawn cannot restore, SHALL NOT count: `ViewVisibility`, `VisibleEntities`, `Clusters` and `PreviousViewData`, one list tsx owns, each entry there because it was seen to change in a play that touched nothing. A component holding the game's settings counts even when Bevy writes it every frame (`Camera`). The answer SHALL be read before the Stop's rebuild despawns TS-spawned entities, so a child the game spawned and the rebuild despawned marks its parent only if the game's spawn did.

#### Scenario: A system moves one enemy
- **WHEN** the game started with 10,000 enemies applied and a system wrote one enemy's `Transform`
- **THEN** that enemy is the root's one changed node

#### Scenario: A component removed
- **WHEN** a system removed `Health` from enemy `c`
- **THEN** `c` is a changed node

#### Scenario: Play's camera swap
- **WHEN** the game started with enemies the editor camera sees and the game's camera does not, and the play touched none
- **THEN** the root has no changed node

#### Scenario: A root opened during play
- **WHEN** a level is applied after the game started
- **THEN** it has no baseline
