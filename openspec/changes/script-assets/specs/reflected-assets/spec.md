## MODIFIED Requirements

### Requirement: Rust hand-maps asset types
`bevy_ts::prelude::BevyTsAppExt` SHALL have `ts_hand_map::<T: Asset>(read, write)`, callable from any crate, with `read(slots, source, &T) -> JSON` and `write(slots, base: Option<&T>, json) -> T`. `T` SHALL need no `Reflect` or `ReflectAsset`. `assets.add`, `assets.get`, `assets.set`, `T.of(props)` and refs SHALL find `T` by its type path and use its entry in place of the reflected path, a hand map winning over reflection for a type that has both. The flush SHALL insert the built `T` whole into `Assets<T>` at the ref's id, for an add and a set. `read` SHALL be given the JSON a script last wrote the asset from, `None` for an asset no script wrote. `set` SHALL merge its patch over the current read at the top level, write the result over the current asset, and queue nothing when the new read equals the old. A restart or stop SHALL remove hand-mapped assets as it removes reflected ones. A check before a run (TSX's load check) SHALL know hand-mapped types. An app's own map of a type SHALL win over bevy-ts' own (`Mesh`, `Image`), whether it is registered before or after bevy-ts' own. The same registration run again (one place in the code, another app in the process) SHALL replace its entry; a map of a type the app already mapped from another place SHALL panic naming the type and both places.

#### Scenario: A type with no Reflect
- **WHEN** a crate calls `app.init_asset::<Note>().ts_hand_map::<Note>(read, write)` for a `Note` that is `Asset` and `TypePath` only, and a script calls `assets.add(Note, { text: "a" })`
- **THEN** after the run `Assets<Note>` holds the `Note` write built, at the ref's id, and `assets.get` on the ref returns `read`'s JSON

#### Scenario: A hand-mapped set merges at the top level
- **WHEN** a script calls `assets.set(note, { count: 2 })` on a note added with `{ text: "hi", count: 1 }`
- **THEN** the note has `text: "hi"` and `count: 2`, and a second `set` with `{ count: 2 }` queues nothing

#### Scenario: An app's own Mesh map wins
- **WHEN** an app calls `ts_hand_map::<Mesh>(read, write)` before any asset lookup, and a script then calls `assets.get` on a mesh ref
- **THEN** the app's `read` answers, not bevy-ts' own

#### Scenario: A type mapped twice by the app
- **WHEN** an app maps `Note` at `a.rs:3` and again at `b.rs:9`
- **THEN** the second call panics with "Note is hand-mapped twice: at a.rs:3 and at b.rs:9" (with the type's path and columns)

### Requirement: An asset goes in any Handle slot
A `Handle<T>` slot written through `reflect_value` SHALL take, besides an asset path: an `AssetRef<T>` from `assets.add`; an asset value `T.of(props)`; and, for a type with a loader-settings entry, `T.load(path, settings)`. This SHALL hold in every slot the writer reaches: a TSX prop of a reflect-tier component or a writable hole, a reflected component given to `commands.spawn` or `insert` (and the check before it applies), a reflected value a system writes back through `Mut<T>`, and a field of an asset given to `assets.add` or `assets.set`. A ref or value of another asset type SHALL fail naming both types. A ref after a hot reload, restart or stop SHALL fail with "stale asset ref"; a hot reload SHALL make refs stale only once it applies, so a failed reload leaves the live refs working. A handle read by `reflect_value::read` SHALL read as its `AssetRef` when a script made the asset, and by reflect-coverage's rule otherwise.

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

#### Scenario: Refs in spawned components
- **WHEN** one system calls `commands.spawn(Mesh3d.of(meshes.add(a)))`, `commands.spawn(Mesh3d.of(meshes.add(b)))` and the same with two `server.load(path, Mesh)` refs, and a later system does it again
- **THEN** every entity is spawned and its `Mesh3d` holds its own ref's handle

#### Scenario: A ref written through Mut
- **WHEN** a system sets `m.value = ref` on a `Mut<Mesh3d>` from its query
- **THEN** the entity's `Mesh3d` holds that ref's handle
