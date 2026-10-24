# Assets from scripts

Scripts build, read and edit any asset type Bevy reflects (`StandardMaterial`, `Image`,
`ColorMaterial`, `TextureAtlasLayout`, ...) through `assets`, and put an asset in any `Handle<T>`
slot. Fields are Bevy's own, written as the reflected writer takes them (docs/reflection.md):
colours as `"#f80"`, enums as `"Variant"` or `{ Variant: data }`, structs merged field by field.
The code is `crates/bevy-ts/src/reflect_asset.rs`.

```tsx
import { assets } from "bevy-ts";
import { Image, MeshMaterial3d, StandardMaterial } from "bevy-ts/bevy";

const mat = assets.add(StandardMaterial, { base_color: "#ffd7a8", unlit: true });
assets.set(mat, { perceptual_roughness: 0.3 });
assets.get(mat)?.perceptual_roughness; // 0.3

<entity>
  <MeshMaterial3d value={mat} />
</entity>
```

## The calls

- `assets.add(Type, props)` builds the asset from its type's default with `props` written over it
  and returns its ref, an `AssetRef<Type>`. A bad field or value throws naming the type and field,
  and adds nothing. A type with no default (`Mesh`) is refused: `bevy-ts/bevy` gives it no `of`,
  and `add` rejects it at the type level.
- `assets.get(ref)` reads the asset as JSON, or `undefined` once it is gone. Each call is one host
  call and a full read: keep values in JS rather than reading every frame.
- `assets.set(ref, patch)` writes `patch` over the asset's current value. A patch that changes
  nothing marks nothing changed: no `AssetEvent::Modified`, no re-prepare.

They work inside a system's run and during a TSX render, and throw anywhere else.

`add` and `set` are visible to the script at once: `get` in the same run reads them, and a ref
works in a slot in the same run. The asset reaches `Assets<T>`, and `AssetEvent::Added` or
`Modified` is sent, when the run ends, before the run's commands apply. A run that throws still
adds its assets: their refs were handed out.

## Refs in slots

A ref goes in any `Handle<T>` slot the reflected writer reaches: a TSX prop
(`<MeshMaterial3d value={mat} />`), a field of another asset
(`assets.add(StandardMaterial, { base_color_texture: img })`) or a script's reflected insert. A ref
of another asset type fails naming both types. A handle a script made reads back as its ref; any
other handle reads as its path, or `null`.

## Lifetime

A ref is `{ $asset: n, type }`; `n` counts up from 1 and is never reused in a session. Script-made
assets live until the script reloads or stops:

- After a hot reload every ref is stale: using one throws "stale asset ref". The assets stay for
  the entities the reload kept, until the next restart. The reloaded script adds its assets again,
  so under `bevy-ts dev` each hot reload keeps one more copy of them until a restart or Stop; a
  shipped game never hot reloads.
- A restart or a Stop removes every script-made asset.

Adding an asset every frame grows memory until reload, and a warning is logged past 10,000 assets:
add once and `set` after. Freeing an asset sooner is script-assets' handle lifecycle, not here.

## Meshes

A system that takes `meshes: ResMut<Assets<Mesh>>` builds and updates meshes with Bevy's `Mesh`
API under Bevy's names. The param is the engine's `Meshes` export (`bevy-ts/assets`,
`crates/bevy-ts-cli/src/assets.rs`).

```ts
import type { Assets, AssetRef, MeshData, ResMut } from "bevy-ts";
import { Indices, Mesh, PrimitiveTopology, RenderAssetUsages } from "bevy-ts/assets";

let chunk: AssetRef<MeshData> | undefined;

export function remesh(meshes: ResMut<Assets<Mesh>>) {
  const { positions, normals, uvs, indices } = buildSurface(); // Float32Array, Float32Array, Float32Array, Uint32Array
  if (!chunk) {
    chunk = meshes.add(
      Mesh.new(PrimitiveTopology.TriangleList, RenderAssetUsages.default())
        .insertAttribute(Mesh.ATTRIBUTE_POSITION, positions)
        .insertAttribute(Mesh.ATTRIBUTE_NORMAL, normals)
        .insertAttribute(Mesh.ATTRIBUTE_UV_0, uvs)
        .insertIndices(Indices.U32(indices)),
    );
    return;
  }
  meshes.getMut(chunk)!
    .insertAttribute(Mesh.ATTRIBUTE_POSITION, positions)
    .insertAttribute(Mesh.ATTRIBUTE_NORMAL, normals)
    .insertAttribute(Mesh.ATTRIBUTE_UV_0, uvs)
    .insertIndices(Indices.U32(indices));
}
```

- `Mesh.new(topology, usage)` makes a mesh that records its calls in JS; nothing crosses until
  `meshes.add(mesh)`, which builds it in Rust and adds it in one go, returning a strong ref. A
  failed step adds nothing.
- `insertAttribute` takes the eight built-in `Mesh.ATTRIBUTE_*`: a `Float32Array` of the
  attribute's `components` per vertex (3, 3, 2, 2, 4, 4, 4), and a `Uint16Array` of 4 for
  `ATTRIBUTE_JOINT_INDEX`. `insertIndices` takes `Indices.U16(Uint16Array)` or
  `Indices.U32(Uint32Array)`. Also `removeAttribute`, `removeIndices`, `computeNormals`,
  `computeFlatNormals`, `computeSmoothNormals`, `computeAreaWeightedNormals`, `generateTangents`,
  `duplicateVertices`, `countVertices`, and `attribute(attr)` and `indices()`, which return copies.
- `meshes.getMut(ref)` is the live mesh: each call is one host call into the asset. Data of the
  same length and format as what is there is copied into the mesh's own buffer, with no
  allocation. However many calls change a mesh in a frame, Bevy sends one
  `AssetEvent::Modified`, and re-uploads the whole mesh once. `meshes.get(ref)` is the same mesh,
  read-only; both are `undefined` once the mesh is gone. `meshes.remove(ref)` is Bevy's
  `Assets::remove`.
- What Bevy refuses throws with Bevy's text and the method's name, and leaves the mesh as it was:
  a wrong array or length, normals on a topology other than `TriangleList`, flat normals on an
  indexed mesh, smooth normals without indices, an index past the last vertex, tangents without
  normals or UVs, and data of a `RENDER_WORLD`-only mesh once it has gone to the render world.

### Cost

A 24,000-vertex chunk (positions, normals, UVs, colours, 36,000 `U32` indices) remeshed with
`getMut` costs 45 µs median from TS against 29 µs for the same copies in Rust (Apple M5 Max;
`docs/research/script-assets.md`). Remeshing an edited chunk every frame from TS is fine. Keep
one mesh per chunk and `getMut` it: a changed vertex count costs a little more (allocation), and
adding a new mesh per edit makes a new asset every time; if you do, `assets.drop` the old ref.
