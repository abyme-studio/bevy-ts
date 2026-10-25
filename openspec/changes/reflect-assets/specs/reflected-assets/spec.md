## ADDED Requirements

### Requirement: Scripts add, read and set reflected assets
`bevy-ts` SHALL export `assets` with `add(Type, props)`, `get(ref)` and `set(ref, patch)` for every asset type registered with `ReflectAsset` and a default. `add` SHALL build the asset from its type's default, write `props` over it with `bevy_ts::reflect_value::write`, convert it with `FromReflect`, and only then add it; a value that fails any step SHALL fail the call naming the type and field, and SHALL add nothing. `add` SHALL return an `AssetRef<T>`. A type with no default (`Mesh`) SHALL be refused, naming the type. `get` SHALL return the asset read by `reflect_value::read`, or `undefined` when the asset is gone. `set` SHALL write `patch` over the asset's current value; when the result equals the current value it SHALL NOT borrow the asset mutably, so no `AssetEvent::Modified` is sent; otherwise it SHALL apply the result once. A failing patch SHALL change nothing. The calls SHALL work inside a system's run, during a TSX render and during the script's load (a scene's first render runs at module load), and SHALL throw elsewhere (a timer callback), naming where they may be called. An `add` or `set` SHALL be checked at the call, throwing there on an error, and SHALL be visible to the script at once (a `get` in the same run reads it); it SHALL be in `Assets<T>`, with its `AssetEvent`, when the run ends, before the run's commands apply. A no-op `set` SHALL queue nothing.

#### Scenario: Visible at once, in Assets when the run ends
- **WHEN** a system calls `assets.add(StandardMaterial, { unlit: true })`, then `assets.get` on the ref, in one run
- **THEN** `get` returns the material with `unlit: true`, and after the run `Assets<StandardMaterial>` holds it, flushed before the run's commands apply

#### Scenario: Add a material
- **WHEN** a system calls `assets.add(StandardMaterial, { base_color: "#ff8800", unlit: true })`
- **THEN** `Assets<StandardMaterial>` holds a new material with that colour and `unlit: true`, other fields at their defaults, and the call returns its ref

#### Scenario: A bad field adds nothing
- **WHEN** a system calls `assets.add(StandardMaterial, { roughnes: 0.5 })`
- **THEN** the call throws naming `StandardMaterial` and `roughnes`, and the asset count is unchanged

#### Scenario: A no-op set sends no event
- **WHEN** a system calls `assets.set(mat, { unlit: true })` on a material already unlit
- **THEN** no `AssetEvent::Modified` is sent for it

#### Scenario: A set changes one field
- **WHEN** a system calls `assets.set(mat, { perceptual_roughness: 0.2 })`
- **THEN** `assets.get(mat).perceptual_roughness` is `0.2`, every other field keeps its value, and one `AssetEvent::Modified` is sent

#### Scenario: Mesh has no default
- **WHEN** a system calls `assets.add(Mesh, {})`
- **THEN** the call throws naming `Mesh` and that it has no default

### Requirement: An asset goes in any Handle slot
A `Handle<T>` slot written through `reflect_value` SHALL take, besides an asset path: an `AssetRef<T>` from `assets.add`; an asset value `T.of(props)`; and, for a type with a loader-settings entry, `T.load(path, settings)`. This SHALL hold in every slot the writer reaches: a TSX prop of a reflect-tier component or a writable hole, and a field of an asset given to `assets.add` or `assets.set`. A ref or value of another asset type SHALL fail naming both types. A ref after a hot reload, restart or stop SHALL fail with "stale asset ref"; a hot reload SHALL make refs stale only once it applies, so a failed reload leaves the live refs working. A handle read by `reflect_value::read` SHALL read as its `AssetRef` when a script made the asset, and by reflect-coverage's rule otherwise.

#### Scenario: A ref on a mesh entity
- **WHEN** a TSX element has `<MeshMaterial3d value={mat} />` with `mat` from `assets.add`
- **THEN** its `MeshMaterial3d<StandardMaterial>` holds that material's handle

#### Scenario: A texture inside a material
- **WHEN** a script calls `assets.add(StandardMaterial, { base_color_texture: img })` with `img` an image ref
- **THEN** the material's `base_color_texture` is that image's handle

#### Scenario: The wrong asset type
- **WHEN** a `Handle<StandardMaterial>` slot is given an `Image` ref
- **THEN** the write fails naming `StandardMaterial` and `Image`

#### Scenario: A stale ref
- **WHEN** a ref made before a script reload is used after it
- **THEN** the call fails with "stale asset ref", and the asset it named is still in `Assets<T>`, so entities holding it keep drawing

### Requirement: Asset values are shared by equal props
`T.of(props)` SHALL return a plain value and SHALL NOT touch the world. When it is written into a `Handle<T>` slot, slots given the same type and props (compared by canonical JSON, keys sorted) SHALL share one asset, built on the first write. A value SHALL be built only when its slot is written (an insert or a changed prop), never per frame. Assets made by `add` and by values SHALL survive a hot reload and SHALL be removed on a restart or stop.

#### Scenario: Two equal values share
- **WHEN** two entities have `<MeshMaterial3d value={StandardMaterial.of({ unlit: true, base_color: "#fff" })} />` and `<MeshMaterial3d value={StandardMaterial.of({ base_color: "#fff", unlit: true })} />`
- **THEN** both hold the same handle, and one material was built

#### Scenario: A value changes on re-render
- **WHEN** a component re-renders with `StandardMaterial.of({ base_color: "#000" })` in place of `"#fff"`
- **THEN** the entity's handle is the `"#000"` material, and the `"#fff"` material is not rebuilt

### Requirement: Asset types in bevy-ts/bevy
`bevy-ts/bevy` SHALL export a class for every type registered with `ReflectAsset` in the prebuilt engine, named by the rule components use, typed with its reflected fields as the writer takes them. A class SHALL have `.of(props)` only when the type has a default. A `Handle<T>` field SHALL be typed `string | AssetRef<T> | AssetValue<T> | null`. `assets.add` SHALL reject at the type level a class with no default and props with unknown fields.

#### Scenario: Typed fields
- **WHEN** a script calls `assets.add(StandardMaterial, { unlit: "yes" })`
- **THEN** `tsc` reports the type of `unlit`

#### Scenario: A handle field takes a ref
- **WHEN** a script passes an `AssetRef<Image>` as `base_color_texture`
- **THEN** it type-checks, and an `AssetRef<StandardMaterial>` there does not

### Requirement: Rust hand-maps asset types
`bevy_ts::prelude::BevyTsAppExt` SHALL have `ts_hand_map::<T: Asset>(read, write)`, callable from any crate, with `read(slots, source, &T) -> JSON` and `write(slots, base: Option<&T>, json) -> T`. `T` SHALL need no `Reflect` or `ReflectAsset`. `assets.add`, `assets.get`, `assets.set`, `T.of(props)` and refs SHALL find `T` by its type path and use its entry in place of the reflected path, a hand map winning over reflection for a type that has both. The flush SHALL insert the built `T` whole into `Assets<T>` at the ref's id, for an add and a set. `read` SHALL be given the JSON a script last wrote the asset from, `None` for an asset no script wrote. `set` SHALL merge its patch over the current read at the top level, write the result over the current asset, and queue nothing when the new read equals the old. A restart or stop SHALL remove hand-mapped assets as it removes reflected ones. A check before a run (TSX's load check) SHALL know hand-mapped types.

