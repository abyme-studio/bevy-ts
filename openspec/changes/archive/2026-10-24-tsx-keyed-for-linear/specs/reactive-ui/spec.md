## MODIFIED Requirements

### Requirement: Control flow components
`bevy-ts` SHALL export `<Show when fallback>` and `<For each key>`. When their inputs change, they SHALL mount or unmount only the affected subtree through the reconciler, keyed by `key` for `<For>` items, and dispose the owners of unmounted subtrees. A `<For>`'s item entities SHALL be in its items' order among their parent's children, and a changed order SHALL move entities without despawning any. The work of one `<For>` change SHALL grow with the items added, removed or moved, not with the items already mounted: growing a list from 0 to N items SHALL cost time linear in N.

#### Scenario: A list item removed
- **WHEN** a `<For each={items} key={(i) => i.id}>` loses the middle item
- **THEN** only that item's entities are despawned and the other items keep their entity ids

#### Scenario: Bound children of a mounted subtree
- **WHEN** `<Show when={() => s() !== null}><text>{() => `show ${s()}`}</text></Show>` mounts with `s` already set, or a `<Show>` or `<For>` mounts its content later (a signal set by a system or a handler)
- **THEN** each bound child shows its value in the frame its subtree mounts, and follows its signals afterwards

#### Scenario: A list reversed
- **WHEN** a `<For>` over `["a", "b", "c"]` is set to `["c", "b", "a"]`
- **THEN** no entity is spawned or despawned, every item keeps its entity id, and the parent's `Children` hold the items' entities in the order c, b, a

#### Scenario: A list grows from nothing
- **WHEN** a `<For>`'s items go from 0 to 10,000 in one signal set
- **THEN** the 10,000 items mount in the next host call, and the time taken at 100,000 items is no more than about ten times the time at 10,000 (`bench/tsx-for/`)
