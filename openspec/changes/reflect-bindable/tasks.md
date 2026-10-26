## 1. Bound reflect-tier props

- [x] 1.1 `crates/bevy-ts-cli/src/types.rs`: the enum branch of `emit_bevy` (and the `native.d.ts` emitter, if it writes enum `tsxProps` separately) types `value` as `Bindable<XValue>`; `hole_props` unchanged
- [x] 1.2 Regenerate `packages/bevy-ts/bevy.d.ts` (`scripts/gen-bevy-types.sh`) and `packages/bevy-ts/test/engine/native.d.ts`
- [x] 1.3 `crates/bevy-ts-tsx/src/resolve.rs`: comments that say reflect props cannot be bound now say only layout-tier holes refuse; `jsx-runtime.d.ts` `TsxProps` doc says the same
- [x] 1.4 tsc test in `packages/bevy-ts/test/reflect.ts`: `<Projection value={() => ...}>` passes; `<DirectionalLight color={() => "#fff"}>` is a `@ts-expect-error`
- [x] 1.5 Rust test in `crates/bevy-ts-cli/tests/it/reflect_values.rs`: a `<Projection value>` bound to a signal a system sets is `Perspective`, then `Orthographic`, then `Perspective`; a bound `{ Isometric: {} }` errors naming the variants

## 2. GlobalTransform reads

- [x] 2.1 `packages/bevy-ts/index.d.ts`: `GlobalTransform` class with the methods in ts-systems "GlobalTransform reads as Bevy's GlobalTransform"; `Mut<GlobalTransform>` stays a type error
- [x] 2.2 `packages/bevy-ts/runtime/core.js` and `crates/bevy-ts-v8/js/core.js`: `GlobalTransform` reflect class whose read rows (query data, `Ref`, `Option`, `ref.get`) wrap the cached `value` in the method view; `HOST.transform` still takes the row
- [x] 2.3 `crates/bevy-ts-cli/src/types.rs` `emit_bevy`: skip `GlobalTransform`, re-export it from `bevy-ts` with `Time` and `Transform`; regenerate `bevy.d.ts` and `runtime/bevy.js`; fix `packages/bevy-ts/test/systems.ts`'s import if it moves
- [x] 2.4 Read test in `crates/bevy-ts-cli/tests/it/reflect_systems.rs`: a rotated, scaled parent and a child; a TS system after `bevy:transform-propagate` logs every method's result; a Rust system logs Bevy's; equal within 1e-5
- [x] 2.5 `crates/bevy-ts-cli/tests/it/schedules.rs` `after_propagation_reads_this_frames_global_transform`: drop `#[ignore]` and its stale doc line, read `g.translation().x`
- [x] 2.6 tsc test in `packages/bevy-ts/test/reflect.ts`: the methods' return types; `Mut<GlobalTransform>` stays a `@ts-expect-error`

## 3. Resources

- [x] 3.1 `crates/bevy-ts/src/systems.rs`: `reflect_term` checks `ReflectResource` before the storage type; `resolve_ref` checks for a resource before `sparse()` for an untiered id too
- [x] 3.2 Test in `crates/bevy-ts-cli/tests/it/rust_resources.rs` `unusable_resources_fail_the_load_naming_the_system_and_resource`: a `Query<[ClearColor]>` case with `ClearColor` from `bevy-ts/bevy`, wanting `ClearColor is a resource, not a component; take Res<ClearColor>` and no "sparse"
- [x] 3.3 Test that `Res<ClearColor>` from `bevy-ts/bevy` loads and reads its value, unless `reflect_systems.rs` or `audio.rs` already covers a reflect-tier `Res` (then name that test in the matrix)

## 4. Docs and specs

- [x] 4.1 `docs/native.md` "Reflected rows in TS systems": `GlobalTransform`'s methods; TSX section: reflect-tier props bind, layout holes do not
- [x] 4.2 `docs/research/bevy-coverage.md`: rows "Reflect-tier TSX props bound to a function", "`GlobalTransform` read; `TransformHelper`" (`TransformHelper` stays missing), "Rust resources"; drop items 1 and 2 from "Open, ranked" and the three fixed lines from "Docs and specs that claim more than the code"
- [x] 4.3 `docs/research/script-2d.md:33`: reflect-path props bind; tell script-2d's lead that `openspec/changes/script-2d/design.md` says otherwise (their path, not ours)
- [ ] 4.4 Sync the deltas into `openspec/specs` at archive
