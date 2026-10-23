## Why

TS cannot make, read or change an asset. `res()` reaches only resources with a native layout, so
`Assets<T>` is out of reach, and `bevy.d.ts` names no asset type. The only script-made assets are
`<material>`'s hand-written serde fields and `<mesh>`'s shapes. Bevy 0.20 registers ten asset types
with `ReflectAsset`, and `reflect-enums-color` brings one writer for any reflected slot. With both,
every reflected asset can be built and edited from TS through one path, instead of one hand-written
intrinsic per asset type. A texture's sampler is also unreachable, so a scaled `uvTransform` clamps
and agent town's atlas cannot tile.

## What Changes

Three stages, each landable on its own, in this order.

1. **`assets.add` / `get` / `set` over `ReflectAsset`.** `assets.add(StandardMaterial, { base_color:
   "#f80" })` builds the asset from its default through `bevy_ts::reflect_value::write`, checks it
   with `FromReflect`, and adds it; it returns an `AssetRef`. `assets.get(ref)` reads it as JSON
   through `reflect_value::read`. `assets.set(ref, patch)` writes over the current value and borrows
   the asset mutably only when the value changes, so a no-op set sends no `AssetEvent::Modified`.
   An `AssetRef` goes in any `Handle<T>` slot: a TSX prop (`<MeshMaterial3d value={ref} />`), a
   field of another asset (`base_color_texture: img`), or a script's reflected insert.
   `bevy.d.ts` gains a class per reflected asset type with its fields typed, and `Handle<T>` fields
   type `string | AssetRef<T> | AssetValue<T> | null`. **BREAKING (wire):** a reflected handle to a
   script-made asset reads as its ref, not `null`; the protocol moves to the next free minor.
2. **Asset values in TSX; `<material>` on the generic path.** `StandardMaterial.of({ ... })` is an
   asset value: a plain description, not an element. Written into a `Handle<T>` slot, it becomes a
   handle shared by every slot given equal props, as `<material>` shares today.
   `<material>` keeps its prop names and value spellings, but its serde `MaterialProps` goes: each
   prop maps by a table onto a `StandardMaterial` field and is written by the generic path.
   standard-material-fields' texture-path loading, colour-space rule, lazy tangents, limits test
   and coverage tests stay. material-alpha's `alpha` and `alphaMode` props fold into the table; the
   `MaterialAlpha` component stays as it is.
3. **Image sampler and loader settings.** `Image` is `reflect(opaque)`, so its `sampler` is
   hand-mapped: `assets.get(img)` and `assets.set(img, { sampler })` reach it through typed
   `Assets<Image>` code, with the `ImageSampler` value built by the writer. A path with loader
   settings has one typed form, `Image.load(path, { sampler?, is_srgb? })`, an asset value loaded
   through `load_builder().with_settings::<ImageLoaderSettings>`. `<material>`'s texture props take
   it, and the colour-space rule moves onto it. Agent town's atlas tiles with `address_mode_u` and
   `address_mode_v` set to `"Repeat"`.

The hole check (reflect-coverage's `reflect_coverage.rs`) walks `ReflectAsset` types too; that
filter is added by reflect-enums-color. Asset fields not yet covered sit on
`reflect_value::UNCOVERED` marked `reflect-assets`. Each stage removes the entries it covers; this
change does not edit the walker.

**Out of scope:** building `Mesh` attributes and indices (script-assets); `AssetServer` load state,
handle lifecycle and load events (engine-core); assets Bevy does not reflect (`Font`, `Gltf`,
`AudioSource`, `Shader`, scenes); `ScriptMaterial` assets (`<material kind>` stays as it is);
binding asset props to signals.

## Capabilities

### New Capabilities

- `reflected-assets`: TS builds, reads and edits any `ReflectAsset` type; asset refs and asset
  values in `Handle<T>` slots; generated asset types; image samplers and loader settings.

### Modified Capabilities

- `standard-materials`: `<material>` writes through the generic asset path with its props
  unchanged; its texture props take an asset value, so a texture can tile.

## Impact

- Code: new `crates/bevy-ts/src/reflect_asset.rs` (asset lookup, add/get/set, the ref table, the
  value cache, the `Handle` hook, the image hand-map and loader table); `crates/bevy-ts/src/host.rs`
  (the asset host calls); `crates/bevy-ts-v8/js/core.js` and `packages/bevy-ts/runtime/core.js`
  (`assets`, asset classes, `.of`, `Image.load`); `crates/bevy-ts-tsx/src/intrinsics.rs`
  (`MaterialProps` replaced by a prop table); `crates/bevy-ts-tsx/src/reflected.rs` (passes the
  hook); `crates/bevy-ts-cli/src/types.rs` (asset classes, handle types); `packages/bevy-ts/index.d.ts`,
  `jsx-runtime.d.ts`; `bevy.d.ts` and `bevy.js` regenerated.
- Asked of reflect-enums-color (owner of `reflect_value`, `color`, the walker), through
  lead@reflect-assets: one hook in `reflect_value`'s `Handle` arm, read and write, for values that
  are not a path string; the TS type of a `Handle<T>` slot; `ReflectAsset` in the walker filter,
  with uncovered asset fields on `UNCOVERED` marked `reflect-assets` (settled).
- Tests: new `crates/bevy-ts-cli/tests/reflect_assets.rs`; unit tests in `reflect_asset.rs`;
  `crates/bevy-ts-tsx/tests/standard_material.rs`, `standard_material_limits.rs` and
  `crates/bevy-ts-cli/tests/materials.rs` stay green unchanged; `tests/typescript.rs` cases.
- Docs: new `docs/assets.md`; `docs/materials.md` (textures as values, tiling); asset rows in
  `docs/reflection.md`; a link from `README.md`.
- Protocol: stage 1 changes how a script-made handle reads; next free minor at landing.
- Depends on: reflect-enums-color and reflect-coverage stage 1 (`read`, `UNCOVERED`, the hole check)
  before stage 1; material-alpha and standard-material-fields before stage 2.
- No new dependency or service.
- Operator's go (ask-first), detail in design "Ask-first items":
  1. Asset values in TSX: the design spells them `StandardMaterial.of({...})`, a plain value, not a
     TSX element, so no element-as-value pattern and no reconciler change. If the element spelling
     `<StandardMaterial ... />` is wanted, that is the new pattern and needs the go.
  2. Asset values cached by their props' canonical JSON, built on insert and on a changed prop, never
     per frame. This is `<material>`'s cache today, made generic. Memory: one asset per distinct
     prop set, held until the script reloads or stops.
