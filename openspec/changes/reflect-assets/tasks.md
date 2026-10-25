## 0. Before starting

- [x] 0.1 reflect-enums-color and reflect-coverage stage 1 have landed (`reflect_value::read`, `UNCOVERED`, the hole check with `ReflectAsset` in its filter); rebase on main
- [x] 0.2 The `HandleSlots` hook (design decision 4) and the `Handle<T>` TS type are in `reflect_value` and `types.rs`, from reflect-enums-color and reflect-coverage; ask through lead@reflect-assets if not

## 1. Stage 1: assets.add / get / set

- [x] 1.1 `crates/bevy-ts/src/reflect_asset.rs`: asset lookup by type path; `add` (default, write, `FromReflect` check, add); `get` (`read`); `set` (clone, write, compare, `get_mut` only on a difference)
- [x] 1.2 `ScriptAssets` ref table: strong handles by ref number and by id; cleared on reload and stop; "stale asset ref" errors
- [x] 1.3 Implement `HandleSlots` for `{ $asset }` (typed through `ReflectHandle`, wrong type named) and reads of table handles as refs; pass it from scripts and from `bevy-ts-tsx`'s `reflected.rs`, in check mode for `reflected::check` and the dry runs (form, live ref, asset type; builds nothing); a test that a valid ref passes TSX's check and one with no hook fails naming the handle type
- [x] 1.4 Host calls in `host.rs`; `assets` in `crates/bevy-ts-v8/js/core.js` and `packages/bevy-ts/runtime/core.js`; works during the script's load (`ScriptBackend::load_with`, `host::LoadHost`); throws outside a system run, a TSX render or the load (a timer callback)
- [x] 1.5 `types.rs`: `ReflectAsset` registrations in `named()` as `asset(...)` classes typed `AssetClass<{ fields }>`; `AssetRef<T>`, `AssetValue<T>` and `assets` in `packages/bevy-ts/index.d.ts`; regenerate `bevy.d.ts`/`bevy.js` with `scripts/gen-bevy-types.sh`; `tests/typescript.rs` cases for typed fields and handle slots
- [x] 1.6 Unit tests in `reflect_asset.rs`: add with a bad field adds nothing; a no-op set sends no `Modified`; `Mesh` refused
- [x] 1.7 `crates/bevy-ts-cli/tests/reflect_assets.rs`: the stage 1 scenarios of `reflected-assets` (add, set, ref in `MeshMaterial3d`, texture inside a material, wrong type, stale ref)
- [x] 1.8 Delete each `UNCOVERED` entry marked `reflect-assets:` that stage 1 covers; the hole check passes
- [x] 1.9 Protocol: regenerate `protocol/bevy-ts.openrpc.json` if it changed; bump to the next free minor at landing
- [x] 1.10 `docs/assets.md` (the API, refs, assets kept across hot reloads and removed on restart or stop, `get` cost); asset rows in `docs/reflection.md`; link from `README.md`
- [x] 1.11 The hand-mapped types list (design decision 9): whole-type entries `{ asset TypeId, read, write }`; `add`/`get`/`set` use an entry in place of the reflected path; `set` merges the patch over `read(current)` at the top level and queues nothing when the reads match; a test with a fixture entry; animation-control's `AnimationGraph` entry uses it

## 1a. The runtime hand map (after stage 1, before stage 2)

- [x] 1a.1 `app.ts_hand_map::<T: Asset>(read, write)` on `BevyTsAppExt`, typed over `T`, replacing the const `HAND_MAPS`; `T` needs no `Reflect`; lookup by `T::type_path()`, ahead of the registry; the flush inserts the built `T` into `Assets<T>` at the ref's id; restart and stop remove it
- [x] 1a.2 Move the entries onto it: the test fixture `Note` moves, now `Asset + TypePath` only. script-gizmos (`GizmoAsset`), material-params (`ScriptMaterial`) and animation-control (`AnimationGraph`, `AnimationClip`) register their own, in their own changes
- [x] 1a.3 Unit tests: add, get, set, `T.of` sharing, ref reads and checks, restart removal for a type with no `Reflect`
- [x] 1a.4 `docs/assets.md` (hand-mapped types); spec `reflected-assets` (Rust hand-maps asset types); design decision 9

## 2. Stage 2: asset values; <material> on the generic path

- [x] 2.1 material-alpha and standard-material-fields have landed; rebase on main
- [x] 2.2 `.of(props)` on asset classes; `{ $of }` in `HandleSlots` with the canonical-JSON cache feeding `ScriptAssets`
- [x] 2.3 `intrinsics.rs`: keep `MaterialProps`' serde fields as the parsed props and map them onto Bevy's fields by the prop table (design decision 8), `alpha` and `alphaMode` included; `<material>` and `<mesh color>` build through `StandardMaterial.of`; texture loading, colour space and the tangent swap kept
- [x] 2.4 Test that `jsx-runtime.d.ts`' `MaterialProps` matches the prop table
- [x] 2.5 `standard_material.rs`, `standard_material_limits.rs`, `materials.rs` and material-alpha's tests pass with no edit; add the element-and-value sharing and value re-render scenarios
- [x] 2.6 Delete stage 2's `reflect-assets:` `UNCOVERED` entries, if any; hole check passes (none: the only one, `Image`'s, is stage 3's)
- [x] 2.7 `docs/assets.md` (asset values, sharing, `set` on a shared value); `docs/materials.md` (`<material>` is a `StandardMaterial.of`)

## 3. Stage 3: Image sampler and loader settings

- [ ] 3.1 `Image`'s hand-mapped entry (`sampler` only, design decision 9): `read` gives `{ sampler }`, `write` returns the base `Image` with a written `ImageSampler`; set and read back
- [ ] 3.2 Loader-settings table with one entry, `Image` → `ImageLoaderSettings` (`sampler`, `is_srgb`); `{ $load }` in `HandleSlots`; `Image.load` in the runtime and types; unknown keys and types without an entry fail; warning for one path with two settings
- [ ] 3.3 `<material>` texture props take refs and `Image.load` values; paths map to `Image.load` with the field's `is_srgb`
- [ ] 3.4 Tests: repeating sampler set and read back; linear data; a headless render of agent town's atlas case (scale 4, repeat) in `crates/bevy-ts-cli/tests/materials.rs`
- [ ] 3.5 Delete the `reflect-assets:` `UNCOVERED` entry for `Image`'s sampler; hole check passes
- [ ] 3.6 `docs/materials.md`: tiling textures; `docs/assets.md`: samplers and `Image.load`

## 4. Landing (each stage)

- [ ] 4.1 `cargo test` for touched crates through `~/dev/abyme/scripts/gate-slot` while working; workspace build, tests and clippy once before landing; `pnpm check`
- [ ] 4.2 `openspec validate reflect-assets --strict`
