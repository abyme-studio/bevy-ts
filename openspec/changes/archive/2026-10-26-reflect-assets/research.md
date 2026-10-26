# reflect-assets: research

Findings for the reflect-assets change, 2026-10-23. Bevy pinned at 0.20.0. Registry paths are under
`~/.cargo/registry/src/index.crates.io-1949cf8c6b5b557f/`. Confidence: high where a line is cited,
marked otherwise.

## 1. The writer

- Main today: `crates/bevy-ts-tsx/src/reflected.rs` writes one top-level field of a reflect-tier
  component (`set` :26, `field_value` :15). A string in a direct `Handle<T>` field loads through
  `ReflectHandle` + `load_builder().load_erased` (:18-21). No enums, no merge.
- reflect-enums-color (22/22 tasks, not yet on main) moves this into `crates/bevy-ts/src/reflect_value.rs`:
  `write(registry, server: Option<&AssetServer>, slot: &mut dyn PartialReflect, base, json)` (:41).
  It handles Color (`bevy_ts::color`), `Handle` at any depth, `Option` (null = None), enums
  (`"Unit"` / `{ Variant: data }`), structs (unknown keys rejected), newtypes. `server = None` is a
  dry-run check.
- It takes no World, Entity or ReflectComponent. An asset slot from `ReflectAsset::get_mut` or a
  value from `default_of` writes as is. Nothing in the writer needs to change for assets.
- One owner: `bevy_ts::reflect_value` and `bevy_ts::color`, owned by reflect-enums-color (its design
  decision 7). reflect-assets adds a sibling of `reflected.rs`'s component layer: `ReflectAsset`
  lookup, base from `default_of`, `add` / `get_mut` instead of `insert` / `reflect_mut`.
- The hole walker does not exist yet: reflect-coverage task 1.5 (0/28). Its design (design.md:80-87)
  walks registrations by type-data filter (`ReflectComponent` | `ReflectResource`). Adding
  `ReflectAsset` is one predicate. Ask lead@reflect-enums-color for it rather than a second walker.

## 2. Bevy 0.20 ReflectAsset

- `bevy_asset-0.20.0/src/reflect.rs:27-157`: `get`, `get_mut`, `get_unchecked_mut`, `add(&mut World,
  &dyn PartialReflect) -> UntypedHandle`, `insert`, `remove`, `len`, `ids`, `handle_type_id`,
  `assets_resource_type_id`.
- `add` and `insert` go through `FromReflect` and panic if it fails (:179-180). Always write into a
  full default first, then `add` the finished value. Never `add` a partial.
- `Handle<A>` is a reflected enum (`handle.rs:113`), `#[reflect(Handle)]`, no ReflectDefault. Typed
  from untyped without static T: `ReflectHandle` (registered under `handle_type_id()`) `.typed(untyped)`
  (reflect.rs:251). The same handle can go into a component field (`MeshMaterial3d<StandardMaterial>`).
- Every mutable borrow through ReflectAsset marks the asset changed and sends
  `AssetEvent::Modified` (assets.rs:635-638, 680-686), even when nothing changes. `set` must
  dry-run or compare first, then borrow.
- Reflect-registered assets in 0.20 (`register_asset_reflect`, asset lib.rs:601): StandardMaterial,
  Image, Mesh, TextureAtlasLayout, ColorMaterial, SpriteMeshMaterial, AnimationClip, AnimationGraph,
  ShaderBuffer, AutoExposureCompensationCurve. Not reflected at all: Font, Gltf, AudioSource, Shader,
  scenes. `ExtendedMaterial` derives Reflect but is never registered.
- Paths: no `load_with_settings` in 0.20. `load_builder().with_settings::<S>(closure)` (server/mod.rs
  :1737) and `.load_erased(type_id, path)` (:1798). Settings are applied by `downcast_mut::<S>`; a
  wrong type only logs (meta.rs:258-272). So erased settings need a small typed table per loader.
  Labels go in the path (`scene.glb#Mesh0`).

## 3. Image and Mesh (lead's questions)

- Image is `reflect(opaque)` (`bevy_image-0.20.0/src/image.rs`, the `Image` derive above :654).
  `image.sampler` is not reachable by a reflect path. `ImageSampler`, `ImageSamplerDescriptor`,
  `ImageAddressMode`, `ImageFilterMode` are all Reflect + Default (image.rs:697, 747, 781, 854), so
  the writer can build an `ImageSampler` value. Getting it onto an Image needs hand code: a typed
  `Assets<Image>::get_mut(id).sampler = value`.
- `ImageLoaderSettings` is serde only, not Reflect (image_loader.rs:133); `ImageFormatSetting` too.
  For the path form, map a reflected `ImageSampler` (plus `is_srgb`) into
  `with_settings::<ImageLoaderSettings>`. One typed entry, ours.
