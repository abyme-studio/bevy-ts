# material-params research

Read 2026-10-26 against Bevy 0.20.0 (`~/dev/refs/bevy-0.20`, `b3bdd75b`) and bevy-ts `0bf5e27a`.
Bevy paths are relative to that checkout. Nothing here was measured; costs are read from code.

Today `kinds::instance` (`crates/bevy-ts-material/src/kinds.rs:539`) keys a `ScriptMaterial` by
the props' JSON. New props make a new asset, and `KindState::instances` (`kinds.rs:140`) keeps a
strong handle to every one, so none is ever freed. Props are not bindable
(`crates/bevy-ts-tsx/src/resolve.rs:264`): they change on a re-render or a save.

## 1. Writing an instance in place

Recommendation: write in place with `get_mut`, only when the bytes differ; fine for a few
materials a frame, not for hundreds.

`ScriptMaterial` is not on the `AsBindGroup` path. It implements `ErasedRenderAsset` by hand on
the non-bindless allocator (`crates/bevy-ts-material/src/render.rs:153`), so what applies is:

- `Assets::get_mut` returns an `AssetMut`; `AssetEvent::Modified` is sent only on `DerefMut`
  (`crates/bevy_asset/src/assets.rs:440`, `:665`, `:680`). Compare first through `Deref`, then
  write: an equal value sends nothing.
- `Modified` re-extracts the asset by clone (`crates/bevy_render/src/erased_render_asset.rs:314`).
  Prepare first drops the old render asset (`erased_render_asset.rs:430`), then calls our
  `prepare_asset`, which frees the old binding and allocates a new one (`render.rs:188`).
- Non-bindless has no in-place data update: `try_update_data` returns false with
  `// TODO: Have a fast path for non-bindless materials` (`crates/bevy_render/src/material_bind_groups.rs:683`).
  Each modified material makes a new uniform buffer and a new bind group
  (`material_bind_groups.rs:2498`, `:2529`). Small, but GPU objects made every frame it changes.
- Re-specialisation: yes, every entity drawing it. Our `MeshOrMaterialChanged` holds
  `AssetChanged<ScriptMaterial3d>` (`render.rs:250`), as Bevy's own
  `check_entities_needing_specialization` does (`crates/bevy_pbr/src/material.rs:912`). The key is
  unchanged (type id, mesh key, `kind.id`: `render.rs:207`), so `SpecializedMeshPipelines` hits its
  cache (`crates/bevy_render/src/render_resource/pipeline_specializer.rs:202`). No shader compile, but
  per-entity work each frame the material changes.
- A texture param changed to an image not loaded yet returns `RetryNextUpdate` (`render.rs:176`)
  after the old render asset was dropped, so the mesh draws nothing until it loads. Load before
  swapping, or accept the gap.
- Bindless would give an in-place write (`try_update_data`, `material_bind_groups.rs:672`), but our
  kinds differ in layout per allocation, which is why they are non-bindless. Not worth it here.

## 2. Per-instance values

Recommendation: an `instance` schema per kind, packed by Bevy 0.20's `GpuComponentArrayBuffer`
into one storage buffer indexed by `MeshTag`.

| | MeshTag alone | Storage buffer + MeshTag (`GpuComponentArrayBuffer`) | Own extract and draw |
|---|---|---|---|
| Bevy 0.20 | `examples/shader/automatic_instancing.rs` | new in 0.20: `crates/bevy_render/src/gpu_component_array_buffer.rs`, `examples/shader/gpu_component_array_buffer.rs` | `examples/shader_advanced/custom_shader_instancing.rs` |
| Carries | one `u32` | any `Pod` struct per entity | anything |
| Shader | `mesh_functions::get_tag(instance_index)` (`crates/bevy_pbr/src/render/mesh_functions.wesl:155`) | `var<storage> data: array<T>` in the material group, read at `mesh[instance_index].tag` | own pipeline, phase item, draw commands |
| Batching | kept: one material | kept: one material, one bind group | own |
| Fits us | too small for params | yes | no: replaces the material path we use |

How the buffer works: a component with `GpuComponentArrayBuffer` is packed by
`update_components` in `PostUpdate` into one `ShaderBuffer` asset; each entity gets a `MeshTag`
that is its index; removal swap-removes and re-tags the moved entity
(`gpu_component_array_buffer.rs:130`, `:300`). A changed value re-uploads the buffer with
`write_buffer` when its size is the same (`crates/bevy_render/src/storage.rs:353`), and bind groups
are rebuilt only when it grows (`material_bind_groups.rs:2549`).

For ScriptMaterial:

- TS: `material("game::Glow", { params: {...}, instance: { hover: f32 }, fragment })`. `instance`
  props on `<material kind>` go to a component on the entity, never to the shared asset.
- Rust: `GpuComponentArrayBuffer::Out` is one static `Pod` type, and kinds differ. Use one global
  component, `ScriptInstance([u32; 64])`, so 256 bytes per entity at most, laid out by the kind's
  instance struct. Each kind's layout adds a read-only storage binding to the array's buffer
  (`UnpreparedBindingResource::ShaderBuffer`, `crates/bevy_render/src/render_resource/bind_group.rs:802`).
