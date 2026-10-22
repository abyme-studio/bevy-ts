## Why

A TS script cannot declare a relationship. Bevy 0.20 has them (`ChildOf`/`Children`, and any
`#[relationship]` pair a game defines), and ecs-queries moved them out to this change (its
design.md decision 12). A TS component today is plain bytes with no drop, so it cannot hold a
target's `Vec<Entity>`. Research (`docs/research/custom-relationships.md`) picked the shape and
measured the read cost.

## What Changes

- **Sources.** `class Likes extends Relationship { declare target: Entity }` declares a source.
  It is an ordinary layout-tier component whose first field is `target: Entity`, at offset 0. Other
  fields may follow. A source is immutable, as `ChildOf` is: a script retargets it by inserting
  it again.
- **Targets.** `class LikedBy extends RelationshipTarget(Likes) {}` declares a target, with
  `{ linkedSpawn: true }` as an option. A target is a new, second component kind: a Rust-owned
  `Vec<Entity>`, registered by id with a drop fn, a clone fn and Bevy 0.20's
  `RelationshipAccessor`. It has no fields and no column JS can see.
- **Hooks by id.** bevy-ts registers Bevy's relationship hooks on these ids: source `on_insert`
  and `on_discard`, target `on_discard`, and target `on_despawn` when `linkedSpawn`. One
  bevy-ts dispatcher per hook slot, looking up each id's role, shared with ecs-queries'
  required-components `on_add` (its decision 11). The hook logic is Bevy's, ported, and honours
  `RelationshipHookMode`.
- **Behaviour as Bevy's.** Inserting a source adds it to the target's list. Removing or replacing
  it removes it, and an empty target is removed. Removing a target removes its sources'
  relationship. Despawning a `linkedSpawn` target despawns its sources. A self-reference is
  refused with a warning.
- **Reads.** A query may name a target as read-only data: `Query<[LikedBy]>` gives each row a
  read-only list of `Entity`. Rust packs each matched table's lists into one offsets array and one
  entity array before the JS call. No host call per row. Writes go only through the source, as in
  Bevy.
- **Clone.** A target clones as Bevy's `clone_relationship_target` does. A source clones with
  ecs-queries' bytewise clone fn, plus mapping its `target` through the clone's entity mapper.
- **Hot reload.** A change of role (plain, source or target, the counterpart, `linkedSpawn`) is a
  layout change: the component gets a fresh id with its own hooks. A source migrates bytes as
  today. A target is never byte-copied: its lists are rebuilt from the sources.
- **Protocol.** The schema's TS extension gains a `relationship` entry, and a target's value reads
  as a list of entities. A minor bump: the next free minor at landing.
- **Out of scope:** authoring relationships in levels (levels refuse `Entity` fields today), a
  named follow-up; Bevy's `Children` as TS query data, which reflect-coverage stage 3 owns; the
  bytewise clone for every TS component, which ecs-queries ships with `cloneAndSpawn`.

## Capabilities

### New Capabilities

- `ts-relationships`: declaring sources and targets, the hooks and what they do, reading targets
  in queries, clone, despawn, and their hot reload.

### Modified Capabilities

- `ts-components`: registration of the target kind beside the plain-bytes kind.
- `hot-reload`: a role change is a layout change; target lists are rebuilt, never migrated.
- `remote-protocol`: the schema's `relationship` entry and a target's value.

## Impact

- `crates/bevy-ts`: `components.rs` (roles, the target kind, the role table, hook registration,
  hot-reload rebuild); a new `relationships.rs` (the ported hooks, clone fns, the packer);
  `systems.rs` (target query data in the frame); the shared hook dispatcher from ecs-queries.
- `crates/bevy-ts-cli/src/reader.rs`: `Relationship` and `RelationshipTarget(...)` in the compile
  step, and the offset-0 rule.
- `packages/bevy-ts`: `index.d.ts` and `runtime/core.js` (the two base classes, the list view).
- `protocol/bevy-ts.openrpc.json` and generated types; `crates/bevy-ts-remote` (schema, value).
- Tests: `crates/bevy-ts-cli/tests/relationships.rs`; a bench line for packed reads.
- Docs: `docs/scripts.md` (relationships), `docs/native.md` (reading a TS target from Rust).
- Depends on ecs-queries landing first: the hook dispatcher, `cloneAndSpawn` and its bytewise clone.
- Needs the operator's go:
  - **A new architectural pattern:** a second TS component kind holding Rust-owned, non-plain data
    (drop, custom clone, no column), bevy-ts-owned relationship hooks by id with a role table, and
    rebuilding target lists on hot reload. The alternative is to decline and use `Children` plus a
    TS `Entity` field.
  - **Performance:** a query that names a target pays about 4 ns per matched row plus 1 ns per
    entity in the lists, every run, for the packing. Queries that name none are unchanged.
  - **Hooks on TS components:** a bug in a hook panics inside a structural change. Mitigated by
    porting Bevy's code closely and testing every hook mode.
- No new dependency.
