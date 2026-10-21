## Why

`bevy-ts/tsx_tree` lists a level in file order (1.12.1), but `tsx_tree+watch` deltas carry no position. A move within a `.bsn` sends no message, and an added row says neither its root nor its parent nor its place. abyme's outliner re-reads the whole tree after every write to stay in order. With order in the delta, it can stop.

The shape was agreed with build@bevy-outliner (abyme), 2026-10-22.

## What Changes

- Every `tsx_tree` row (snapshot, `tsx_tree`, watch `added`) gains `root` and `parent`: the root's name, and the node id of its nearest ancestor row (null at the root's top).
- Watch messages gain `ordered: [{ root, parent, children }]`: the full child order of every sibling list that gained or lost a child, or changed order, in this message. A move within a `.bsn` now sends a message. A list whose parent was removed is not sent.
- `removed` lists every node gone, the whole subtree, not only its top.
- Protocol: the next free minor when it lands (1.13 and 1.14 are taken; planned 1.15). Additions only.
- Out of scope: abyme's outliner change (team bevy-outliner drops its re-read once this ships).

## Capabilities

### New Capabilities
None.

### Modified Capabilities
- `scene-editor-hooks`: the protocol requirement's `tsx_tree` rows and `tsx_tree+watch` messages carry position and order changes.

## Impact

- `crates/bevy-ts-tsx`: `tree.rs` (`Change` gains the sibling lists to send), `reconcile.rs` (record which lists changed, compare old and new order), `protocol.rs` (`root`, `parent`, `ordered`, whole-subtree `removed`).
- `protocol/bevy-ts.openrpc.json` and its generated Rust and TS types (`scripts/gen-protocol.sh`), version bump.
- `crates/bevy-ts-cli/tests`: a level move, an add and a reparent seen in the watch stream.
- `docs/levels.md`: the tree paragraph names `ordered`.
- Operator go: none needed. It is a protocol minor, additions only, on existing patterns.
