## Context

- `<mesh shape>` (mesh-primitives, in the land batch on 2026-10-24) builds every Bevy 0.20
  surface primitive from a `MeshProps` union in `crates/bevy-ts-tsx/src/intrinsics.rs`, cached by
  props in `IntrinsicAssets`. No path takes vertex data.
- reflect-assets (in flight) gives `assets.add/get/set` over `ReflectAsset`, the `ScriptAssets`
  ref table (ref number to `Handle::Uuid`), `AssetRef<T>`, the `HandleSlots` hook and a list of
  hand-mapped whole-type entries. It refuses `Mesh` (no default, data `reflect(ignore)`) and
  leaves load state and handle lifecycle to "engine-core", which has no change; this one takes
  them. Its refs keep their assets until stop.
- writable-native-params (archived on its branch, landing) lets an exported param write
  (`ResMut<Assets<Mesh>>`), fetches it at the TS system's ticks, and lets an export claim a Bevy
  spelling with `stands_for`, so a TS system writes `meshes: ResMut<Assets<Mesh>>`.
- rust-events (in flight) delivers `MessageReader<AssetEvent<A>>` for every `ReflectAsset` type,
  payloads read by `reflect_value::read`.
- The host path (`crates/bevy-ts/src/host.rs`) passes up to four slices per method as views over
  JS memory, `Float32Array`, `Uint16Array`, `Uint32Array` among them. Results are numbers,
  vectors, records and `Option`s. The engine already ships one built-in exported param,
  `Cameras` in `bevy-ts/camera` (`crates/bevy-ts-cli/src/camera.rs`).
- Bevy 0.20: `Handle` is `Strong(Arc)` or `Uuid`; no `Weak` (an `AssetId` is the non-owning
  reference). `AssetEvent` has `is_added`, `is_modified`, `is_removed`, `is_unused`,
  `is_loaded_with_dependencies`. `Mesh` has eight built-in attributes, `Indices::{U16, U32}`, the
  four `compute_*_normals`, `generate_tangents` (`bevy_mikktspace` is in `Cargo.lock`) and
  `try_*` forms that return `MeshAccessError` instead of panicking.

## Goals / Non-Goals

**Goals:**
- Load, watch and let go of assets from TS with Bevy's names.
- Build and update meshes from TS with Bevy's `Mesh` API, at a cost close to Rust's.
- `Mesh` covered by the reflection hole check.

**Non-Goals:**
- Per-type asset fields, samplers, loader settings (reflect-assets).
- `AssetEvent` delivery (rust-events). This change adds the `is*` methods and the id equality.
- Custom vertex attributes, morph targets, `Mesh2d`, the `Gltf` asset's parts.
- Sharing JS memory with Bevy's `Vec` (see decision 5).

## Decisions

1. **Built-in exports with Bevy's spellings.** `crates/bevy-ts-cli/src/assets.rs` exports two
   params in module `bevy-ts/assets`, beside `camera.rs`:
   `ScriptAssetServer` with `stands_for = "Res<AssetServer>"`, and `ScriptMeshes` with
   `stands_for = "ResMut<Assets<Mesh>>"`. A TS system writes Bevy's types; the reader resolves
   the claim. Both are in every engine.
   *Alternative:* methods on reflect-assets' `assets` object. Rejected: `assets` carries JSON,
   and Bevy's unit for this work is the param.