- Mesh: `primitive_topology` and `attributes` are `#[reflect(ignore)]` (bevy_mesh mesh.rs:231-238).
  `indices` is reflected only through a private enum. Mesh data is opaque to reflection, so
  script-assets adds `insert_attribute` / `insert_indices`. Reflection covers only `asset_usage`,
  `enable_raytracing` and the like.

## 4. Hand-wired assets today

| Place | What | Fate |
|---|---|---|
| `<material>`, `bevy-ts-tsx/src/intrinsics.rs:60-167` | 5 props, cached `Assets<StandardMaterial>::add` | standard-material-fields lands it at 42 props, then its serde fields move to the generic path |
| material-alpha branch (8/12) | `<material alpha alphaMode>`, `MaterialAlpha` component | lands as is; props fold later, component stays |
| standard-material-fields branch (16/19) | every field, texture paths, colour-space rule, lazy tangents | lands as is; keep texture loading, colour space, tangents, limits, coverage tests |
| `<mesh>`, intrinsics.rs:12-129; mesh-primitives (17/18) | shapes to Mesh | not ours (opaque mesh data) |
| recipe `cuboid`, `bevy-ts/src/recipes.rs:93-110` | uncached Mesh + material add | leave |
| `<material kind>`, `bevy-ts-material/src/kinds.rs:495-541` | ScriptMaterial, texture params by path | leave; ScriptMaterial is not reflect-registered |
| `<image src>`, `bevy-ts-tsx/src/ui.rs:824` | `AssetServer::load` | the generic Handle rule covers it already |
| animation, levels, viewport, cli | Rust-side | not script API |

No TS assets API exists today: no `assets.add/get/set`. `res()` reaches only resources with a native
layout, so `Assets<T>` is unreachable from TS.

## 5. TS types

- `scripts/gen-bevy-types.sh` runs `bevy-ts types --bevy` (`bevy-ts-cli/src/types.rs:320`
  `emit_bevy`) and writes `packages/bevy-ts/bevy.d.ts` and `crates/bevy-ts-v8/js/bevy.js`.
- `named()` (types.rs:265-273) keeps only `ReflectComponent` types. No asset type is in bevy.d.ts.
- Reflect-tier components are `Reflected<{fields}>`, and TSX elements get props through
  `LibraryManagedAttributes` (jsx-runtime.d.ts:225-235).
- `Handle<T>` is typed `string` (types.rs:163). `Option<Handle<Image>>` is still reported as a hole
  in today's bevy.d.ts (e.g. `ClusteredDecal.base_color_texture`, :377). reflect-enums-color's
  `Option` support should clear that.
- For assets: add `ReflectAsset` to `named()`, emit them as e.g. `Asset<{fields}>`, and type a
  handle field `string | AssetRef<T>`. Then `<StandardMaterial .../>` used as a prop value types
  from the same generated fields.

## 6. Open questions and ask-first items

- New pattern (ask-first): a TSX element that is a value (an asset), not an entity. Today every tag
  spawns or edits an entity. Ask how it is spelled: `<StandardMaterial/>` as a prop value, or a
  function `StandardMaterial({...})` returning a handle ref. The function form needs no reconciler
  change.
- Performance (ask-first): an asset per render. Cache by the props' canonical JSON, as
  `IntrinsicAssets` does (intrinsics.rs:112), and drop it when no entity holds the handle. Writes
  must skip no-op borrows (section 2).
- Handle lifetime in TS: a handle from `assets.add` is a strong handle held in the JS heap. It
  needs a finalizer or explicit `drop`. Who owns that: engine-core's handle lifecycle, per the lead.
- `insert`/`add` panic on a FromReflect failure. Always build from a default, check, then add.
- Asset ids in TS: nothing writes `AssetId`/UUID today. Pick one ref shape (`{ asset: id }` or an
  opaque class).
- No new dependency.

## Recommendation

Build on reflect-enums-color's writer. Add one asset layer in `bevy-ts-tsx` (or a new
`bevy_ts::reflect_asset` beside `reflect_value`), sharing the writer.

Staging:

1. After reflect-enums-color lands: `assets.add(Type, props)` / `get` / `set` in TS through
   ReflectAsset, with a handle ref usable in any Handle field. Generated asset types in bevy.d.ts.
   Add `ReflectAsset` to reflect-coverage's walker filter (ask that lead).
2. TSX value form: `<StandardMaterial .../>` (or `StandardMaterial({...})`) as a handle-valued prop,
   cached by props. Move `<material>`'s serde fields onto it; keep its texture colour-space rule and
   tangents.
3. Image: path form with sampler settings through one typed `ImageLoaderSettings` entry, and
   `image.sampler` as a hand-mapped field since Image is opaque. Atlas tiling (address mode repeat)
   lands here.

Mesh data stays with script-assets.
