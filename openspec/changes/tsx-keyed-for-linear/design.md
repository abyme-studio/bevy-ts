## Context

`<For>` is a slot. JS keeps one owner per item key (`Slot.groups`) and, on each run of `Slot.for` (`crates/bevy-ts-v8/js/jsx-runtime.js:1266`), mounts the keys it lacks and unmounts the keys it no longer sees. Each mount is one entry `[slot, key, nodes]` in the host call's response.

Rust applies the response in `host::apply` (`crates/bevy-ts-tsx/src/host.rs:361-366`): one `slot_op` per unmount, then one per mount. Each `slot_op` checks and resolves its nodes, then calls `apply_root(..., Prune::Under(prefix))` with the prefix `[slot path, key]`.

`apply_root` (`crates/bevy-ts-tsx/src/reconcile.rs:140-193`) takes the root's whole `state.nodes` minus the nodes it just saw as despawn candidates (line 165). For each candidate it looks up the entity, clones its key path (line 175) and tests the prefix. Mounting group k walks the k−1 groups before it. N groups cost N²/2 key-path clones. 10,000 rows of the probe are 10,000 groups of one node: 5·10⁷ clones of a 4-step `Vec<String>`. That is the 23.9 s.

Unmounting all N items has the same shape: N `slot_op`s, each walking the root.

`commands.spawn` mounts go through `mount::mount` with `Prune::None` (`crates/bevy-ts-tsx/src/mount.rs:65`), which takes no candidates. That is why the same rows take 0.31 s that way.

Order: `Slot.for` never reorders. A key already mounted is skipped, so a list set to a new order keeps the entities in mount order. The reconciler has `order_children` for scene nodes (`reconcile.rs:539`) but a slot's groups never reach it.

Spawn inserts `ChildOf`, then each component, then `TsxNode`, each its own `insert` (`reconcile.rs:546-566`). With k components an entity moves archetype k+2 times. That is linear, but a large constant at 100k.

## Goals / Non-Goals

**Goals:**
- One `<For>` change costs time linear in what changed.
- Item order is entity order.
- A bench that shows it at 1k, 10k and 100k, against the 23.9 s baseline.

**Non-Goals:**
- A mount quota per frame. That is Heimdall's (scale.md, stage 3), built on this.
- Changing how `commands.spawn` or `spawnNative` work.
- BSN template spawning. The scene-reconciler spec allows direct insertion as its fallback; this stays on it.
- Faster resolution of component elements (`docs/research/tsx-scale.md`, "What is left"). Measured separately if the bench shows it is next.

## Decisions

1. **One reconcile per slot per host call.** `host::apply` groups the response's unmounts and mounts by slot, keeping their order, and calls one `slot_apply(world, slot, unmounts, mounts)`. It checks every group's nodes, resolves them, and makes one `apply_root` call with all groups' nodes. A group that fails its check logs its error and is left out; the rest apply, as today.
   - Rust already applies a call's unmounts before its mounts, and JS withdraws a pending mount rather than unmount it (`jsx-runtime.js:1250`). Batching keeps both rules.

2. **Prune by the groups given, not by the root.** A new `Prune::Groups { prefix, keys }`: candidates are the rows under `prefix + [key]` for each key given, found from an index, not from `state.nodes`.
   - The index: `SlotState` gains `rows: HashMap<String, Vec<u64>>`, the node ids each group mounted. `slot_apply` replaces a group's list when it mounts and takes it when it unmounts.
   - Candidates for a remount are the group's old ids minus the ids just seen. An unmount's candidates are all its old ids.
   - No key path is cloned on this path. Cost is the size of the groups touched.
   - Considered: keeping `Prune::Under` and indexing `state.nodes` by key-path prefix (a trie). Rejected: more code, and the slot already knows its groups.

3. **Order once per slot per call.** JS sends the slot's key order with the call when it changed: `Slot.for` compares the new key list with the last one it sent (one pass) and, if they differ, adds `[slot, keys]` to a new `orders` field of the response. Rust sorts the slot's group entities among the parent's `Children` by that list in one `sort_by_key`, leaving siblings outside the slot where they are, and pushes the list to the journal's `ordered`.
   - No LIS. In the DOM each move is a call with a cost, so Solid and ivi find the fewest. In Bevy, `Children` is one `Vec`; a sort is one write and one change tick whatever moved. Finding the fewest moves buys nothing.
   - A slot at a scene's top level (no parent entity) has no `Children` to sort. Its order is kept in the root's `order` map only, as top-level scene nodes are today.

4. **Insert a node's components in one step.** `spawn` builds the `ChildOf`, the native-layout components' bytes and `TsxNode` first, then makes one `EntityWorldMut::insert_by_ids` call (Bevy's API for a dynamic bundle), so the entity moves archetype once for them. Reflect-tier components and `<mesh>`/`<material>` intrinsics still insert after, as today: they go through reflection and assets.
   - A new helper beside `bevy_ts::commands::insert_bytes` takes several ids and byte buffers. It keeps each buffer 16-aligned, as `insert_bytes` does.

5. **JS: no scan per unmount.** `Slot.unmount` finds a pending mount by a flag on its owner (`owner.mount.sent`) rather than `ctx.mounts.indexOf` (`jsx-runtime.js:1253`). A withdrawn mount is marked and skipped when the response is built.

6. **Tests count work, the bench times it.** Per docs/testing.md rule 2, no test waits on or asserts a duration.
   - Rust, `crates/bevy-ts-tsx/tests/it/slot.rs`: drive `slot_apply` with hand-made group descriptions (no V8). Assert one reconcile per call (`TsxTree::reconciles`), and a new `ReconcileStats::checked` (despawn candidates examined) equal to the groups' node count, at 10 and at 1,000 existing groups. Also: reversed order keeps entity ids and sorts `Children`; a remount that drops a child despawns only it.
   - JS, `crates/bevy-ts-v8/js/test/reactive.test.js`: a reversed list sends one `orders` entry and no mount or unmount; an unchanged list sends none; an unmount of a pending mount withdraws it.
   - The existing `tsx_reactive.rs` For test (`crates/bevy-ts-cli/tests/it/tsx_reactive.rs:159-167`) stays as the one engine test.

7. **The bench lives in `bench/tsx-for/`** (docs/testing.md rule 8). A small project and a Node driver, in the shape of the heimdall-scale probe (`/tmp/heimdall-scale-probe`): a headless engine from this worktree's release build, one fresh run per arm, 3 runs an arm. Arms, each at 1k, 10k and 100k: grow 0→N in one set; grow by 1k a frame; reverse; clear. `commands.spawn` of the same N rows is the reference. It writes `bench/tsx-for/results/` and the summary goes in `docs/research/tsx-scale.md`.
   - Target: 10k grow under 1 s (from 23.9 s), within 3× of `commands.spawn`; 100k within about 10× of 10k.

## Risks / Trade-offs

- [The per-group index can drift from the tree when a system despawns a slot's entity] → the index holds ids, and every candidate is checked with `alive` before use, as today. A stale id is dropped from the list.
- [Batching changes when an error stops a group] → today a bad group logs and is skipped; batched, it still is. The test covers one bad group among good ones.
- [Hot path, performance-sensitive] → lands only with bench numbers, before and after, at all three sizes. This is a fix in the existing reconciler: no new pattern, no new dependency, nothing on the ask-first list.
- [Shared-machine noise] → the bench reports the load average with each run, as the probe does, and the targets are ratios.

## Migration Plan

None. No API or file format changes. Revert is one commit.
