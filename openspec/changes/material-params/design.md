## Context

`research.md` read Bevy 0.20.0 and bevy-ts `0bf5e27a`; its file:line references are not repeated
here. Today `kinds::instance` keys a `ScriptMaterial` by its props' JSON and `KindState::instances`
holds a strong handle to each, forever. Props are not bindable. `ScriptMaterial` is `Asset +
TypePath` only, drawn by a hand-written `ErasedRenderAsset` on Bevy's non-bindless allocator.

lead@engine-core decided: stage 1 builds now; stage 2 is specced against reflect-assets'
`app.ts_hand_map::<T: Asset + TypePath>(read, write)` and builds after reflect-assets lands it;
stage 3 is opt-in, refuses a user `MeshTag`, and builds after stage 2's 60 Hz bench.

## Goals / Non-Goals

Goals: keyed instances freed when unused; one owned asset written in place, same `AssetId`;
per-entity values that keep one material and one batch.

Non-goals: the `<material>` `StandardMaterial` cache's leak (`intrinsics.rs`, same cause; a
follow-up can reuse the sweep); bindless script materials; fading script materials (a kind fades
through its own instance param); a kind's own prepass shader reading instance values.

## Decisions

1. **Shared or owned.** Two kinds of instance, never mixed:
   - Keyed: `<material kind={Glow} color="#f80" />`. Props are the key. Immutable from TSX: new
     props take another key, never write the asset, so one element's change never reaches another
     element sharing the old key. The sweep frees a key nobody uses.
   - Owned: `const g = Glow.add({ color: "#f80" })`, written with `assets.set(g, patch)`, given as
     `<material asset={g} />`. One asset, written in place for every slot it is in. Never enters
     the keyed cache; lives as reflect-assets' refs do (stale after a hot reload applies, removed
     on restart or stop).
   - Per entity: `instance` params (decision 4), on the entity, not on either asset.
   *Alternative:* write a keyed asset in place when its props change. Rejected: every element
   sharing the key would change.

2. **The sweep.** A system in `PostUpdate`, after the TSX reconcile, drops each `instances` entry
   whose `Handle::Strong` Arc has a strong count of 1. Every `ScriptMaterial3d`, the clones on glTF
   meshes included, shares that Arc, so 1 means nothing draws it. Dropping the last handle makes
   Bevy free the asset and the render side's `unload_asset` free the binding. A re-render that
   removes and re-adds a key in one frame keeps it, since the sweep runs after reconcile.
   *Alternative:* `AssetEvent::Unused`. Cannot fire while the cache holds a strong handle.

3. **Owned instances through reflect-assets' hand map.** `bevy-ts-material` registers
   `app.ts_hand_map::<ScriptMaterial>(read, write)`; no edit in reflect-assets' module.
   - `read(world, &ScriptMaterial) -> Value`: `{ kind, ...params, ...textures }`, params decoded
     from the bytes by the kind's `ParamsLayout`, textures as their asset paths.
   - `write(world, base, json) -> ScriptMaterial`: builds the whole asset. `kind` is required on
     add and may not change on set. Unknown names fail with today's message
     (`<kind> has no param or texture <p>; it has ...`).
   - `assets.set` merges the patch over `read` at the top level and, when it changed, swaps the
     asset at the same handle id; meshes keep the handle. An equal patch queues nothing.
   - TS: `material()` returns a kind with `add(props): AssetRef<KindFields>`, which calls
     `assets.add` with `ScriptMaterial`'s asset class and `{ kind: name, ...props }`.
     `assets.get(ref)` returns `{ kind, ...params, ...textures }`.
   - `<material asset={ref} />` takes a `ScriptMaterial` ref and `gltfMaterial`, nothing else; a
     ref of another type, or a stale one, fails at the element. `asset` is reserved like `kind`.
   - A hot reload's schema change re-encodes owned instances by param name too: the re-encode
     walks `Assets<ScriptMaterial>` for the kind, not only the keyed map.
   - Cost per changed frame (research §1): a new uniform buffer and bind group, and every entity
     drawing it re-specialises from the pipeline cache, no shader compile. A texture changed to an
     unloaded image draws nothing until it loads.

