## 1. Engine

- [ ] 1.1 One helper gives a row's nearest ancestor row; `tsx_tree` rows carry `root` and `parent` from it
- [ ] 1.2 `Change` gains the `(root, parent)` lists that changed; `reconcile` compares each touched list's child order before and after, old and new parent of a reparent included
- [ ] 1.3 `removed` lists the whole subtree of a removed node
- [ ] 1.4 Watch messages carry `ordered`, built from the live tree at send time, skipping lists whose parent is gone; an order-only reconcile sends a message

## 2. Protocol

- [ ] 2.1 `protocol/bevy-ts.openrpc.json`: `root`, `parent` on `TsxTreeNode`, `ordered` on `TsxTreeWatchMessage`, `removed` described as whole subtrees; bump to the next free minor at landing
- [ ] 2.2 Regenerate Rust and TS types (`scripts/gen-protocol.sh`)

## 3. Tests and docs

- [ ] 3.1 `crates/bevy-ts-cli/tests/levels_protocol.rs`: the spec's four scenarios (move in file, add between siblings, reparent, remove a parent) through `tsx_tree+watch`, and a client applying the deltas equals `tsx_tree`
- [ ] 3.2 `docs/levels.md`: the tree paragraph says watch deltas carry `parent` and `ordered`