2. **One ref table, strong entries.** reflect-assets' `ScriptAssets` keeps the table and already
   stores an `UntypedHandle` per ref (`Made { handle, reflect }`); ref numbers are `u64`, from 1,
   never reused. This change adds an entry kind, `Uuid | Strong`, and `insert_strong(handle) ->
   u64`, `drop(n)` and `remove(world, n)` beside it (agreed with build@reflect-assets,
   2026-10-24). `server.load` and `meshes.add` insert strong handles; a strong entry is never
   queued and has no `ReflectAsset` value. `drop` removes the entry; Bevy's own handle drop sends
   `Unused` and frees the asset when no component holds a clone. `remove` calls `Assets::remove`
   (through `ReflectAsset` for any type) and removes the entry. No new session tag: a hot reload
   runs reflect-assets' `clear(world, false)`, which now also drops the strong entries; UUID
   entries keep reflect-assets' rule (kept until restart or stop). Stop drops and removes all.
   Script-made assets (`assets.add`, `.of`) stay UUID handles (the call has only `&World`), so
   their growth across hot reloads is dev-only and freed on restart; `docs/assets.md` says so.
   *Alternative:* JS `FinalizationRegistry` to drop on GC. Rejected: GC timing is not
   deterministic, and a test could not say when an asset goes. Bevy's drop is explicit in
   effect too (scope end), so `assets.drop` is the honest spelling.

3. **`AssetRef` is a host kind.** `Kind::AssetRef(type_path)`; Rust type `ScriptRef<A>(u64)`, one
   slot holding the ref number. JS glue unwraps `{ $asset: n }` on the way in, checking its type
   against the method's `A`, and wraps the number on the way out into the same frozen object
   `assets` returns. The param resolves the number through `ScriptAssets` it holds. Any native
   crate can then take or return handles. `bevy-ts types` declares `AssetRef<A>`.
   *Alternative:* return a bare number and wrap in a hand-written module. Rejected: every
   handle-taking export would hand-write the same wrapper.

4. **Load state crosses as a code.** Each state method returns a small integer; on the failed
   code, the glue makes one more call that copies the error's text into a `Uint8Array` and
   builds `{ Failed: text }`. Two calls only on failure. The type of `load` comes from the second
   argument (a generated asset class) or from `AssetServer`'s loader for the extension, then
   `load_builder().load_erased(type_id, path)`. A labelled path needs the type: the loader of the
   file's extension names the file's type, not the label's.

5. **The mesh data path: typed arrays, one copy, no per-vertex call.**
   - `Mesh.new(...)` and its `insertAttribute`/`insertIndices`/`compute*` calls on an unadded
     mesh record into a JS object: the typed arrays and an op list. Nothing crosses yet.
   - `meshes.add(mesh)` crosses once per attribute plus one for indices and one to finish: each
     call passes one typed array as a `&[f32]`/`&[u16]`/`&[u32]` view in place, which Rust copies
     into a fresh `Vec<[f32; N]>` (the view is already cast by
     `host::slice_ref`). The finish call runs the recorded `compute*`/`generateTangents` in
     order with Bevy's `try_*` forms, then `Assets::add`. A failure adds nothing.
   - `meshes.getMut(ref)` returns a live handle object: each method is one host call on
     `Assets::get_mut`. `insertAttribute` with the same length and format as the present
     attribute copies into the existing `Vec` (`attribute_mut`, `copy_from_slice`), no
     allocation; otherwise it inserts a new `Vec`. `insertIndices` likewise. Several calls in one
     run mark the asset changed once, so Bevy sends one `Modified` and re-uploads once.
   - Copy, not borrow: Bevy's `Mesh` owns `Vec`s that outlive the run and are extracted to the
     render world; JS memory can move or be collected. One `memcpy` per attribute is the floor
     short of building the mesh in Rust.
   - Measured by `crates/bevy-ts-cli/tests/mesh_update_bench.rs` (ignored by default, run by
     hand through gate-slot): a 32×32×32 voxel chunk surface, about 24,000 vertices with
     positions, normals, UVs and `U32` indices. Arms: (a) Rust-only `get_mut` + the same copies,
     the floor; (b) TS `getMut` + four `insertAttribute` + `insertIndices`, same length; (c) TS
     with a changed vertex count (allocation); (d) TS `Mesh.new` + `meshes.add` of 64 chunks;
     (e) `assets.set` of `Mesh` JSON, the path to avoid. Each reports median and p95 µs per
     remesh over 3 runs of 200 frames, and ns per vertex. Target: (b) within 1.5× of (a) plus
     5 µs of host calls. Numbers go in `docs/research/script-assets.md` and the tasks.

