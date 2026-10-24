# Custom relationships on TS components

Recommendation: the source (`Likes`) stays a plain-bytes layout-tier component with its `target`
`Entity` field at offset 0. The target (`LikedBy`) is a new, second component kind: a
Rust-owned `Vec<Entity>` registered by id, with a drop fn and a custom clone fn. bevy-ts owns
the hooks by id, on the dispatcher stage 7 of ecs-queries adds for required components. Scripts
read targets as query data from a per-table packed copy (offsets plus entity pairs) built before
the JS call. That costs no host call per row: about 4 ns per row plus about 1 ns per entity,
against 32-41 ns per row for a host call.

Researched 2026-10-23 against Bevy 0.20.0 (`Cargo.lock`), source in
`~/.cargo/registry/src/*/bevy_ecs-0.20.0`.

## 1. How Bevy 0.20 does it

- A relationship is two typed components. `Relationship` (the source, e.g. `ChildOf`) holds one
  `Entity`. `RelationshipTarget` (e.g. `Children`) holds a `Collection: RelationshipSourceCollection`.
  For `Vec<Entity>`, `add` pushes with no dedupe and `remove` drops the last match
  (`relationship/relationship_source_collection.rs:151-163`).
- Hooks live in the trait's default methods (`relationship/mod.rs`), and the derive wires them
  up:
  - source `on_insert`: a self-reference is warned about and removed (unless
    `allow_self_referential`). For a one-to-one collection, the old source is removed first.
    Then it queues `entry::<Target>().and_modify(add).or_insert(with_capacity(1)+add)`, a
    command, "necessary for batch mode". A missing target removes the relationship.
  - source `on_discard` (replace or remove): removes the source from the target's list at once.
    If the list is now empty, it queues a silenced command that removes the target component,
    but only if it is still empty.
  - target `on_discard`: `try_remove::<Relationship>()` on every source (a command).
  - target `on_despawn`, only when `linked_spawn`: `try_despawn()` on every source.
  - Every hook first checks `HookContext.relationship_hook_mode` (`Run`, `RunIfNotLinked`,
    `Skip`). Clone and move set it so hooks do not fight hand-built collections.
- The target list is only changed through `collection_mut_risky`. User code reads it; Bevy
  never hands out a mutable list.
- Clone (`clone_relationship_target`, `relationship/mod.rs`): the cloned target's collection
  starts empty. With linked cloning and `LINKED_SPAWN`, each source is added and its entity is
  queued for cloning (`queue_entity_clone`). On a move, each source's entity field is retargeted
  in a deferred step with `RelationshipHookMode::Skip`. The source clones as a value, with its
  entity mapped (`MapEntities`).
- Dynamic metadata, new in 0.20: `ComponentDescriptor::new_with_layout` takes a
  `RelationshipAccessorInitializer` (`component/info.rs:296-327`). It holds the source's
  `entity_field_offset`, or the target's `iter: unsafe fn(Ptr) -> Box<dyn Iterator<Item = Entity>>`,
  plus `linked_spawn` and a getter for the counterpart's id. It is metadata only: tests and
  `bevy_remote`'s JSON schema read it (`bevy_remote/src/schemas/json_schema.rs:90`). It does not
  run hooks. Hooks by id: `World::register_component_hooks_by_id`, which panics once the id is in
  any archetype (`world/mod.rs:359`). Each slot takes one hook (`try_on_*` returns `None` when
  the slot is already set). Hooks are plain `fn(DeferredWorld, HookContext)` and receive
  `component_id`, so one generic hook per slot can look up per-component data.
- `DeferredWorld::modify_component_by_id_with_relationship_hook_mode` is `pub(crate)`. A move
  can still retarget a source without its hooks firing: a `get_mut_by_id` write is a mutation,
  and mutations fire no hooks.

## 2. Options for the target list

The design (openspec/changes/custom-relationships/design.md) supersedes points 4 and 5 below: target lists are rebuilt on reload (decision 8), and the source is immutable, so a move retarget is a `get_mut_assume_mutable_by_id` write (decision 6).

