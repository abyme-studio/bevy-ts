## Context

Findings are in `research.md` (2026-10-23, Bevy 0.20.0). The short of it:

- `bevy_ts::reflect_value::write(registry, server, slot, base, json)` (reflect-enums-color) writes
  JSON into any reflected slot: `Color`, `Handle<T>` from a path, `Option`, enums, struct merges. It
  takes no `World`, so an asset slot writes as a component slot does. reflect-coverage stage 1 adds
  `read`, its inverse, the `UNCOVERED` allowlist and the hole check. reflect-enums-color owns all of
  `reflect_value`, `color` and the walker.
- `ReflectAsset` gives `get`, `get_mut`, `add`, `insert`, `ids`, `handle_type_id`. `add` and
  `insert` panic when `FromReflect` fails. Every `get_mut` marks the asset changed and sends
  `AssetEvent::Modified`, even when nothing changes.
- Reflect-registered assets: `StandardMaterial`, `Image` (opaque), `Mesh` (data ignored, no
  `ReflectDefault`), `TextureAtlasLayout`, `ColorMaterial`, `SpriteMeshMaterial`, `AnimationClip`,
  `AnimationGraph`, `ShaderBuffer`, `AutoExposureCompensationCurve`.
- `ImageLoaderSettings` is serde, not `Reflect`; `with_settings::<S>` downcasts and only logs on a
  wrong type, so erased settings need a typed table.
- `<material>` (`intrinsics.rs`) caches `Assets<StandardMaterial>::add` by props in
  `IntrinsicAssets`. standard-material-fields grows it to 42 props with texture paths, a colour
  space per field and lazy tangents; material-alpha adds `alpha`, `alphaMode` and `MaterialAlpha`.
- TS systems reach the world through free functions (`res`, `commands`, `query`). No asset API.

## Goals / Non-Goals

**Goals:**
- One TS path to build, read and edit every `ReflectAsset` type, on the one writer and reader.
- One way to put an asset in a `Handle<T>` slot, wherever that slot is.
- `<material>` on that path with no change for authors.
- Tiling textures: the image sampler and loader settings.

**Non-Goals:**
- Mesh attributes and indices (script-assets). `assets.add(Mesh, ...)` is refused: no default.
- Load state, handle lifecycle and load events (script-assets). Script-made assets live until restart or stop.
- Assets Bevy does not reflect; `ScriptMaterial` (`<material kind>`).
- Bindable asset props. Asset values are written on insert and on a changed prop.
- A second writer, reader or walker.

## Decisions

1. **One module, `bevy_ts::reflect_asset`, beside `reflect_value`.** It owns: the asset lookup by
   type path (`ReflectAsset` from the registration), `add`, `get`, `set`, the ref table, the value
   cache, the `Handle` hook (decision 4), the hand-mapped types list and the loader-settings table (decision 9). It
   calls `reflect_value::write`, `read` and `default_of`; it never walks a value itself. TSX
   (`bevy-ts-tsx`) and scripts (`host.rs`) both call it.
   *Alternative:* an asset layer inside `bevy-ts-tsx`'s `reflected.rs`. Rejected: scripts need it,
   and `bevy-ts-tsx` depends on `bevy-ts`, not the other way.

2. **`add` builds whole, checks, then adds.** `add(world, type_path, json)`: `default_of(type)`
   (refused, naming the type, when there is none: `Mesh`), `write` with the `AssetServer`, then
   `ReflectFromReflect::from_reflect` on the result; a `None` fails with the type path. Only a
   value that converted is passed to `ReflectAsset::add`, so `add`'s panic cannot fire. A partial
   value is never added.
   A TS run holds only `&World`, and Bevy 0.20 has no erased add or id reserve without `&mut World`.
   So `add` from a script checks and builds the value at the call (errors throw there), returns
   its ref at once, and queues the value in `ScriptAssets`. The queue flushes with `&mut World`
   right after the run, before the run's commands apply, so a ref inserted in the same run
   resolves. The asset is visible to the script at once (`get` reads queued values) and is in
   `Assets<T>` when the run ends.

