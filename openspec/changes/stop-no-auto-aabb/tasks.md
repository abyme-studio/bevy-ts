## 1. Fix

- [x] 1.1 `restore::changes` and `restore::write_back` count an `Aabb` change on a `NoAutoAabb` entity (`crates/bevy-ts-tsx/src/restore.rs`)

## 2. Tests

- [x] 2.1 `restore::bounds_on_a_no_auto_aabb_node_are_a_play_change`: red before 1.1, green after
- [x] 2.2 The other `restore::` tests stay green

## 3. Docs

- [x] 3.1 `docs/world-text.md`, the spec delta
