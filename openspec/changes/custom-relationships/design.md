## Context

A TS component is registered in `crates/bevy-ts/src/components.rs` `register` with
`new_with_layout`, `drop: None`, `mutable: true` and `ComponentCloneBehavior::Default`. JS sees
its column zero-copy, and hot reload migrates it by copying bytes into a new id (`commit`).

Bevy 0.20 builds a relationship from two typed components and the hooks in
`relationship/mod.rs`. New in 0.20, `ComponentDescriptor::new_with_layout` takes a
`RelationshipAccessorInitializer`: metadata only (the source's entity offset, or the target's
`iter` fn, `linked_spawn`, the counterpart's id). It runs no hooks. Hooks by id come from
`World::register_component_hooks_by_id`, which panics once the id is in an archetype, and each
slot takes one hook.

ecs-queries (lands first) adds:
- decision 11: an `on_add` hook by id for required components;
- task 5.5: `cloneAndSpawn`, with a bytewise custom clone fn for every TS component (today
  `Default` resolves to reflect clone, which skips a component with no reflect data).

Research: `docs/research/custom-relationships.md` (options, Bevy's hook code, the read bench).

## Goals / Non-Goals

**Goals:**
- A TS script declares a relationship pair and gets Bevy's behaviour: lists kept by hooks,
  removal, `linkedSpawn` despawn, clone.
- A Rust system or Bevy's own tooling sees a TS pair as a relationship, through the accessor.
- Reading a target in a TS query costs no host call per row.
- Hot reload keeps every relationship.

**Non-Goals:**
- Levels authoring relationships. Levels refuse `Entity` fields (`compose.rs:943`). A named
  follow-up.
- Bevy's `Children` (or a Rust target) as TS query data. reflect-coverage stage 3 owns that read.
  The packer reads through the accessor, so it could serve them; that is reflect-coverage's call.
- One-to-one and set collections (Bevy's `Entity`, `EntityHashSet` targets). The target is an
  ordered `Vec<Entity>`, Bevy's default and `Children`'s.
- The bytewise clone for every TS component. ecs-queries ships it.
- Writing a target's list from TS. Bevy hands out no mutable list either.

## Decisions

1. **A source is a layout-tier component with `target: Entity` at offset 0.**
   `class Likes extends Relationship { declare target: Entity; since: f32 = 0 }`. The compile step
   refuses a `Relationship` class whose first field is not `target: Entity`. Layout rules put an
   8-aligned first field at 0, so the offset is fixed for every source. The accessor is
   `Relationship { entity_field_offset: 0, .. }`. `static allowSelfReferential = true` maps to
   Bevy's `allow_self_referential`. A fixed offset lets the source's clone fn, a plain `fn` with no
   world, map the entity without a lookup.

2. **A source is immutable**, as `ChildOf` is. It is registered with `mutable: false`. `Mut<Likes>`
   fails the load naming the class, and a protocol mutation of it re-inserts the value. An
   in-place write fires no hooks, so a mutable source would leave the target's list stale. To
   retarget, a script inserts the source again (`entity(e).insert(Likes.of({ target }))`), which
   fires `on_discard` then `on_insert`.

3. **A target is a second component kind.** `class LikedBy extends RelationshipTarget(Likes) {}`,
   `RelationshipTarget(Likes, { linkedSpawn: true })` for linked spawn. It is registered with
   `Layout::new::<Vec<Entity>>()`, `drop_ptr::<Vec<Entity>>`, `mutable: true` (Bevy writes the
   list through `collection_mut_risky`), table storage, a custom clone fn (decision 6) and the
   `RelationshipTarget` accessor whose `iter` is one fixed fn over `Vec<Entity>`. A body with
   fields fails the load. A source named by two targets, or a target naming a class that is not a
   `Relationship`, fails the load. A source no target names fails the load: Bevy's derive needs
   both halves too.
   Alternatives (research §2): a `u32` handle into a slab resource keeps "plain bytes" in name
   only and adds a lookup on every path; a fixed inline array adds a cap Bevy does not have; the
   reflect tier needs a Rust type per target.

4. **One dispatcher per hook slot, keyed by `ComponentId`.** A resource `TsHooks` holds
   `ComponentId → Role` (`Source { target }`, `Target { source, linked_spawn }`) beside
   ecs-queries' required-components entry. One `fn` per slot (`on_add`, `on_insert`,
   `on_discard`, `on_despawn`) reads the role for `HookContext.component_id` and runs each part
   that applies. Hooks are registered in `register`, while the id is fresh, for every slot the
   role uses: source `on_insert` and `on_discard`; target `on_discard`, and `on_despawn` when
   linked. This change takes ecs-queries' `on_add` registration into the same dispatcher, so one
   id never needs a second registration. A role change gets a new id (decision 8), so hooks never
   need to change on a live id.

5. **The hook bodies are Bevy's, ported.** Every hook first checks
   `HookContext.relationship_hook_mode` (`Run`, `RunIfNotLinked`, `Skip`).
   - source `on_insert`: a self-reference without `allowSelfReferential` warns and removes the
     source. Else a command does `entry(target).and_modify(push).or_insert(vec![source])`; a
     missing target removes the source.
   - source `on_discard`: removes one match of the source from the target's list at once; an
     empty list queues a command that removes the target if it is still empty.
   - target `on_discard`: `try_remove` of the source on every listed entity (a command).
   - target `on_despawn` (linked only): `try_despawn` of every listed entity.
   The ids come from the role, so the code works on `Ptr`s and `get_mut_by_id`, not types. A
   module of tests runs each Bevy relationship test against a TS pair.

6. **Clone.** A target's clone fn is `clone_relationship_target` over the id: the clone's list
   starts empty; with linked cloning and `linkedSpawn` each source is pushed and queued for
   cloning; on a move each source's `target` is retargeted in a deferred step, written with
   `get_mut_assume_mutable_by_id` (the source is immutable, decision 2) under
   `RelationshipHookMode::Skip` semantics: a raw write fires no hook. A source's clone fn is
   ecs-queries' bytewise clone, then offset 0 mapped through `ctx.entity_mapper()`.
   Dependency: ecs-queries' bytewise clone fn and `cloneAndSpawn` land first; this change reuses
   the fn and does not ship its own.

7. **Reads are packed per table before the JS call.** For a query naming a target as data,
   `systems.rs` fills, per matched table, a `u32` offsets array of `len + 1` and one entity array,
   in the encoding an `Entity` column uses, and puts both in the frame where a column would go. JS
   hands each row a read-only list view (`length`, index, iteration) over a `subarray`. The
   buffers are reused across runs, grown when short. `Option<LikedBy>` reads `undefined` on a row
   without one. Change detection on a target uses its own tick, which Bevy bumps when the list
   changes. Measured (research §3): 4-10 ns per row against 32-41 ns for a host call per row. Cost
   is paid only by queries that name a target.

8. **Hot reload: a role change is a layout change; target lists are rebuilt.** A component's
   role, counterpart, `linkedSpawn` and `allowSelfReferential` are part of its layout identity.
   A change gives it a new id with hooks for its new role, through `commit`'s `Planned::New`.
   When either half of a pair gets a new id in a generation, `commit`:
   1. sets `TsHooks.suspended` for the pair, so the dispatcher skips its hooks;
   2. migrates sources by field name as any layout change does, and removes every old target component (its drop runs);
   3. clears the flag, then rebuilds: for each entity holding the source, push it onto its
      target's list under the new target id, inserting the target where missing.
   A target is never byte-copied. One rule for every case, rather than relying on push and
   remove netting out. A pair whose ids are reused keeps its lists untouched. A component that
   turns plain from source or target, or the reverse, takes the same path. An undeclared source
   or target keeps its data and hooks (hot-reload: undeclared components are kept).

9. **Scenes and levels do not save targets.** A target is derived, as `Children` is from
   `ChildOf`. Levels refuse `Entity` fields, so sources are not authored there yet.

10. **Protocol.** `TsSchemaExtension` gains an optional `relationship`:
    `{ role: "source" | "target", counterpart: string, linkedSpawn?: boolean,
    allowSelfReferential?: boolean }`. A target's `layout` is reported with no fields, and its
    value in `get_components` is a JSON array of entity ids. A mutation of a target is refused, as
    Bevy hands out no mutable list. Added fields only: a minor bump, the next free minor at
    landing.

## Risks / Trade-offs

- [A hook bug panics inside a structural change] → port Bevy's code line by line; test each hook
  in each `RelationshipHookMode`, with despawn, clone and move.
- [The dispatcher is shared with required components] → one registration per id per slot, made
  in `register`; a test registers a component that is both required-bearing and a source.
- [Immutable sources break a script that writes `target` in place] → the load fails with the
  re-insert form in the message, before anything runs.
- [Rebuilding target lists on a reload costs a scan of the sources] → only when a pair's ids
  change, O(sources), once per reload.
- [List order after a rebuild may differ from before] → it follows source iteration order. Bevy
  promises no order for a non-ordered relationship either; documented.
- [`get_mut_assume_mutable_by_id` on an immutable source] → used only inside the clone fn's move
  step, where Bevy does the same through a crate-private call; a test covers moving a target.

## Migration Plan

Additive. Existing TS components are unchanged except that they share the dispatcher with
ecs-queries. Rollback is reverting the change; no data format changes.

## Open Questions

None.