- Generated WGSL, appended as today (`layout.rs:347`):
  `struct GlowInstance { hover: f32, _pad: array<u32, 63> }`, `@group(3) @binding(n) var<storage, read> instances: array<GlowInstance>;`,
  `fn instance(i: u32) -> GlowInstance { return instances[mesh_functions::get_tag(i)]; }`. The
  fragment calls `instance(in.instance_index)`. `VertexOutput.instance_index` is always on in the
  main pass (`crates/bevy_pbr/src/render/mesh.rs:3401`, `forward_io.wesl:113`). The uniform layout
  rules (`ParamsLayout`) also hold in a storage buffer, so the encoder is reused as is.
- glTF roots: `scene.rs` must copy the root's `ScriptInstance` to each swapped mesh, because the
  tag is per mesh entity (`scene.rs:80`).
- Cost: 256 bytes per tagged entity (10k entities: 2.5 MB). A frame where any value changes clones
  the whole buffer to the render world (`asset_usage` main and render) and uploads it whole.
- Limits: one `MeshTag` per entity, so nothing else may set one (Bevy warns in debug,
  `gpu_component_array_buffer.rs:180`); 256 bytes of instance params per kind; storage buffers,
  which every native target we ship has; the prepass and shadow get `instance_index` only under
  `VERTEX_OUTPUT_INSTANCE_INDEX` (`crates/bevy_pbr/src/prepass/mod.rs:449`), so a kind's own
  prepass shader may not read instance values.
- A per-kind buffer of exact stride would be smaller, but it is a second tag allocator beside
  Bevy's. Start with Bevy's.

## 3. Dropping unused instances

Recommendation: each `PostUpdate`, drop the `instances` entries whose `Handle::Strong` Arc has a
strong count of 1 (only the cache holds it).

- `Handle::Strong(Arc<StrongHandle>)` is public (`crates/bevy_asset/src/handle.rs:118`). Every
  `ScriptMaterial3d`, including the clones `scene.rs` puts on glTF meshes, shares that Arc, so a count
  of 1 means no entity draws it.
- Dropping the entry drops the last handle: Bevy frees the asset and sends `Unused`
  (`assets.rs:507`), and the render side calls our `unload_asset`, which frees the binding
  (`erased_render_asset.rs:322`, `render.rs:223`).
- `AssetEvent::Unused` alone cannot work: the cache's strong handle stops it from firing. Holding
  only an `AssetId` and minting with `get_strong_handle` (`assets.rs:412`) works too, but needs
  event plumbing for the same result.
- Safe because `instance` hands its handle to the entity in the same call
  (`crates/bevy-ts-tsx/src/intrinsics.rs:516`). Run it after reconcile, so a re-render that removes
  and re-adds the same key within one frame keeps the asset.
- `<material>`'s `StandardMaterial` cache (`intrinsics.rs:378`) leaks the same way. Out of scope;
  the same sweep would fix it.

## 4. Shared and owned instances in TSX

Recommendation: keyed instances are never written in place; per-entity changes go through
`instance` params, and an asset meant to be written is made with `assets.add` and given by ref.

- Today a re-render with new props takes another key (`intrinsics.rs:516` → `kinds.rs:539`). That
  is already safe: one sphere's hover gives that sphere a new asset and leaves the others alone. The
  danger comes only if a changed prop writes the keyed asset in place, so that must not happen.
- Keyed (`<material kind={Glow} color="#f80" />`): props are the key, the asset is immutable from
  TSX, and the sweep (3) frees a key nobody uses. A hover that changes `color` re-keys: a new
  asset and bind group per distinct value, and no batching with the others.
- Per-entity (`hover={h}`, an `instance` param): the entity's own `ScriptInstance`, written in
  place, with one material for the whole crowd. Making these bindable (as UI props are,
  `resolve.rs:255`) is the cheap way to animate a hover.
- Owned or named (`assets.add(Glow, {...})`, written with `assets.set`): one asset written in place
  for every slot it is in, as reflect-assets' design documents for its cached values
  (`reflect-assets/openspec/changes/reflect-assets/design.md:118`). Given by ref, it never enters
  the keyed cache and lives as reflect-assets' refs do.

## 5. Overlap

- reflect-assets: `lead@reflect-assets` is not running, so its source and docs answered. The TS
  surface is `assets.add(type, props): AssetRef<F>`, `assets.get(ref)` and `assets.set(ref, patch)`,
  with refs `{ $asset: n, type }` (`reflect-assets/packages/bevy-ts/index.d.ts:309`, `:323`).
  `set` merges the patch and queues only a real change, applied at run end with one `Modified`
  (`reflect-assets/crates/bevy-ts/src/reflect_asset.rs:436`). `ScriptMaterial` is a non-goal there
  (`design.md:34`): `set` needs `ReflectAsset`, and per-type JSON goes through `HAND_MAPS`, a `const`
  slice in `bevy-ts` (`reflect_asset.rs:497`) that `bevy-ts-material` cannot add to.
  Recommendation: reuse its spelling and ref shape. That needs two changes in their module:
  `ScriptMaterial` registered as an opaque `ReflectAsset`, and hand maps registered at runtime, so
  `bevy-ts-material` maps params by name with its existing encoder. Agree that with reflect-assets
  before speccing.
- material-fade has landed (archived `2026-10-24-material-fade`). `MaterialAlpha` leaves script
  materials alone (`docs/materials.md`, Alpha and fading). This change adds no fade or alpha; a
  kind fades through its own `instance` param.

## Should this become a change

Yes, in this order: the sweep (3), in-place writes for owned refs (1, 4) once reflect-assets has the
two hooks (5), then `instance` params (2).
