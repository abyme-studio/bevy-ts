## 1. Slots

- [x] 1.1 `core.js`: the accessor's row, tick, tick column, typed views and nested views keyed by symbols, in the generated classes and in every place that binds them
- [x] 1.2 A query row's `present` and `On`'s `stopped` and `bind` keyed by symbols
- [x] 1.3 `packages/bevy-ts/runtime/core.js` regenerated

## 2. Proof

- [x] 2.1 Test: a component, a resource, a `Local`, a message and an event with fields named after every slot, read and written
- [x] 2.2 bob bench, before and after, 3 runs per arm (within noise; lead ruled 8 per arm not worth the machine under load)