The plain-bytes rule: a TS component is registered with `drop: None` and
`ComponentCloneBehavior::Default` (`crates/bevy-ts/src/components.rs:152-167`). JS sees its column
zero-copy, and hot reload migrates it by copying bytes into a new id (`components.rs:293-315`).
A `Vec<Entity>` breaks all three: it needs a drop, it cannot be bytewise copied or migrated, and
JS cannot read its column.

| Option | Drop | Clone | Hot reload | JS read | Verdict |
|---|---|---|---|---|---|
| A. Rust-owned `Vec<Entity>` per target id (Bevy's shape) | `drop_ptr::<Vec<Entity>>` | custom fn, as `clone_relationship_target` | layout is the same for every target, so the id is reused; never byte-migrated | packed per table before the run | Pick |
| B. Plain-bytes handle (`u32`) into a slab resource of lists | `on_remove` frees the slot | custom fn copies the slot | handle bytes migrate, slab is untouched | same as A | Keeps "plain bytes" in name only: the bytes mean nothing without the slab, and every path pays a second lookup |
| C. Fixed inline array `Array<Entity, N>` plus a count | none | bytewise | as today | zero-copy | A cap Bevy does not have; overflow has no good answer |
| D. Reflect tier (reflect-coverage stage 3's lazy host call per row) | — | — | — | 32-41 ns per row, plus JSON | Needs a reflected Rust type per TS target, which a dynamic one is not; slowest read |
| E. Decline: `Children` plus a TS `Entity` field | — | — | — | — | Lands nothing new, and no Bevy parity |

Pick A. Where it breaks the layout tier, and the minimal new pattern:

1. One new kind, `TsRelationshipTarget`. It is registered with
   `Layout::new::<Vec<Entity>>()`, `drop_ptr::<Vec<Entity>>`, `mutable: true`, a custom clone
   fn, and the `RelationshipTarget` accessor (the `iter` fn is a fixed fn over `Vec<Entity>`). It
   has no fields, no zero-copy column and no defaults. TS declares it with no body:
   `class LikedBy extends RelationshipTarget(Likes) {}`.
2. The source is an ordinary layout-tier component with one rule: `target: Entity` is at offset 0.
   Its accessor is `Relationship { entity_field_offset: 0 }`. A fixed offset lets the clone fn,
   which is a plain `fn` pointer with no captures and no world, map the entity without a lookup.
3. Hooks by id, on one bevy-ts dispatcher per slot. ecs-queries decision 11 takes `on_add`
   (required components). Relationships take source `on_insert`/`on_discard` and target
   `on_discard`/`on_despawn` (`on_despawn` only when `linkedSpawn`). Each hook reads a
   `ComponentId → role` table in a resource and runs Bevy's logic, ported, honouring
   `relationship_hook_mode`. Register hooks when the id is new, since they cannot change later.
4. Hot reload:
   - A change of role (plain, source, target, or a different counterpart or `linkedSpawn`)
     counts as a layout change, so the component gets a fresh id with its own hooks.
   - Byte migration of a source inserts the new id, then removes the old one. Because `add`
     pushes and `remove` drops one match, the target's list nets out right.
   - A target is never byte-copied. When its id changes, the dispatcher skips while a flag is set,
     the old target is dropped, and the lists are rebuilt from a scan of the sources.
5. Clone. A finding beyond relationships: today no TS component clones at all. `Default`
   resolves to `component_clone_via_reflect`, which returns early when there is no reflect data
   (`component/clone.rs:85-92`). So ecs-queries' `cloneAndSpawn` would silently drop every TS
   component. Fix it for all TS components with a bytewise custom clone fn
   (`write_target_component_ptr`). The source's version also maps offset 0 through
   `ctx.entity_mapper()`. The target's version is `clone_relationship_target` over the id, with
   the move retarget done by a `get_mut_by_id` write.
6. Scene save: levels do not save targets. A target is derived, and the source hooks rebuild it,
   as Bevy rebuilds `Children` from `ChildOf`. Levels already refuse `Entity` fields
   (`bevy-ts-levels/src/compose.rs:943`), so a relationship cannot be authored in a level yet.
   That is a follow-up, not part of this change.
7. Reads: the target is read-only query data, which is Bevy's rule too. For a query naming a
   target, `systems.rs` fills, per matched table, a `u32` offsets array of `len + 1` and a flat
   array of (index, generation) pairs. It passes them in the frame where a column would go, and
   JS hands each row a `subarray`. The same packer serves Bevy's own `Children` through its
   relationship accessor. That is a second route to reflect-coverage stage 3's `Children` read;
   tell lead@reflect-enums-color.

## 3. Host-call cost per row

A prototype bench, not landed. A `deno_core` 0.412 binary with a fast op shaped like
`op_bevyts_host` (id, args, ret, four slices). Each run sums the first word of every target entity.
Median of 400 runs after 200 warm-up runs, Apple M5 Max, load average about 20 (other agents
building), two passes per shape. The pass-to-pass spread is ±10%.

| Rows × entities per row | Host call per row | Packed: pack + JS read | of which pack | Inline array (floor) |
|---|---|---|---|---|
| 1,000 × 4 | 35-38 ns/row (35 µs) | 9.5-10.3 ns/row | 5.1-5.5 | 2.6-2.8 |
| 10,000 × 1 | 32-37 ns/row (324-366 µs) | 4.5-4.7 ns/row | 2.2-2.8 | 0.7 |
| 10,000 × 4 | 35-36 ns/row (352-357 µs) | 8.6-8.9 ns/row | 4.1-4.2 | 2.0-2.2 |
| 10,000 × 16 | 40-41 ns/row (398-414 µs) | 20.5-21.7 ns/row | 7.8-7.9 | 13.9-14.9 |
| 100,000 × 4 | 32-35 ns/row (3.2-3.5 ms) | 7.8 ns/row (0.78 ms) | 3.9-4.7 | 2.7-3.2 |

- A host call costs about 30 ns of crossing per row, whatever the list length. The bench's Rust
  side is a `Vec` index. A real call also resolves entity to table row to column, so expect more.
- Packing costs about 1 ns per entity in Rust, and the JS read is as fast as an inline array.
  Packed reads are 2-7× cheaper than a host call per row.
- A lazy host call (reflect-coverage stage 3's pattern) wins only when a system reads well under a
  quarter of its rows. A query that names `LikedBy` as data reads most of them. Keep packing.

## 4. Ask-first

- New architectural pattern: a second TS component kind holding Rust-owned, non-plain data
  (drop, custom clone, no column); bevy-ts-owned relationship hooks by id; the role table and
  rebuild on hot reload. The operator must approve it, as ecs-queries already flagged.
- Performance: packing adds per-run work for queries that name a target, about 1 ns per entity
  plus 4 ns per row. Queries without one are unchanged. Land it with a bench line.
- Hook ordering: TS components get Bevy hooks for the first time, so a hook bug panics inside a
  structural change. Port Bevy's hook code closely and test every hook mode.
- The TS clone fix (point 5) changes behaviour for every TS component under `cloneAndSpawn`,
  which is a bug fix. It belongs with whichever lands first, this change or ecs-queries.
- New dependency or infrastructure: none.

## Sources

- `bevy_ecs` 0.20.0: `relationship/mod.rs` (hooks, clone, accessor),
  `relationship/relationship_source_collection.rs:151-163`, `component/info.rs:264-327`,
  `component/clone.rs:35-92`, `entity/clone_entities.rs:205-235`, `world/mod.rs:345-365`,
  `lifecycle.rs:88-97, 180-320`.
- bevy-ts: `crates/bevy-ts/src/components.rs:152-167, 281-325`, `crates/bevy-ts-v8/src/lib.rs:47-82`,
  `crates/bevy-ts-levels/src/compose.rs:943`.
- `git show 91899ab:openspec/changes/ecs-queries/design.md` decisions 11-12; branch `ecs-queries`
  `docs/research/ecs-queries.md` "Hierarchy and relationships"; branch `reflect-enums-color`
  `openspec/changes/reflect-coverage/design.md` decision 7.
