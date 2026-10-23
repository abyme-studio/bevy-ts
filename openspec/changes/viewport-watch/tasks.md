## 1. Per-subscriber watches

- [x] 1.1 `Subscribers` in `bevy-ts-remote`: cursors per subscriber by serving order within an update, its own update counter, `turn`, `cursor`, `set`, `owed`
- [x] 1.2 viewport mode, selection, tools, edits and assets watches on it; `owed()` and the `Watchers` maps removed
- [x] 1.3 entities, get_components (state per subscriber slot), log and reload on it
- [x] 1.4 tsx_tree and level watches on it
- [x] 1.5 Tests: `Subscribers` unit tests; two tools subscribers with the same params, one late
- [x] 1.6 `level+watch` tests call once per update
- [x] 1.7 After a close, a slot is served the lowest cursor among it and the slots after it
- [x] 1.8 `assets+watch`: a new subscriber's poll keeps the open subscribers' changes; test

## 2. Engine entities

- [x] 2.1 `bevy-ts/entities` and `entities+watch` leave out `Window`, `Monitor`, `Gamepad` and `PointerId` entities
- [x] 2.2 Test: after a Stop neither the list nor the watch's snapshot has them, and a pointer spawned while the watch is open is not sent

## 3. Spec

- [x] 3.1 remote-protocol delta
