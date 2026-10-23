## 1. Bubbling

- [x] 1.1 `crates/bevy-ts-tsx/src/picking.rs`: the observers queue bubbled events; each event carries `target` (the original target) beside `entity`
- [x] 1.2 `packages/bevy-ts/jsx-runtime.d.ts`: `PointerEvent.target`, and the doc comment says events bubble

## 2. Tests

- [x] 2.1 `crates/bevy-ts-cli/tests/tsx_reactive.rs`: a click on a child runs its parent's handler with `entity` the parent and `target` the child
- [x] 2.2 A test that spawns `Fox.glb` under a TSX entity with `onPointerClick`, clicks one of its meshes, and sees the root's handler run with `target` that mesh

## 3. Docs

- [x] 3.1 The picking rule (bubbling, `target`, no stopping from TS) in `docs/picking.md`, and a scene root taking clicks for its scene in `docs/animation.md`

## 4. Gate

- [ ] 4.1 The touched crates' tests, clippy, `pnpm check`, `openspec validate scene-pick --strict`