3. **`set` borrows only to write.** `set(world, ref, json)`: clone the current value through
   `ReflectAsset::get`, `write` the patch over the clone (the patch merges over the asset's current
   value, not over a default), compare with `reflect_partial_eq`. Equal: return, no borrow, no
   event, nothing queued. Different: queue the clone; the flush after the run does `get_mut` and
   `apply`, so `AssetEvent::Modified` fires at run end, the same frame. A later `set` or `get` in
   the run sees the queued value. A failing patch changes nothing.
   `get(world, ref)` is `read` of `ReflectAsset::get`; a missing asset is `undefined` in TS.
   *Alternative:* write straight into `get_mut`. Rejected: every call would send `Modified` and
   re-prepare the asset.

4. **Refs, values and paths share one `Handle` hook.** The writer's `Handle` arm takes a path
   string today. reflect-enums-color adds one hook to `reflect_value::Cx`: an optional
   `&dyn HandleSlots` with `write(handle_type, json) -> Result<UntypedHandle>` for any non-string
   JSON, and `read(untyped) -> Option<Value>` tried before the path in `read`. This change
   implements it. It resolves three shapes, each tagged by one key so none can be mistaken for a
   struct:
   - `{ "$asset": n }`: a ref from `assets.add`. Its typed handle comes from `ReflectHandle::typed`.
     A ref of the wrong asset type fails naming both types.
   - `{ "$of": type_path, "props": {...} }`: an asset value (decision 6).
   - `{ "$load": path, "settings": {...} }`: a path with loader settings (decision 9).
   `read` of a handle in the ref table gives its `{ "$asset": n }`; other handles keep reflect-coverage's
   rule (path, else `null`). With no hook, any of the three shapes fails naming the handle type
   (reflect-coverage's rule): nothing could apply it. So every check pass that may meet one
   installs the hook in check mode: TSX's load-time `check` and the dry run before `add`/`set`. In
   check mode the hook checks each shape's form, that a ref is live and of the slot's asset type,
   and that a value's props write over its type's default; it builds and loads nothing. No pass
   runs without the hook and must accept them: the hole check round-trips `read` output, which
   holds no refs, and TS type-checking runs no Rust. The TS type of a `Handle<T>` slot becomes `string | AssetRef<T> | AssetValue<T> |
   null`; reflect-coverage owns `types.rs`'s handle arm and takes that type.
   *Alternative:* an `"asset:7"` string. Rejected: a real path could look like it, and settings do
   not fit in a string.

5. **The ref table.** `ScriptAssets`, a resource: ref number `n` (a `u64`) -> an entry, and the
   reverse map by `UntypedAssetId`. An entry is one of two kinds. `Uuid` entries are this
   change's: a `Handle::Uuid`, kept until restart or stop (below). `Strong` entries are
   script-assets': a strong handle it adds with `insert_strong` and frees with its `drop` and
   `remove`, dropped on hot reload. The rules below are for `Uuid` entries. The UUID is built from the asset type, a per-session tag and
   `n`, so a ref has its id before the asset exists, and the flush after the run (decision 2)
   inserts the asset at that id with `ReflectAsset::insert`. `assets.add` and the value cache
   (decision 6) put entries in it. A ref is `{ $asset: n, type }` in TS, an `AssetRef<T>` frozen
   object; `n` counts up from 1 and is never reused in a session.
   A UUID handle holds no strong count, so the asset stays until it is removed. A hot reload
   starts a new tag: old refs fail with "stale asset ref", but their assets stay, so entities
   that hold the handles keep drawing. A restart or stop removes every asset the table made, with
   `ReflectAsset::remove`. Freeing an asset sooner is script-assets' handle lifecycle.
   *Alternatives:* strong index handles (need `&mut World` to reserve, which a run lacks); raw
   `AssetId` bits (no id before the flush).

6. **Asset values: `.of`, cached by canonical props.** Every generated asset class has
   `.of(props): AssetValue<T>`, which returns `{ $of: type_path, props }` and does nothing else.
   Written into a `Handle<T>` slot, the hook keys a cache by the type path and the props'
   canonical JSON (keys sorted). A hit returns the cached handle; a miss calls `add` and caches.
   So every slot given equal props shares one asset, as `<material>` does today. Cached handles
   live in `ScriptAssets` with the same lifetime. A value is built only when its slot is written: TSX
   insert and changed prop, `assets.add`/`set` with it in a field, a reflected insert. Never per
   frame. `assets.set` on the ref of a cached value changes every slot sharing it; docs say so,
   and `assets.add` makes an unshared one.
   *Alternative:* a TSX element as a value (`material={<StandardMaterial ... />}`). Rejected here:
   every tag spawns or edits an entity today, so it needs a new element kind in the reconciler,
   and `.of` is the spelling reflect-coverage gives reflect-tier components. It is ask-first item 1
   if wanted.

7. **The TS surface.** `import { assets } from "bevy-ts"` and asset classes from `bevy-ts/bevy`:
   ```ts
   const img = Image.load("textures/atlas.png", { sampler: { Descriptor: { address_mode_u: "Repeat", address_mode_v: "Repeat" } } });
   const mat = assets.add(StandardMaterial, { base_color: "#ffd7a8", base_color_texture: img });
   assets.set(mat, { perceptual_roughness: 0.3 });
   assets.get(mat)?.base_color;
   <entity><Mesh3d value={meshRef} /><MeshMaterial3d value={StandardMaterial.of({ unlit: true })} /></entity>
   ```
   `assets` works wherever scripts reach the world: inside a system's run, during a TSX render, and
   during the script's load (a scene's first render runs at module load; `ScriptBackend::load_with`
   with `host::LoadHost`).
   Each call is one host call carrying JSON through one new `HostCalls` method; its result is
   visible to the script at once and lands in `Assets<T>` when the run ends (decisions 2, 3).
   Outside those (a timer callback) it throws, naming where it may be called.
   `types.rs`'s `named()` also takes registrations with `ReflectAsset`, emitted as
   `asset("<type path>")` classes typed `AssetClass<{ fields }>`, with the same short-name and
   crate-prefix rule. `Image` is typed by its hand-mapped fields (`{ sampler: ImageSampler }`)
   plus `load`. An asset with no default (`Mesh`) is typed with no `of` and `add` refuses it at
   the type level.

