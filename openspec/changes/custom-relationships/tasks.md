## 0. Before

- [ ] 0.1 ecs-queries has landed on main (its hook-by-id `on_add` for required components, `cloneAndSpawn` and the bytewise TS clone fn), and this branch is rebased on it
- [ ] 0.2 The operator's go on the ask-first items in proposal.md is recorded

## 1. Shared hook dispatcher (`crates/bevy-ts/src/components.rs`)

- [ ] 1.1 Start from ecs-queries' required-components `on_add` registration: move it into a `TsHooks` resource holding `ComponentId → Role` (required parts, `Source`, `Target`) and a `suspended` set, with one dispatcher `fn` per slot (`on_add`, `on_insert`, `on_discard`, `on_despawn`)
- [ ] 1.2 `register` registers every slot the role uses, once, while the id is fresh; ecs-queries' required-components tests pass unchanged
- [ ] 1.3 Test: a component that has `static require` and is a source gets both behaviours from one registration

## 2. Compile step (`crates/bevy-ts-cli/src/reader.rs`, `packages/bevy-ts`)

- [ ] 2.1 `Relationship` and `RelationshipTarget(Source, opts?)` in `index.d.ts` and `runtime/core.js`; `static allowSelfReferential`
- [ ] 2.2 The reader records role, counterpart, `linkedSpawn`, `allowSelfReferential` in the manifest's component declaration
- [ ] 2.3 Load errors: first field not `target: Entity`; a target with fields; a target naming a non-`Relationship`; two targets for one source; a source no target names; `Mut<Source>` and `Mut<Target>` in a query

## 3. Registration (`crates/bevy-ts/src/components.rs`)

- [ ] 3.1 Sources: `mutable: false`, ecs-queries' bytewise clone fn wrapped to map offset 0 through `ctx.entity_mapper()`, the `Relationship` accessor at offset 0
- [ ] 3.2 Targets: `Layout::new::<Vec<Entity>>()`, `drop_ptr::<Vec<Entity>>`, mutable, the `RelationshipTarget` accessor with a fixed `iter` over `Vec<Entity>`, counterpart id and `linked_spawn`
- [ ] 3.3 Role, counterpart, `linkedSpawn` and `allowSelfReferential` join the layout identity, so a change plans `Planned::New`

## 4. Hooks and clone (`crates/bevy-ts/src/relationships.rs`)

- [ ] 4.1 Port Bevy 0.20's source `on_insert` and `on_discard` and target `on_discard` and `on_despawn` from `relationship/mod.rs`, by id, honouring `relationship_hook_mode` and `TsHooks.suspended`
- [ ] 4.2 Target clone fn: port `clone_relationship_target` by id; the move retarget written with `get_mut_assume_mutable_by_id`
- [ ] 4.3 Tests in `crates/bevy-ts-cli/tests/relationships.rs`, one per ts-relationships scenario: insert and remove, retarget, remove target, linked and unlinked despawn, self-reference with and without `allowSelfReferential`, a missing target, a Rust insert by id, a TSX insert, a protocol insert, clone of a source, linked clone and move of a target, each `RelationshipHookMode`

## 5. Reads (`crates/bevy-ts/src/systems.rs`, `runtime/core.js`)

- [ ] 5.1 A target as query data: per matched table, fill a reused `u32` offsets array (`len + 1`) and an entity array in the `Entity` column encoding; place them in the frame where a column goes
- [ ] 5.2 JS: a read-only list view per row (`length`, index, iterator) over `subarray`; `Option<Target>`; `With`/`Without`/`Changed`/`Added` on a target
- [ ] 5.3 Tests: a list of 0, 1 and many entries; a table with mixed rows; a list changed by a flush between two runs; `Changed<LikedBy>` after a source insert
- [ ] 5.4 Bench line (`--ignored`, release): 10,000 rows × 1, 4 and 16 entities, packed read against an inline array; record in `docs/research/custom-relationships.md`

## 6. Hot reload (`crates/bevy-ts/src/components.rs` `commit`)

- [ ] 6.1 When a pair gets a new id: suspend the pair's hooks, migrate sources by bytes, remove old targets, resume, rebuild lists from a scan of sources under the new target id
- [ ] 6.2 Tests, one per hot-reload scenario: a source gains a field, a target becomes linked, a plain component becomes a source; and a reload with no pair change leaves every list and its tick untouched

## 7. Protocol (`protocol/bevy-ts.openrpc.json`, `crates/bevy-ts-remote`)

- [ ] 7.1 `TsSchemaExtension.relationship`; a target's value as an array of entity ids; a target mutation refused; a source mutation re-inserts
- [ ] 7.2 Bump to the next free minor at landing; regenerate Rust and TS types; CI's generated-output check passes
- [ ] 7.3 Tests: the remote-protocol scenarios

## 8. Levels

- [ ] 8.1 Level and scene saves skip target components; test it

## 9. Docs

- [ ] 9.1 `docs/scripts.md`: declaring a pair, immutable sources, reading a target, linked spawn, clone, reload, no level authoring yet
- [ ] 9.2 `docs/native.md`: reading a TS target from Rust through `RelationshipAccessor`
