## Why

A keyed `<For>` that goes from 0 to 10,000 items takes 23.9 s in one frame (heimdall-scale probe, npm alpha.20, 3 runs). At 1,000 items it takes 0.25 s. The same 10,000 rows mounted with `commands.spawn` take 0.31 s. So the cost is quadratic, and it is in `<For>`, not in spawning.

The cause is in Rust. Each `<For>` item is one group, and the host call applies each group alone (`crates/bevy-ts-tsx/src/host.rs:364`, `slot_op`). Each apply calls `apply_root` with `Prune::Under(prefix)`, which takes every node of the whole root not seen in this group as a candidate to despawn (`crates/bevy-ts-tsx/src/reconcile.rs:163-186`), and clones each candidate's key path to check it. Group k walks the k−1 groups mounted before it. Unmounting every item has the same shape.

Every row a world streams in goes through `<For>`. Heimdall's cut, and any list in a UI, waits on this.

## What Changes

- A host call's mounts and unmounts for one slot apply in one reconcile, not one per group.
- What a slot reconcile may despawn is found from the groups it was given, not from the whole root. Cost grows with the change, not with what is mounted.
- A spawned node's components go in with one archetype move, not one per component.
- `<For>` keeps entity order equal to item order. Today a reordered list mounts nothing and moves nothing, so its entities keep their first order. A changed order is one write of the parent's `Children` per slot per call.
- The JS side drops a linear scan per unmounted item (`ctx.mounts.indexOf`, `crates/bevy-ts-v8/js/jsx-runtime.js:1253`).
- A bench in `bench/tsx-for/` at 1k, 10k and 100k items, against the 23.9 s baseline.

No API change. `<For each key>` is written as today.

## Capabilities

### New Capabilities

None.

### Modified Capabilities

- `reactive-ui`: the control flow requirement gains item order and a cost bound for `<For>`.
- `scene-reconciler`: a new requirement that a slot's reconcile costs what it changes, not what the root holds.

## Impact

- Code: `crates/bevy-ts-tsx/src/host.rs` (`apply`, `slot_op`), `crates/bevy-ts-tsx/src/reconcile.rs` (`apply_root`, `spawn`, a slot-scoped prune), `crates/bevy-ts-v8/js/jsx-runtime.js` (`Slot.for`, `Slot.unmount`), and its generated copy `packages/bevy-ts/runtime/jsx-runtime.js`.
- Tests: `crates/bevy-ts-tsx/tests/it/` (Rust, work counted not timed), `crates/bevy-ts-v8/js/test/reactive.test.js` (JS diff and order).
- Bench: new `bench/tsx-for/`, outside the workspace.
- Docs: `docs/research/tsx-scale.md` gains the `<For>` numbers.
- Ask-first: none. A bug fix in the existing reconciler, no new pattern or dependency. It touches a hot path, so it lands with the bench numbers.
