## Why

The coverage audit (`docs/research/bevy-coverage.md` at 50340d9ef, "Open, ranked" 1 and 2, and "Docs and specs that claim more than the code") found four gaps a game team waits on. Agent town and script-2d bind `<Projection value>` to a signal: the runtime binds it, but tsc refuses it. Agent town reads `GlobalTransform` through BRP with its own helper because a TS row is 12 raw floats. A spec line says a reflect-tier Rust resource fails the load, which it no longer does. A Rust resource named as query data reports a sparse-set error instead of saying to take `Res<T>`.

## What Changes

- **Bound reflect-tier props type-check.** The enum component's `value` prop in `tsxProps` becomes `Bindable<…>` in the generator (`bevy-ts types --bevy`, `native.d.ts`) and the checked-in `bevy.d.ts`. The runtime already binds them (`crates/bevy-ts-tsx/src/resolve.rs`, `Target::Reflect`). Layout-tier holes stay values, as now.
- **`GlobalTransform` has Bevy's read API.** `bevy-ts` ships it typed, as `Transform`; its rows offer `translation()`, `rotation()`, `scale()`, `toScaleRotationTranslation()`, `computeTransform()`, `affine()`, `toMatrix()`, the six directions, `transformPoint`, `transformVector`, `reparentedTo` and `mulTransform`, computed in JS from the cached affine. `value` stays the 12 floats. The ignored after-propagation test runs.
- **Spec fix.** ts-systems "Resources": a reflect-tier Rust resource loads through its reflected handle; only the layout-class spelling fails.
- **A resource as query data says so.** The load error names the type as a resource and says to take `Res<T>`, for both tiers, before any sparse-set check.
- Coverage matrix rows, `docs/native.md`, and `docs/research/script-2d.md` follow.

No new dependency, infrastructure or pattern. Not on the ask-first list: a `GlobalTransform` read stays one cached reflected read; the methods are JS arithmetic on 12 floats, done only when called.

## Capabilities

### New Capabilities

None.

### Modified Capabilities

- `native-components`: reflect-tier TSX props take a binding.
- `ts-systems`: "Resources" corrected and gains the resource-as-query-data error; `GlobalTransform` reads as Bevy's.

## Impact

- `crates/bevy-ts-cli/src/types.rs` (enum `tsxProps`, `GlobalTransform` re-export), `packages/bevy-ts/bevy.d.ts`, `packages/bevy-ts/runtime/bevy.js`, `packages/bevy-ts/test/engine/native.d.ts` (regenerated).
- `packages/bevy-ts/index.d.ts`, `packages/bevy-ts/runtime/core.js`, `crates/bevy-ts-v8/js/core.js`: the `GlobalTransform` class and row view.
- `crates/bevy-ts/src/systems.rs`: `reflect_term` and `resolve_ref` check for a resource before a sparse set.
- `crates/bevy-ts-tsx/src/resolve.rs`: stale comments only.
- Tests: `packages/bevy-ts/test/reflect.ts`, `crates/bevy-ts-cli/tests/it/{reflect_values,reflect_systems,schedules,rust_resources}.rs`.
- Docs: `docs/native.md`, `docs/research/bevy-coverage.md`, `docs/research/script-2d.md`.
