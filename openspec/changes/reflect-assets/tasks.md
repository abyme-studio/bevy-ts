## 0. Before starting

- [ ] 0.1 reflect-enums-color and reflect-coverage stage 1 have landed (`reflect_value::read`, `UNCOVERED`, the hole check with `ReflectAsset` in its filter); rebase on main
- [ ] 0.2 The `HandleSlots` hook (design decision 4) and the `Handle<T>` TS type are in `reflect_value` and `types.rs`, from reflect-enums-color and reflect-coverage; ask through lead@reflect-assets if not

## 1. Stage 1: assets.add / get / set

- [ ] 1.1 `crates/bevy-ts/src/reflect_asset.rs`: asset lookup by type path; `add` (default, write, `FromReflect` check, add); `get` (`read`); `set` (clone, write, compare, `get_mut` only on a difference)
- [ ] 1.2 `ScriptAssets` ref table: strong handles by ref number and by id; cleared on reload and stop; "stale asset ref" errors
- [ ] 1.3 Implement `HandleSlots` for `{ $asset }` (typed through `ReflectHandle`, wrong type named) and reads of table handles as refs; pass it from scripts and from `bevy-ts-tsx`'s `reflected.rs`, in check mode for `reflected::check` and the dry runs (form, live ref, asset type; builds nothing); a test that a valid ref passes TSX's check and one with no hook fails naming the handle type
- [ ] 1.4 Host calls in `host.rs`; `assets` in `crates/bevy-ts-v8/js/core.js` and `packages/bevy-ts/runtime/core.js`; throws outside a system run or TSX render
- [ ] 1.5 `types.rs`: `ReflectAsset` registrations in `named()` as `asset(...)` classes typed `AssetClass<{ fields }>`; `AssetRef<T>`, `AssetValue<T>` and `assets` in `packages/bevy-ts/index.d.ts`; regenerate `bevy.d.ts`/`bevy.js` with `scripts/gen-bevy-types.sh`; `tests/typescript.rs` cases for typed fields and handle slots
- [ ] 1.6 Unit tests in `reflect_asset.rs`: add with a bad field adds nothing; a no-op set sends no `Modified`; `Mesh` refused
- [ ] 1.7 `crates/bevy-ts-cli/tests/reflect_assets.rs`: the stage 1 scenarios of `reflected-assets` (add, set, ref in `MeshMaterial3d`, texture inside a material, wrong type, stale ref)
- [ ] 1.8 Delete each `UNCOVERED` entry marked `reflect-assets:` that stage 1 covers; the hole check passes
- [ ] 1.9 Protocol: regenerate `protocol/bevy-ts.openrpc.json` if it changed; bump to the next free minor at landing
- [ ] 1.10 `docs/assets.md` (the API, refs, assets kept across hot reloads and removed on restart or stop, `get` cost); asset rows in `docs/reflection.md`; link from `README.md`

## 2. Stage 2: asset values; <material> on the generic path

- [ ] 2.1 material-alpha and standard-material-fields have landed; rebase on main
- [ ] 2.2 `.of(props)` on asset classes; `{ $of }` in `HandleSlots` with the canonical-JSON cache feeding `ScriptAssets`
- [ ] 2.3 `intrinsics.rs`: replace `MaterialProps`' serde fields with the prop table (design decision 8), `alpha` and `alphaMode` included; `<material>` and `<mesh color>` build through `StandardMaterial.of`; texture loading, colour space and the tangent swap kept
- [ ] 2.4 Test that `jsx-runtime.d.ts`' `MaterialProps` matches the prop table
- [ ] 2.5 `standard_material.rs`, `standard_material_limits.rs`, `materials.rs` and material-alpha's tests pass with no edit; add the element-and-value sharing and value re-render scenarios
- [ ] 2.6 Delete stage 2's `reflect-assets:` `UNCOVERED` entries, if any; hole check passes
- [ ] 2.7 `docs/assets.md` (asset values, sharing, `set` on a shared value); `docs/materials.md` (`<material>` is a `StandardMaterial.of`)

## 3. Stage 3: Image sampler and loader settings

- [ ] 3.1 Hand-mapped `Image.sampler` in `get` and `set` through typed `Assets<Image>`, assigned only on a difference
- [ ] 3.2 Loader-settings table with one entry, `Image` → `ImageLoaderSettings` (`sampler`, `is_srgb`); `{ $load }` in `HandleSlots`; `Image.load` in the runtime and types; unknown keys and types without an entry fail; warning for one path with two settings
- [ ] 3.3 `<material>` texture props take refs and `Image.load` values; paths map to `Image.load` with the field's `is_srgb`
- [ ] 3.4 Tests: repeating sampler set and read back; linear data; a headless render of agent town's atlas case (scale 4, repeat) in `crates/bevy-ts-cli/tests/materials.rs`
- [ ] 3.5 Delete the `reflect-assets:` `UNCOVERED` entry for `Image`'s sampler; hole check passes
- [ ] 3.6 `docs/materials.md`: tiling textures; `docs/assets.md`: samplers and `Image.load`

## 4. Landing (each stage)

- [ ] 4.1 `cargo test` for touched crates through `~/dev/abyme/scripts/gate-slot` while working; workspace build, tests and clippy once before landing; `pnpm check`
- [ ] 4.2 `openspec validate reflect-assets --strict`
