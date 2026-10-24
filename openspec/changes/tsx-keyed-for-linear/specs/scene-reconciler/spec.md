## ADDED Requirements

### Requirement: A slot's reconcile costs what it changes
All mounts and unmounts one host call returns for a `<Show>` or `<For>` slot SHALL apply as one reconcile of that slot. What that reconcile may despawn SHALL be found from the nodes under the groups it was given, never by walking every node of the slot's root. A spawned node's native-layout components (TS components and Rust components with a native layout) SHALL be inserted in one step, with its `ChildOf` and `TsxNode`, so the entity moves archetype once for them.

#### Scenario: Many groups in one call
- **WHEN** one host call mounts 1,000 groups into one `<For>` slot
- **THEN** the slot is reconciled once, and the nodes it checks for despawn are the nodes under those 1,000 groups, whatever else the root holds

#### Scenario: One group among many
- **WHEN** a slot already holds 10,000 groups and one host call mounts one more
- **THEN** the reconcile checks only the new group's nodes for despawn, and no other group's entity is written