6. **`Mesh` as asset data: a hand-mapped entry.** reflect-assets' list takes whole-type entries
   `{ asset, read, write }`. `Mesh`'s entry reads and writes Bevy-named JSON:
   `primitive_topology` and `asset_usage` by their reflected shapes, `attributes` keyed by
   Bevy's attribute name (`Vertex_Position`), arrays of tuples by format, `indices` as
   `{ U16 }`/`{ U32 }`. Its default is an empty `TriangleList` mesh with both usages. This gives
   `assets.get`, `assets.set`, `Mesh.of` and the hole check's coverage; its `UNCOVERED` entry
   goes. If the hole check does not yet treat a hand-mapped type as covered, that filter is asked
   of reflect-enums-color (owner of the walker) through the lead; this change does not edit the
   walker.

7. **`<mesh>` vertex data builds like `Mesh.of`.** `MeshProps` gains a member keyed by
   `positions` (`crates/bevy-ts-tsx/src/vertex_mesh.rs`); each prop is one of Bevy's `Mesh` calls,
   and the compute ops run through `bevy_ts::mesh_data`'s checked `MeshOp`s, the ones
   `ResMut<Assets<Mesh>>` runs. It is cached as the shapes are, in `IntrinsicAssets` by its props,
   so a material needing tangents gets the tangent variant and errors name the TSX prop, not
   Bevy's attribute name. A large mesh hashes its props once per insert or changed prop, never per
   frame. The parse picks the member by `positions` before serde's `shape` tag; mesh-primitives'
   members are unchanged.

8. **`AssetEvent` methods.** The runtime adds `isAdded`, `isModified`, `isRemoved`, `isUnused`
   and `isLoadedWithDependencies` to rust-events' generic `AssetEvent` class. `ref.id` is read by
   the same `reflect_value::read` of `AssetId<A>` that rust-events uses for payloads, so the
   methods compare two values of one shape, field by field.

## Seams

| Seam | Owner | This change |
|---|---|---|
| `ScriptAssets`, `AssetRef`, `HandleSlots`, hand-mapped list | reflect-assets | asks for `UntypedHandle` entries; adds strong inserts, `drop`, `remove`, the `Mesh` entry |
| Writable params, `stands_for` | writable-native-params | uses them |
| `AssetEvent` delivery and its TS class | rust-events | adds `is*` methods to the class |
| `MeshProps` union, primitives | mesh-primitives | adds the vertex-data member |
| `reflect_value::UNCOVERED`, the walker | reflect-enums-color | deletes the `Mesh` entry; asks for a hand-mapped filter if missing |

## Ask-first items

1. **Architecture.** A new host kind (`AssetRef`), and refs that own strong handles freed on
   `drop` and on hot reload, changing reflect-assets' "kept until stop" for strong refs.
   Alternative: no handle kind (each export wraps by hand); UUID-only refs (no `Unused`, no
   freeing before stop, so a chunked world leaks every replaced chunk).
2. **Performance.** The remesh path costs one copy per attribute and Bevy's full re-upload on
   `Modified`; arm (b) is measured against the Rust floor before docs recommend it. Fallback if
   it misses: build chunk meshes in a native crate and pass the ref to TS through the
   `AssetRef` kind.

## Risks / Trade-offs

- [A script never drops what it adds] → memory grows until a reload or stop; docs say to drop,
  or to `getMut` one mesh instead of adding a new one per edit; a warning at 10,000 live refs
  (reflect-assets' threshold).
- [`remove` of a mesh an entity draws leaves a dangling handle] → Bevy's behaviour, which draws
  nothing; documented.
- [Full re-upload per `Modified`] → Bevy's; partial upload is not in Bevy 0.20.
- [Glue wraps every ref crossing] → a frozen object per returned ref; refs are few per frame.
- [reflect-assets lands after this is ready] → stage 1 waits on it; the plan does not fork the table.

## Migration Plan

Additive. Strong refs are new; reflect-assets' UUID refs keep their rule. Rollback is a revert per stage.

## Open Questions

None. The two ask-first items wait on the operator's go.
