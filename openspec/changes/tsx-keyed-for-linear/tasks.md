All cargo runs go through gate-slot.

## 1. Baseline

- [x] 1.1 Build `bench/tsx-for/` (design decision 7): project, Node driver, arms grow-once, grow-1k-a-frame, reverse, clear and `commands.spawn`, at 1k, 10k and 100k, 3 runs each, load average per run
- [ ] 1.2 Run it on `main` and keep the results in `bench/tsx-for/results/` as the before numbers (expect about 24 s for 10k grow-once)

## 2. Rust: one reconcile per slot, pruned by group

- [x] 2.1 Add `rows: HashMap<String, Vec<u64>>` to `SlotState` (`crates/bevy-ts-tsx/src/tree.rs`) and `checked` to `ReconcileStats`
- [x] 2.2 Add `Prune::Groups { prefix, keys }` to `apply_root` (`crates/bevy-ts-tsx/src/reconcile.rs`): candidates from the groups' old ids, no key-path clone, each checked with `alive`
- [x] 2.3 Replace per-group `slot_op` with `slot_apply` in `crates/bevy-ts-tsx/src/host.rs`: group the response's unmounts and mounts by slot, check and resolve each group, skip a failing group with its error, one `apply_root` per slot
- [x] 2.4 Apply a slot's `orders` entry: sort the slot's group entities among the parent's `Children`, others in place; journal the list in `ordered`
- [x] 2.5 Insert a spawned node's `ChildOf`, native-layout components and `TsxNode` in one `insert_by_ids` (a multi-id helper beside `bevy_ts::commands::insert_bytes`)

## 3. JS: order and no scan

- [x] 3.1 In `Slot.for` (`crates/bevy-ts-v8/js/jsx-runtime.js`), send `[slot, keys]` in a new `orders` field when the key order differs from the last sent
- [x] 3.2 In `Slot.unmount`, withdraw a pending mount by a flag on its owner, not `ctx.mounts.indexOf`
- [x] 3.3 Regenerate `packages/bevy-ts/runtime/jsx-runtime.js` with `packages/bevy-ts/scripts/sync-runtime.mjs`

## 4. Tests (docs/testing.md)

- [ ] 4.1 `crates/bevy-ts-tsx/tests/it/slot.rs` (and `mod slot;` in `main.rs`): one reconcile per call; `checked` equals the given groups' nodes at 10 and at 1,000 existing groups; reversed order keeps ids and sorts `Children`; a remount dropping a child despawns only it; one bad group among good ones logs and the rest mount
- [ ] 4.2 `crates/bevy-ts-v8/js/test/reactive.test.js`: reversed list sends one `orders` entry and no mount or unmount; unchanged list sends none; unmount of a pending mount withdraws it
- [ ] 4.3 Keep `crates/bevy-ts-cli/tests/it/tsx_reactive.rs`'s For test green; add a reorder assertion there only if 4.1 cannot show `Children` order without the engine

## 5. Numbers and docs

- [ ] 5.1 Rerun the bench on the branch; check 10k grow-once under 1 s and within 3× of `commands.spawn`, 100k within about 10× of 10k
- [ ] 5.2 Add a `<For>` section to `docs/research/tsx-scale.md`: before and after at 1k, 10k, 100k, the cause, the bench command
- [x] 5.3 Say in `docs/ui.md` that `<For>` keeps item order
- [ ] 5.4 `openspec validate tsx-keyed-for-linear --strict`
