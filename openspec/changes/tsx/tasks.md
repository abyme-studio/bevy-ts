## 1. Walking skeleton: two Enemy cubes from TSX, reconciled twice (needs only bridge group 1)

- [x] 1.1 Add `bevy-ts/jsx-runtime` and `bevy-ts/jsx-dev-runtime` to the TS package: `jsx`, `jsxs`, `jsxDEV`, `Fragment` returning description nodes, prefab expansion, type-scoped keys; unit tests with no engine
- [x] 1.2 Spike: compile a `.tsx` with oxc 0.154 in development mode with `importSource: "bevy-ts"` and record where source lands (`jsxDEV` argument or `__source` prop); make the runtime read it
- [x] 1.3 Create `bevy-ts-tsx` with `TsxNode`, `TsxTree` and a mount-only Rust reconciler that spawns `<entity>` with `Transform` and a `<mesh shape="cuboid">` intrinsic from a JSON description
- [x] 1.4 In bridge's skeleton runtime, evaluate a precompiled `Enemy` scene and read its description from the manifest the backend's load returns; two cubes appear
- [x] 1.5 Load a second description (enemy `c` added, `a` removed, `b`'s `translation` changed) and diff by key path; test that only those three writes happen and `b` keeps its entity id

## 2. Bridge dependencies in place

- [x] 2.1 Confirm bridge has B1 (manifest sections) and B4 (TSX compilation); move the skeleton's manifest read onto a registered `tsx` section that can fail the generation
- [x] 2.2 Confirm B5 (layout and id lookup) and encode TS component props into native bytes in Rust
- [x] 2.3 Confirm B2 (host-invoked entries) and B3 (plugin command kinds); register the `tsx` entry and the `mount`/`unmount` opcodes
- [x] 2.4 Confirm B6 and B7; wire `restart` notification and the protocol method pipeline

## 3. Reconciler

- [x] 3.1 Spike: build a BSN `Scene` from runtime description data in Bevy 0.20; pick BSN spawning or direct insertion and record why in design.md
- [x] 3.2 Implement element kinds: `<entity>`, component elements (TS components and `Transform`, `Name`), fragments, `<mesh>` and `<material>`; reject misplaced and duplicate components with TS locations
- [x] 3.3 Implement the keyed diff: spawn, despawn with descendants, insert and remove of tsx-owned components, controlled field writes against last applied values
- [x] 3.4 Implement `initial*` and controlled typing from `component()` declarations; reject a field given both ways in `tsc` and at load
- [x] 3.5 Cache static subtrees by content hash; test that a second identical `<Enemy>` reuses the template
- [x] 3.6 Implement `app.scene` roots, unmount of roots missing from a generation, and `commands.mount`/`unmount` under a runtime root

## 4. Hot reload

- [x] 4.1 Reconcile every root on generation apply after migrations and before the first system; test that an identical generation writes nothing and moves no change tick
- [x] 4.2 Tombstone nodes despawned outside tsx; clear tombstones and the tree on `restart`
- [x] 4.3 Tests for each hot-reload rule in design.md: moved lines, renamed prefab, changed `initialHp`, changed `max`, system-inserted components, a throwing scene, a bad field name

## 5. Reactivity

- [x] 5.1 Write the reactive core in `bevy-ts` (`signal`, `computed`, `effect`, `untrack`, `onCleanup`, owners, `ref`) with unit tests
- [x] 5.2 Turn function props and children into bindings with ids; queue `patch` and `patchReflect` ops into the tsx op stream
- [x] 5.3 Implement `ref.get(C)` and `res(R)` reads inside bindings (no hooks, no `use*` exports): register a subscription per binding, check change ticks in `PostUpdate` in Rust, deliver changed values in one host call per frame, apply the returned patches before `Last`
- [x] 5.4 Test that a frame with no subscribed change makes zero host calls, and that a player hit writes exactly one `Text`
- [x] 5.5 Implement `<Show>` and `<For>` with subtree mount and unmount and owner disposal
- [x] 5.6 Add the dev warnings: a signal, `ref.get` or `res` read outside a tracking scope, unkeyed array siblings, a binding site whose component is in some TS system's `write` set (naming the system and the site)
- [x] 5.7 Group bindings on native-layout fields by site (design decision 11, pending the operator): Rust finds changed instances without JS (own components in table order, ref'd entities per pair, signals by version), one host call evaluates the site's closure per changed row through the row accessors and writes through the column view only when the value differs; dev error when a later run reads outside the first run's set; test that 10,000 instances of a bound prefab with 100 changed cost within 2× the equivalent TS system and that a quiet frame makes no host call

## 6. UI

- [x] 6.1 Implement `<node>` with `style` mapped to `Node` and the length helpers, plus `background`, `border`, `radius`
- [x] 6.2 Implement `<text>` (`Text`, `TextFont`, `TextColor`, bound children through `patchReflect`), `<image>` and `<button>`
- [x] 6.3 Observe `PointerClick|PointerPress|PointerRelease|PointerOver|PointerOut` on entities with `onPointer*` handlers (any `<entity>` or UI element), queue events, and deliver them in the same host call as subscriptions; test three clicks in one frame

## 7. Editor hooks

- [x] 7.1 Record source chains and prop provenance on `TsxNode`, updated on every reconcile
- [x] 7.2 Add `bevy-ts/tsx_tree`, `bevy-ts/tsx_node`, `bevy-ts/tsx_find` and `bevy-ts/tsx_tree+watch` to `protocol/bevy-ts.openrpc.json`, regenerate types, add conformance tests
- [x] 7.3 Test the round trip: rewrite a controlled prop in a `.tsx` file, save, and see the watch stream report the written node

## 8. First demo

- [ ] 8.1 Write `examples/arena/` (Enemy, Player, Hud, `damage` and `bob` systems) as in design.md
- [ ] 8.2 Walk the eight demo steps, recording reload latency and host calls per frame
- [x] 8.3 Measure mount and reload of 1,000 and 10,000 enemies, and HUD update cost, and record them in `docs/`
