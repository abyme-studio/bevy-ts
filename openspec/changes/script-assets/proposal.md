## Why

A TS script cannot build a mesh from its own vertices, update one cheaply (a voxel chunk remeshed
on an edit), load an asset and wait for it, or let an asset go. `<mesh>` makes Bevy's primitives
only (mesh-primitives), reflect-assets refuses `Mesh` (no default, attributes `reflect(ignore)`),
and the bevy-coverage audit marks `AssetServer.load`, load state and custom meshes "missing" with
owner `script-assets` (`docs/research/bevy-coverage.md`, "Assets and scenes" and "Render").

## What Changes

Three stages, each landable on its own, in this order.

1. **Handles and load state.** `server: Res<AssetServer>` in a TS system is Bevy's
   `AssetServer`: `load(path)` returns a strong `Handle<A>` (an `AssetRef<A>` in reflect-assets'
   ref table); `loadState`, `dependencyLoadState`, `recursiveDependencyLoadState`, `isLoaded`,
   `isLoadedWithDirectDependencies` and `isLoadedWithDependencies` return Bevy's values under
   Bevy's names. `assets.drop(ref)` drops the script's strong handle, so Bevy sends `Unused` and
   frees the asset once no component holds it; `assets.remove(ref)` is `Assets::remove`. `ref.id`
   is the asset's `AssetId<A>`, the value `AssetEvent<A>` carries, and `AssetEvent` gains Bevy's
   `isAdded`, `isModified`, `isRemoved`, `isUnused` and `isLoadedWithDependencies`. A hot reload
   drops the strong handles the old script held. A new host kind carries an `AssetRef` across an
   exported method.
2. **`ResMut<Assets<Mesh>>` and Bevy's `Mesh` API in TS.** `Mesh.new(PrimitiveTopology,
   RenderAssetUsages)`, `insertAttribute` for every `Mesh.ATTRIBUTE_*` (position, normal, uv 0
   and 1, tangent, colour, joint weight, joint index), `insertIndices(Indices.U16(...))` /
   `Indices.U32`, `computeNormals`, `computeFlatNormals`, `computeSmoothNormals`,
   `computeAreaWeightedNormals`, `generateTangents`, then `meshes.add(mesh)`. `meshes.getMut(h)`
   updates a live mesh in place. Data crosses as typed arrays: one host call per attribute, one
   copy into Bevy's own `Vec`, no call per vertex. A bench measures chunk remeshing.
3. **`<mesh>` from vertex data, and `Mesh` off the hole list.** `<mesh positions indices uvs
   normals ...>` builds a mesh from props, cached by content as `<mesh shape>` is. `Mesh` gets a
   whole-type entry on reflect-assets' hand-mapped list, so `assets.get(mesh)` reads it,
   `Mesh.of({...})` is an asset value, and `bevy_mesh::mesh::Mesh` leaves
   `reflect_value::UNCOVERED`. A coverage table in the spec lists every Bevy item, its TS name
   and its test.

**Out of scope:** per-type asset fields, image samplers and loader settings (reflect-assets);
delivery of `AssetEvent` and `AssetLoadFailedEvent` to TS readers (rust-events); custom vertex
attributes (`MeshVertexAttribute::new`), morph targets, `Mesh2d` (script-2d); the `Gltf` asset's
named parts (the audit listed it here; it is a per-type field set, so reflect-assets' path or
script-gltf).

## Capabilities

### New Capabilities

- `script-assets`: asset handles and load state from TS, handle drop and removal, asset ids and
  events; meshes built and updated from TS through `Assets<Mesh>` and Bevy's `Mesh` API; the
  coverage table.

### Modified Capabilities

- `jsx-runtime`: `<mesh>` also builds a mesh from vertex data props.

## Impact

- Code: new `crates/bevy-ts-cli/src/assets.rs` (the `AssetServer` and `Assets<Mesh>` exports,
  beside `camera.rs`), `crates/bevy-ts/src/host.rs` (the `AssetRef` kind),
  `crates/bevy-ts/src/reflect_asset.rs` (strong entries, `drop`, `remove`, the `Mesh` entry),
  `crates/bevy-ts-tsx/src/intrinsics.rs` (`MeshProps` vertex-data member),
  `crates/bevy-ts-v8/js/core.js` and `packages/bevy-ts/runtime/core.js` (`Mesh` builder,
  `Indices`, `PrimitiveTopology`, `RenderAssetUsages`, `AssetEvent` methods),
  `crates/bevy-ts-cli/src/types.rs` (the `Mesh` class members), `packages/bevy-ts/index.d.ts`,
  `jsx-runtime.d.ts`, the `bevy-ts/assets` module's d.ts, `bevy.d.ts` regenerated.
- Tests: new `crates/bevy-ts-cli/tests/script_assets.rs`, `crates/bevy-ts-tsx/tests/vertex_mesh.rs`,
  `crates/bevy-ts-cli/tests/mesh_update_bench.rs`; `tests/typescript.rs` cases;
  `tests/reflect_coverage.rs` passes with the `Mesh` entry gone.
- Docs: `docs/assets.md` (handles, load state, drop, meshes), `docs/native.md` (the `AssetRef`
  kind), `docs/research/bevy-coverage.md` rows updated.
- Protocol: none expected; if the handshake changes, the next free minor at landing.
- Depends on: writable-native-params (`ResMut` params, `stands_for`) and reflect-assets stage 1
  (`ScriptAssets`, `AssetRef`, the `HandleSlots` hook, the hand-mapped list) before stage 1;
  rust-events for the event scenarios; mesh-primitives' `MeshProps` union before stage 3.
- Seams moved: reflect-assets names "engine-core" as owner of handle lifecycle and load state;
  there is no such change, so this change owns them. reflect-assets keeps the ref table; this
  change asks it to hold `UntypedHandle` (strong or UUID) per ref.
- No new dependency or service.
- Operator's go (ask-first), detail in design "Ask-first items":
  1. Architecture: a new host kind, `AssetRef`, so an exported method takes and returns handles;
     and refs that own strong handles, freed on `drop` and on hot reload (reflect-assets keeps
     every asset until stop).
  2. Performance: the remesh path (one copy per attribute, Bevy's re-upload of the whole mesh on
     `Modified`); the bench reports numbers before anything relies on it.