8. **`<material>` becomes a prop table over the generic path.** `MaterialProps`' serde fields go.
   `intrinsics.rs` keeps one table: prop name → `StandardMaterial` field and a value mapping
   (`color` → `base_color`; `roughness` → `perceptual_roughness`; camelCase → snake_case; enum
   spellings `"back"`/`null`, `"uv0"`, `{ relief: { maxSteps } }` → Bevy's variant JSON;
   `uvTransform` → `Affine2`; `alpha` → `base_color`'s alpha; `alphaMode` → `alpha_mode`, unset
   following material-alpha's rule). The element's props become one `StandardMaterial.of` value
   and the handle goes on `MeshMaterial3d`, through the same cache. Kept from
   standard-material-fields unchanged: texture-path loading and the colour-space rule (in stage 2
   still through its own code; in stage 3 each texture prop maps to an `Image.load` value with
   `is_srgb` per field), the tangent swap, the 16-sampler limits test, the blocked-texture list and
   the coverage tests. Unknown props, bad enums and bad colours fail at the element as today, with
   the same messages. `jsx-runtime.d.ts`' `MaterialProps` stays hand-written, and a test checks it
   against the table. `<mesh color>` calls the same path. `MaterialAlpha` is untouched.
   *Alternative:* `<material>` takes Bevy's snake_case names. Rejected: breaks every script.

9. **Hand-mapped types (stage 1), Image's entry and one typed loader entry (stage 3).** An asset
   type opaque to reflection gets one whole-type entry in a list in `reflect_asset`:
   `{ asset: TypeId, read(&World, &dyn Reflect) -> Value, write(&World, base, json) -> Box<dyn Reflect> }`.
   `add`, `get` and `set` use a type's entry in place of the reflected path. `add` writes over the
   type's default; `set` merges the patch over `read(current)` at the top level (no deep merge),
   writes the result, and detects a no-op by comparing that read with the current read: equal
   queues nothing. The list and its use land in stage 1, empty of entries this change ships.
   Entries:
   - `Image` (stage 3): `sampler` only. `read` gives `{ sampler }` by `reflect_value::read` of the
     `ImageSampler`; `write` builds an `ImageSampler` with `reflect_value::write` and returns a
     copy of the base `Image` with it. `ImageSampler` and its descriptor types are `Reflect` +
     `Default`, so their JSON is the writer's: `"Default"`,
     `{ Descriptor: { address_mode_u: "Repeat", mag_filter: "Nearest", ... } }`.
   - `AnimationGraph`: added by animation-control (its task 8.6), on the same list.
   Loader settings: one table, asset type path → a typed apply over a `LoadBuilder`. Its one entry
   is `Image`: `settings.sampler` (an `ImageSampler`, by the writer) and `settings.is_srgb` (bool),
   applied with `with_settings::<ImageLoaderSettings>` then `load_erased`. Unknown settings keys
   fail. A type with no entry has no `load` and its `$load` fails naming the type.
   `ImageLoaderSettings` is not `Reflect`, so the entry's two keys are read by hand; adding a key
   is one line.
   *Alternative:* per-field hand-maps (`Image.sampler`). Rejected: a whole-type entry also serves
   `AnimationGraph`, whose data is one opaque graph.

10. **Allowlist entries, not the walker.** reflect-enums-color adds `ReflectAsset` types to the
    hole check's filter and puts every asset field the check cannot cover on
    `reflect_value::UNCOVERED`, each reason starting `reflect-assets:`. Each stage of this change
    deletes the entries it covers and the hole check proves the field now round-trips. Stage 1 is
    expected to clear handle-to-asset fields; stage 3 clears `Image`'s `sampler`. Entries owned
    elsewhere (`Mesh` data, script-assets) keep their owner's mark. This change never edits the
    walker or its filter.

## Seams

| Seam | Owner | This change |
|---|---|---|
| `reflect_value` (`write`, `read`, `default_of`, `UNCOVERED`), `color`, the hole check's walker | reflect-enums-color | calls them; asks for the `HandleSlots` hook (decision 4) through lead@reflect-assets; removes its own `UNCOVERED` entries |
| `types.rs` handle arm | reflect-coverage | asks for `string \| AssetRef<T> \| AssetValue<T> \| null`; adds asset classes in `named()` |
| `<material>` props, colour space, tangents, limits | standard-material-fields, then this change in stage 2 | rebases after it lands; replaces `MaterialProps` with the table |
| `alpha`, `alphaMode`, `MaterialAlpha` | material-alpha | folds the props in; leaves the component |
| `<mesh>`, `MeshProps` | mesh-primitives | `<mesh color>` calls the shared material path; no `MeshProps` edit |
| Handle lifecycle, load state, `AssetEvent`, `Mesh` building | script-assets (`openspec/changes/script-assets`, engine-core's area) | assets removed on restart or stop only; refs stale on hot reload |

## Ask-first items

1. **Asset values in TSX (new pattern).** Removed by decision 6: `.of` returns a plain value, so no
   element is a value and the reconciler does not change. If the operator wants
   `<StandardMaterial ... />` as a prop value, that adds an element kind that spawns nothing; it
   costs a reconciler arm and a JSX type. Alternative: keep `.of`.
2. **Asset caching by props (performance).** Kept, not removed: decision 6 caches by canonical
   JSON. Cost: one sort and hash of the props on each insert and changed prop, never per frame;
   memory, one asset per distinct prop set until restart or stop. This is `IntrinsicAssets` today made
   generic. Alternative: no cache, one asset per slot (more GPU memory, no batching).

## Risks / Trade-offs

- [Assets kept across hot reloads: a script that adds at startup adds again on each reload, and the old assets stay until stop] → A known dev-only limit: each dev hot reload keeps one more copy of the script's assets, freed on restart or Stop. Freeing unreferenced ones would need an erased walk of every component for handles; not worth it for dev. Test `reflect_asset::hot_reloads_keep_assets_until_a_restart`; `docs/assets.md` says it.
- [`assets.add` in a per-frame system grows memory until stop] → Docs say to add once and `set`
  after; a log warning when one script session passes 10,000 refs. Freeing is script-assets'.
- [`assets.set` on a cached value's ref changes every sharer] → Docs; `assets.add` for an unshared
  asset.
- [`assets.get` per frame costs a host call and a JSON read] → Docs say to keep the value in JS;
  not a framework per-frame path.
- [One image path loaded with two settings: Bevy keys the handle by path, so the first wins] →
  The ref table records settings per path; a second load with other settings logs a warning naming
  the path, as standard-material-fields' docs already advise separate files.
- [The prop table and `MaterialProps` d.ts drift] → A test checks both lists match.
- [The hook widens the writer's `Handle` arm] → It is one trait object owned by reflect-enums-color;
  without it, behaviour is unchanged.
- [A stale ref after reload] → Fails with "stale asset ref", naming the ref.

## Migration Plan

Stage 1: additive for scripts; protocol minor bump for the handle read. Stage 2: `<material>` keeps
every prop, value and error; its tests run unchanged. Stage 3: additive. Rollback is a revert per
stage.

## Open Questions

None. The two ask-first items above wait on the operator's word on spelling (1) and caching (2).
