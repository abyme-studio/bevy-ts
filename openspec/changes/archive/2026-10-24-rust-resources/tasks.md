## 1. Registry knows resources (`layout.rs`, `layout_reflect.rs`, `bevy-ts-native`)

- [x] 1.1 Add `resource: bool` to `NativeComponent` and `ReflectTier`; set it in `layout_reflect::tier` from `ReflectResource`
- [x] 1.2 Add `ts_resource_layout::<T: NativeLayout + Resource>()` to `BevyTsNativeExt`, registering with `resource: true`
- [x] 1.3 Unit tests: `GlobalAmbientLight` and `UiScale` are layout tier with `resource`; `Transform` is not a resource; `ClearColor` is reflect tier with `resource`

## 2. Resolution and refusals (`systems.rs`)

- [x] 2.1 In `build_one`, after the `Time` route and `ts(resource)`, resolve through `NativeLayouts` (path or unambiguous short name) into `RunParam::Res`
- [x] 2.2 Refusals in design 3's order, each naming the system, param and resource: reflect tier with reason, component not resource, `ResMut` of an immutable type, ambiguous short name with paths, unknown
- [x] 2.3 `RunParam::Res` gains `optional`; absent and optional gives `ParamFrame::None` with no failure; absent and required keeps today's error

## 3. `Option<Res<R>>` (reader, JS, types)

- [x] 3.1 `reader.rs`: `Option<Res<R>>` and `Option<ResMut<R>>` as params, emitting `optional: true`; `Option<...>` of any other param fails with its location; reader unit test
- [x] 3.2 `core.js`: pass `optional` through `resolveParams`; an optional res param with no frame is `undefined`
- [x] 3.3 `index.d.ts`: brand `Res<T>`/`ResMut<T>`; `Option<T>` is `T | undefined` for a branded `T`, the query-data marker otherwise; a `tsc` fixture type-checks `Option<Res<Score>>` as `ReadView<Score> | undefined` and `Query<[Option<Mut<Pos>>]>` as before

## 4. Classes (`core.js`, `types.rs`)

- [x] 4.1 `core.js`: one private native-class factory; export `nativeComponent` and `nativeResource` (`KIND = "resource"`); sync `packages/bevy-ts/runtime/core.js`; declare `nativeResource` in `index.d.ts`
- [x] 4.2 `types.rs` `emit_bevy`: stop skipping layout-tier resources; emit `nativeResource` classes and `.d.ts` classes extending `Resource` with the hole and read-only notes; keep `Time` re-exported from `bevy-ts`
- [x] 4.3 Run `scripts/gen-bevy-types.sh`; commit the regenerated files; list any renamed export in the commit body; `--check` passes

## 5. Tests and docs

- [x] 5.1 `crates/bevy-ts-cli/tests`: `ResMut<GlobalAmbientLight>` write seen by a Rust system the same frame with `is_changed()`, not changed the next frame; `Res<UiScale>` reads without moving the tick; each refusal; absent `UiScale` fails the run, `Option<Res<UiScale>>` reads `undefined`; an optional TS resource inserted later; a `ts_resource_layout` resource from a native fixture crate
- [x] 5.2 `docs/native.md`: "Rust resources" section (using one, the refusals, `Option<Res<R>>`, `runIf(resourceExists(R))`) and the coverage table from the research, tiers taken from the generated `bevy.d.ts`, reflect rows pointing at the reflect docs, dependents' rows (`AccumulatedMouseMotion`, `AccumulatedMouseScroll`, `MouseScrollPixelsPerLine`, `Time<..>`) naming their owners
- [x] 5.3 `cargo test` for `bevy-ts`, `bevy-ts-cli`, `bevy-ts-native`; package tests; `scripts/gen-bevy-types.sh --check`

## 6. Landing

- [ ] 6.1 On landing, tell lead@script-input-devices and research@script-time (and build-reads@reflect-enums-color, who rebase reflect-tier Res onto it) that `Res<T>` of Bevy's layout-tier resources is on main, with the commit