4. **Per-entity `instance` params.** `material(name, { instance: { hover: f32 }, ... })`.
   - Types as `params`, laid out by the same `ParamsLayout` rules (they hold in a storage buffer).
     At most 256 bytes; more fails at `material()`. Names follow the param rules and may not equal
     a param, texture, sampler, `instance`, `instances`, or `<Struct>Instance`.
   - One global component, `ScriptInstance([u32; 64])`, with Bevy's `GpuComponentArrayBuffer`:
     one storage buffer, each entity's index in its `MeshTag`. Each kind with instance params adds
     a read-only storage binding after its textures and appends
     `struct GlowInstance { hover: f32, ... }`, `var<storage, read> instances: array<GlowInstance>`
     and `fn instance(i: u32) -> GlowInstance`, read as `instance(in.instance_index)`.
   - Opt-in: only an entity whose kind declares `instance` gets `ScriptInstance` (and so a
     `MeshTag`). Its values come from the element's instance props; unset is zero, colours white.
   - A `MeshTag` bevy-ts did not set: an error naming the entity and the element; the tag is kept
     and no `ScriptInstance` is inserted. Bevy-ts never overwrites a user tag.
   - Instance props may be bound (`hover={() => h()}`), as UI props are: a bound value writes the
     entity's `ScriptInstance`, never the asset. Params and textures stay unbindable.
   - On a glTF root, `scene.rs` copies the root's `ScriptInstance` to each swapped mesh, again when
     it changes, and removes it on restore.
   *Alternatives:* `MeshTag` alone (one `u32`); an own extract and draw (replaces our material
   path); a per-kind buffer of exact stride (a second tag allocator beside Bevy's). Research §2.

5. **Coverage.** `docs/materials.md` gets this table:

   | Field | Where it lives | Set by | Written in place | Bindable | Batches with others |
   |---|---|---|---|---|---|
   | `params` | the asset's uniform | keyed props, or `Kind.add`/`assets.set` | owned only | no | equal asset only |
   | `textures` | the asset's bind group | keyed props, or `Kind.add`/`assets.set` | owned only | no | equal asset only |
   | `instance` | the entity's `ScriptInstance` | instance props | yes | yes | yes, one asset |
   | `alphaMode` | the kind | `material()` | no (hot reload) | no | per kind |
   | Fade, `alpha` | `MaterialAlpha`, `StandardMaterial` only (material-fade) | not on script materials | n/a | n/a | n/a |

## Tests

Per `docs/testing.md`: modules of `crates/bevy-ts-cli/tests/it`, render apps from
`common::render_app()`, waits by condition.

- Stage 1 (`materials.rs`): a param driven by state that changes every frame for 120 frames keeps
  `Assets<ScriptMaterial>` and the kind's `instances` at most 2; two elements with equal props
  share one asset, and removing one keeps it.
- Stage 2 (`materials.rs`, `material_params_render.rs`): `assets.set` on an owned instance keeps
  its `AssetId` and the headless frame's pixel colour changes to the new one; the render
  `PipelineCache`'s pipeline count is the same before and after 60 writes; `assets.get` reads back
  the written values; an equal `set` sends no `Modified`; a stale ref or another type in
  `<material asset>` fails at the element.
- Stage 3 (`material_params_render.rs`): two entities of one kind with different `hover` draw
  different colours from one `AssetId`; a bound instance prop changes the pixels frame by frame
  with no new asset; an entity with its own `MeshTag` keeps it and reports the error; a glTF root's
  meshes take its instance values.
- Benches (`bench/material-params/`, outside the workspace, release, static): stage 2, N owned
  instances written at 60 Hz against the same values re-keyed, frame time and asset count; run
  before stage 3, numbers in the landing note. Stage 3, 10k entities with an animated instance
  param against re-keying.

## Risks / Trade-offs

- Owned writes are not free (decision 3): bind group rebuilt and re-specialisation per changed
  frame. The stage 2 bench says where it stops being fine.
- Instance buffer re-uploads whole on any change; 2.5 MB at 10k entities.
- A user `MeshTag` and instance params cannot meet on one entity.
- Prepass and shadow shaders of a kind cannot read instance values.
- `asset` becomes reserved.

## Migration Plan

Stage 1: no API change. Stage 2: additive apart from `asset` reserved. Stage 3: additive. Each
stage lands alone and reverts alone.

## Open Questions

None blocking. Whether the `<material>` `StandardMaterial` cache takes the same sweep is a
follow-up.
