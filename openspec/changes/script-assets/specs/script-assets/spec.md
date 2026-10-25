## ADDED Requirements

### Requirement: `Res<AssetServer>` loads assets and reports their load state
A TS system that takes `server: Res<AssetServer>` SHALL get Bevy's `AssetServer`. `server.load(path)` SHALL call Bevy's load and return an `AssetRef<A>` holding Bevy's strong handle, typed by the second argument (`server.load(path, Mesh)`) when given, else by the default loader for the path's extension. A labelled path (`"models/fox.glb#Mesh0/Primitive0"`) with no type argument SHALL throw naming the path. Loading the same path twice SHALL return refs to the same Bevy handle.

`server.loadState(h)`, `server.dependencyLoadState(h)` and `server.recursiveDependencyLoadState(h)` SHALL return Bevy's variant names, `"NotLoaded"`, `"Loading"`, `"Loaded"`, or `{ Failed: <the error's text> }`. `server.isLoaded(h)`, `server.isLoadedWithDirectDependencies(h)` and `server.isLoadedWithDependencies(h)` SHALL return Bevy's answers. Each takes an `AssetRef`; the `AssetId` form comes with the `AssetEvent` methods (task 1.4a). Each call SHALL be one host call.

#### Scenario: A texture loads and its state goes to Loaded
- **WHEN** a startup system loads `"textures/grid.png"` and an update system reads `server.loadState(ref)` and `server.isLoadedWithDependencies(ref)` each frame in a headless engine
- **THEN** the first reads are `"Loading"` and `false`, and within a bounded number of frames they are `"Loaded"` and `true`, and the `Image` is in `Assets<Image>` at the ref's id

#### Scenario: A missing file fails with Bevy's error
- **WHEN** a system loads `"textures/missing.png"`
- **THEN** `server.loadState(ref)` becomes `{ Failed: <text naming the path> }`, `recursiveDependencyLoadState` becomes `{ Failed: ... }`, and the engine keeps running

#### Scenario: A glTF waits for its dependencies
- **WHEN** a system loads a glTF that references a texture file
- **THEN** `isLoaded` turns `true` no later than `isLoadedWithDependencies`, and `recursiveDependencyLoadState` is `"Loaded"` exactly when `isLoadedWithDependencies` is `true`

#### Scenario: A labelled path needs a type
- **WHEN** a system calls `server.load("models/fox.glb#Mesh0/Primitive0")`
- **THEN** the call throws naming the path and asking for the asset type, and `server.load("models/fox.glb#Mesh0/Primitive0", Mesh)` returns an `AssetRef<Mesh>`

### Requirement: A script's handle is strong until it drops it
A ref returned by `server.load` or `meshes.add` SHALL hold a strong Bevy handle, so the asset stays while the script holds the ref, whether or not a component uses it. `assets.drop(ref)` SHALL drop that handle; afterwards the ref is stale and every use of it SHALL throw "stale asset ref". When no other strong handle remains, Bevy SHALL send `AssetEvent::Unused` and free the asset as it does for any handle. `assets.remove(ref)` SHALL be Bevy's `Assets::remove` at the ref's id: the asset goes at once, Bevy sends `AssetEvent::Removed`, and the ref goes stale. A ref to a UUID handle (reflect-assets' `assets.add`) SHALL keep its asset after `drop`, as Bevy's UUID handles do; `remove` frees it.

A hot reload SHALL drop every strong handle the old script session held. An asset a component still holds SHALL stay. Stop and restart SHALL drop them all.

#### Scenario: A dropped mesh nothing holds is freed
- **WHEN** a system adds a mesh, holds its ref in no component, and a later run calls `assets.drop(ref)`
- **THEN** a `MessageReader<AssetEvent<Mesh>>` sees `Unused` then `Removed` for the id, and `Assets<Mesh>` no longer has it

#### Scenario: A dropped mesh an entity draws stays
- **WHEN** a system adds a mesh, spawns an entity with `Mesh3d` holding it, then drops the ref
- **THEN** the mesh stays in `Assets<Mesh>` while the entity lives, and is freed after the entity despawns

#### Scenario: Remove frees at once
- **WHEN** a system calls `assets.remove(ref)` on a mesh an entity holds
- **THEN** the mesh is gone from `Assets<Mesh>` the same frame and a reader sees `Removed`

#### Scenario: A stale ref throws
- **WHEN** a system uses a ref after `assets.drop(ref)`
- **THEN** the call throws "stale asset ref" naming the ref

#### Scenario: Hot reload frees what only the old script held
- **WHEN** a script adds two meshes, puts one on an entity, and the script is hot reloaded
- **THEN** the mesh on the entity stays and the other is freed

### Requirement: Asset ids and Bevy's `AssetEvent` methods
`ref.id` SHALL be the asset's `AssetId<A>` in the shape a `MessageReader<AssetEvent<A>>` delivers it (rust-events). `AssetEvent<A>` in TS SHALL have Bevy's `isAdded`, `isModified`, `isRemoved`, `isUnused` and `isLoadedWithDependencies`, each taking an `AssetRef<A>` or an `AssetId<A>` and returning whether the event is that variant for that asset. An id SHALL stay valid after its ref is dropped, as Bevy's ids do.

#### Scenario: A load is seen as an event
- **WHEN** a system loads a texture and another reads `MessageReader<AssetEvent<Image>>`
- **THEN** exactly one event has `isLoadedWithDependencies(ref)` true, and `isAdded(ref.id)` is true for one event

#### Scenario: A mesh update is seen as Modified
- **WHEN** a system changes a mesh through `meshes.getMut(ref)`
- **THEN** a reader sees one event with `isModified(ref)` true that frame

### Requirement: An exported method takes and returns asset refs
A host method argument or result of type `AssetRef<A>` (Rust: `ScriptRef<A>`) SHALL cross as the ref's number and reach JS as the frozen `AssetRef<A>` object the `assets` API uses. A ref of another asset type, or a stale ref, SHALL throw naming the method, the expected type and the given one. `bevy-ts types` SHALL declare it as `AssetRef<A>` with `A` the Rust asset type's TS name.

#### Scenario: A native crate returns a handle
- **WHEN** a native crate exports a method returning `ScriptRef<Mesh>` and a TS system puts the result on `<Mesh3d value={ref} />`
- **THEN** the entity draws that mesh, and passing an `AssetRef<Image>` to a method taking `ScriptRef<Mesh>` throws naming both types

### Requirement: `ResMut<Assets<Mesh>>` and Bevy's `Mesh` API build meshes
A TS system that takes `meshes: ResMut<Assets<Mesh>>` SHALL get `add`, `get`, `getMut` and `remove`. A mesh SHALL be written with Bevy's API under Bevy's names in camelCase:

- `Mesh.new(topology, usage)`: `PrimitiveTopology.PointList | LineList | LineStrip | TriangleList | TriangleStrip`; `RenderAssetUsages.MAIN_WORLD`, `RENDER_WORLD`, combined with `|`, and `RenderAssetUsages.default()` (both).
- `mesh.insertAttribute(attr, data)` for `Mesh.ATTRIBUTE_POSITION`, `ATTRIBUTE_NORMAL`, `ATTRIBUTE_UV_0`, `ATTRIBUTE_UV_1`, `ATTRIBUTE_TANGENT`, `ATTRIBUTE_COLOR`, `ATTRIBUTE_JOINT_WEIGHT` (`Float32Array`) and `ATTRIBUTE_JOINT_INDEX` (`Uint16Array`), and `mesh.removeAttribute(attr)`. The array's length SHALL be a whole number of the attribute's components (3, 3, 2, 2, 4, 4, 4, 4).
- `mesh.insertIndices(Indices.U16(Uint16Array))` or `Indices.U32(Uint32Array)`, and `mesh.removeIndices()`.
- `mesh.computeNormals()`, `computeFlatNormals()`, `computeSmoothNormals()`, `computeAreaWeightedNormals()`, `generateTangents()`, `duplicateVertices()`, with Bevy's preconditions.
- `mesh.countVertices()` and `mesh.attribute(attr)`, a copy of the data as a typed array.

`meshes.add(mesh)` SHALL add it and return an `AssetRef<Mesh>`, in `Assets<Mesh>` at once. `meshes.getMut(ref)` SHALL return the live mesh; its writes SHALL go into the asset in place and Bevy SHALL send one `Modified` per frame it was changed. A precondition Bevy reports as an error (a wrong array kind or length, normals on a non-triangle topology, tangents without normals or UVs, data extracted to the render world) SHALL throw with Bevy's message and the method name, and SHALL leave the mesh as it was. No call SHALL panic.

#### Scenario: A quad with Bevy's calls
- **WHEN** a system builds `Mesh.new(PrimitiveTopology.TriangleList, RenderAssetUsages.default())` with four positions, four UVs, `Indices.U32` of six, calls `computeNormals()` and adds it to an entity's `Mesh3d`
- **THEN** the mesh in `Assets<Mesh>` equals the same calls made in Rust, attribute by attribute

#### Scenario: Every attribute round-trips
- **WHEN** a system inserts each of the eight `Mesh.ATTRIBUTE_*` with known data, adds the mesh, then reads each back with `meshes.get(ref).attribute(attr)`
- **THEN** every array read equals the one written, and the Rust mesh's `VertexAttributeValues` for each have Bevy's format

#### Scenario: Tangents need normals and UVs
- **WHEN** a system calls `generateTangents()` on a mesh with positions only
- **THEN** it throws with Bevy's `GenerateTangentsError` text and the mesh is unchanged

#### Scenario: A wrong array
- **WHEN** a system passes a `Float32Array` of 10 floats to `ATTRIBUTE_POSITION`, or a `Float32Array` to `ATTRIBUTE_JOINT_INDEX`
- **THEN** it throws naming the attribute, the expected element type and component count

#### Scenario: Render-world-only data cannot be edited
- **WHEN** a mesh added with `RenderAssetUsages.RENDER_WORLD` has been extracted and a system calls `getMut(ref).insertAttribute(...)`
- **THEN** it throws with Bevy's `MeshAccessError` text

### Requirement: Mesh data crosses as typed arrays with one copy
Mesh data SHALL cross from JS to Rust as typed-array views read in place, one host call per `insertAttribute` or `insertIndices`, and SHALL be copied once into Bevy's own vertex `Vec`. When the attribute or index buffer already has the same length and format, the copy SHALL reuse that `Vec` with no allocation. No mesh call SHALL cost a host call per vertex. A mesh built with `Mesh.new` SHALL keep its data in JS until `meshes.add`.

#### Scenario: A chunk remesh is a handful of calls
- **WHEN** a system remeshes a 24,000-vertex chunk with `getMut` and four `insertAttribute` calls and one `insertIndices`
- **THEN** the run makes six mesh host calls, and the bench reports the cost per remesh beside a Rust-only arm

### Requirement: `Mesh` reads and writes as an asset value
`Mesh` SHALL have a whole-type entry on reflect-assets' hand-mapped list. Its JSON SHALL use Bevy's names: `{ primitive_topology, asset_usage, attributes: { <attribute name, e.g. "Vertex_Position">: [[x, y, z], ...] }, indices: { U16: [...] } | { U32: [...] } | null }`. `assets.get(meshRef)` SHALL read it; `Mesh.of(json)` SHALL be an asset value, cached by content as reflect-assets caches values; `assets.set(meshRef, patch)` SHALL replace the attributes and indices the patch names. An unknown attribute name or a malformed array SHALL fail naming it. `bevy_mesh::mesh::Mesh` SHALL NOT be on `reflect_value::UNCOVERED`, and the hole check SHALL pass.

#### Scenario: A mesh reads back as Bevy-named JSON
- **WHEN** a system builds a triangle with `Mesh.new`, adds it and calls `assets.get(ref)`
- **THEN** the result has `primitive_topology: "TriangleList"`, `attributes.Vertex_Position` with three points, and `indices` as written

#### Scenario: The hole check covers Mesh
- **WHEN** `tests/reflect_coverage.rs` runs
- **THEN** it passes with no `UNCOVERED` entry for `bevy_mesh::mesh::Mesh`

### Requirement: The script-assets coverage table
This table SHALL be the single list of the Bevy items this capability covers, each with its TS name and the test that proves it. A row marked Later names its owner.

| Bevy item | TS name | Test |
|---|---|---|
| `AssetServer::load::<A>` | `server.load(path, A?)` | `script_assets.rs` `a_texture_loads_and_reaches_loaded` |
| `LoadState` / `AssetServer::load_state` | `server.loadState(h)` | `script_assets.rs` `a_texture_loads_and_reaches_loaded`, `a_missing_file_fails_with_bevys_error` |
| `DependencyLoadState` / `dependency_load_state` | `server.dependencyLoadState(h)` | `script_assets.rs` `a_gltf_waits_for_its_dependencies` |
| `RecursiveDependencyLoadState` / `recursive_dependency_load_state` | `server.recursiveDependencyLoadState(h)` | `script_assets.rs` `a_gltf_waits_for_its_dependencies` |
| `is_loaded`, `is_loaded_with_direct_dependencies`, `is_loaded_with_dependencies` | `server.isLoaded`, `isLoadedWithDirectDependencies`, `isLoadedWithDependencies` | `script_assets.rs` `a_gltf_waits_for_its_dependencies` |
| `Handle::Strong`, drop | `AssetRef` from `load`/`add`, `assets.drop(ref)` | `script_assets.rs` `a_dropped_mesh_nothing_holds_is_freed`, `a_dropped_mesh_an_entity_draws_stays` |
| `Handle::Uuid` | `AssetRef` from reflect-assets' `assets.add` | `script_assets.rs` `a_uuid_ref_keeps_its_asset_after_drop` |
| `AssetId` | `ref.id` | `script_assets.rs` `a_ref_id_is_bevys_asset_id` |
| `Assets::remove` | `assets.remove(ref)`, `meshes.remove(ref)` | `script_assets.rs` `remove_frees_at_once` |
| `AssetEvent::{Added, Modified, Removed, Unused, LoadedWithDependencies}` and `is_*` | `AssetEvent.isAdded`, `isModified`, `isRemoved`, `isUnused`, `isLoadedWithDependencies` | Not yet. Owner: script-assets task 1.4a, on the `MessageReader<AssetEvent<A>>` reader rust-events landed, with `script_assets.rs` `a_load_is_seen_as_an_event`, now ignored. The events themselves: `a_mesh_update_is_seen_as_modified`, `a_dropped_mesh_nothing_holds_is_freed` (read in Rust) |
| `Mesh::new`, `PrimitiveTopology`, `RenderAssetUsages` | `Mesh.new`, `PrimitiveTopology.*`, `RenderAssetUsages.*` | `script_assets.rs` `a_quad_matches_the_same_rust_calls` |
| `Mesh::insert_attribute`, `remove_attribute`, `attribute` and the eight `ATTRIBUTE_*` | `insertAttribute`, `removeAttribute`, `attribute`, `Mesh.ATTRIBUTE_*` | `script_assets.rs` `every_attribute_round_trips` |
| `Indices::{U16, U32}`, `insert_indices`, `remove_indices` | `Indices.U16`, `Indices.U32`, `insertIndices`, `removeIndices` | `script_assets.rs` `a_quad_matches_the_same_rust_calls`, `every_attribute_round_trips` |
| `compute_normals`, `compute_flat_normals`, `compute_smooth_normals`, `compute_area_weighted_normals` | same names in camelCase | `script_assets.rs` `computed_normals_match_rust` |
| `generate_tangents` | `generateTangents()` | `script_assets.rs` `computed_normals_match_rust`, `tangents_need_normals_and_uvs` |
| `duplicate_vertices`, `count_vertices` | `duplicateVertices()`, `countVertices()` | `script_assets.rs` `computed_normals_match_rust` |
| `Assets::<Mesh>::add`, `get`, `get_mut` | `meshes.add`, `get`, `getMut` | `script_assets.rs` `a_mesh_update_is_seen_as_modified` |
| `MeshAccessError` | thrown with Bevy's text | `script_assets.rs` `render_world_only_data_cannot_be_edited` |
| `Mesh` as reflected asset data | `assets.get`, `assets.set`, `Mesh.of` | `script_assets.rs` `a_mesh_reads_back_as_bevy_named_json`; `reflect_coverage.rs` |
| `Mesh` from TSX vertex data | `<mesh positions ...>` | `bevy-ts-tsx/tests/it/vertex_mesh.rs`; `packages/bevy-ts/test/tsx/elements.tsx` (`tsc`) |
| Chunk remesh cost | `getMut` + `insertAttribute` | `mesh_update_bench.rs` |
| `MeshVertexAttribute::new` (custom attributes), morph targets | — | Later: a change that adds custom attributes with script materials |
| `AssetEvent` / `AssetLoadFailedEvent` delivery | `MessageReader<AssetEvent<A>>` | rust-events |
| Loader settings, image samplers, per-type fields | — | reflect-assets |
| `Gltf` named parts | — | Later: reflect-assets' path or script-gltf |

#### Scenario: Every row has a test or an owner
- **WHEN** the change is archived
- **THEN** every row names a test that exists in the repository, or is marked Later or names another change