#### Scenario: A type with no Reflect
- **WHEN** a crate calls `app.init_asset::<Note>().ts_hand_map::<Note>(read, write)` for a `Note` that is `Asset` and `TypePath` only, and a script calls `assets.add(Note, { text: "a" })`
- **THEN** after the run `Assets<Note>` holds the `Note` write built, at the ref's id, and `assets.get` on the ref returns `read`'s JSON

#### Scenario: A hand-mapped set merges at the top level
- **WHEN** a script calls `assets.set(note, { count: 2 })` on a note added with `{ text: "hi", count: 1 }`
- **THEN** the note has `text: "hi"` and `count: 2`, and a second `set` with `{ count: 2 }` queues nothing

### Requirement: The hole check covers assets
The reflected-coverage hole check SHALL walk every type registered with `ReflectAsset`. Every asset field it cannot cover SHALL be listed in `reflect_value::UNCOVERED` with a reason, and a field whose coverage this change owns SHALL have a reason starting `reflect-assets:`. When a stage of this change lands, no entry it covers SHALL remain, and the check SHALL pass.

#### Scenario: A covered field leaves the list
- **WHEN** stage 3 lands
- **THEN** `UNCOVERED` has no `reflect-assets:` entry for `Image`'s sampler, and the hole check passes

### Requirement: Image sampler and loader settings
`Image` SHALL expose its `sampler` to `assets.get` and `assets.set`, though `Image` is opaque to reflection: `get` SHALL read it as an `ImageSampler` JSON value, and `set` SHALL write it over the current sampler and assign it only when it changed. `Image.load(path, { sampler?, is_srgb? })` SHALL be an asset value that loads `path` with `ImageLoaderSettings` holding those settings. An unknown settings key SHALL fail naming it. A `load` on a type with no loader-settings entry SHALL fail naming the type. Loading one path with two different settings in one session SHALL log a warning naming the path.

#### Scenario: A repeating sampler
- **WHEN** a script calls `assets.set(img, { sampler: { Descriptor: { address_mode_u: "Repeat", address_mode_v: "Repeat" } } })`
- **THEN** the image's sampler repeats on both axes, and `assets.get(img).sampler` reads it back

#### Scenario: A loaded atlas that tiles
- **WHEN** a slot is given `Image.load("textures/atlas.png", { sampler: { Descriptor: { address_mode_u: "Repeat", address_mode_v: "Repeat" } } })`
- **THEN** the image loads with that sampler, and a material with `uvTransform` scale 4 draws the atlas four times across

#### Scenario: Linear data
- **WHEN** a slot is given `Image.load("textures/wall_n.png", { is_srgb: false })`
- **THEN** the image loads with `is_srgb: false`
