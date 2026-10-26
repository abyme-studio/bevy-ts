# Assets from scripts

Scripts build, read and edit any asset type Bevy reflects (`StandardMaterial`, `Image`,
`ColorMaterial`, `TextureAtlasLayout`, ...) through `assets`, and put an asset in any `Handle<T>`
slot. Fields are Bevy's own, written as the reflected writer takes them (docs/reflection.md):
colours as `"#f80"`, enums as `"Variant"` or `{ Variant: data }`, structs merged field by field.
The code is `crates/bevy-ts/src/reflect_asset.rs`.

```tsx
import { assets } from "bevy-ts";
import { MeshMaterial3dStandardMaterial, StandardMaterial } from "bevy-ts/bevy";

const mat = assets.add(StandardMaterial, { base_color: "#ffd7a8", unlit: true });
assets.set(mat, { perceptual_roughness: 0.3 });
assets.get(mat)?.perceptual_roughness; // 0.3

<entity>
  <MeshMaterial3dStandardMaterial value={mat} />
</entity>
```

## The calls

- `assets.add(Type, props)` builds the asset from its type's default with `props` written over it
  and returns its ref, an `AssetRef<Type>`. A bad field or value throws naming the type and field,
  and adds nothing. A type with no default and no entry of its own is refused: `bevy-ts/bevy` gives
  it no `of`, and `add` rejects it at the type level. `Mesh` has an entry (below).
- `assets.add(value)` makes an asset value (`T.of(props)`, a built `AnimationGraph` or
  `AnimationClip`) its own asset;
  `assets.set(ref, value)` writes the value's props.
- `assets.get(ref)` reads the asset as JSON, or `undefined` once it is gone. Each call is one host
  call and a full read: keep values in JS rather than reading every frame.
- `assets.set(ref, patch)` writes `patch` over the asset's current value. A patch that changes
  nothing marks nothing changed: no `AssetEvent::Modified`, no re-prepare.

A hand-mapped type (below; `AnimationGraph` and `AnimationClip`, [animation](animation.md)) is
read and written whole by its entry, not field by field: a patch replaces each top-level key it
gives.

They work inside a system's run, during a TSX render and while the script loads, and throw
anywhere else (a timer callback).

`add` and `set` are visible to the script at once: `get` in the same run reads them, and a ref
works in a slot in the same run. The asset reaches `Assets<T>`, and `AssetEvent::Added` or
`Modified` is sent, when the run ends, before the run's commands apply. A run that throws still
adds its assets: their refs were handed out.

## A mesh as JSON

Bevy does not reflect a mesh's data, so `Mesh` has its own entry (`crates/bevy-ts/src/mesh_data.rs`):
`assets.get`, `assets.set`, `assets.add(Mesh, data)` and `Mesh.of(data)` take Bevy-named JSON.

```json
{ "primitive_topology": "TriangleList", "asset_usage": "MAIN_WORLD | RENDER_WORLD",
  "attributes": { "Vertex_Position": [[0, 0, 0], [1, 0, 0], [0, 1, 0]], "Vertex_Uv": [[0, 0], [1, 0], [0, 1]] },
  "indices": { "U32": [0, 1, 2] } }
```

- An attribute is keyed by Bevy's name (`Mesh::ATTRIBUTE_POSITION.name`); its values are a number
  per vertex for a one-number format, else an array per vertex. `indices` is `{ U16 }`, `{ U32 }`
  or `null`. A new mesh is an empty `TriangleList` in both worlds under the keys given.
- `set` replaces each top-level key it gives whole: `{ attributes: { Vertex_Position } }` leaves a
  mesh with positions only. Read, change and set the whole `attributes` to keep the others.
- Attributes of different lengths, and an index past the last vertex, throw and change nothing.
- `Float16` attributes read but cannot be written. Morph targets are kept by a `set` that keeps the
  topology and dropped by one that changes it.
- For a mesh that changes each frame, `ResMut<Assets<Mesh>>` (below) is cheaper: JSON costs a full
  read or write per call.

## Refs in slots

A ref goes in any `Handle<T>` slot the reflected writer reaches: a TSX prop
(`<MeshMaterial3dStandardMaterial value={mat} />`), a field of another asset
(`assets.add(StandardMaterial, { base_color_texture: img })`) or a protocol write
(`bevy-ts/mutate_components`). A ref of another asset type fails naming both types. A handle a script made reads back as its ref; any
other handle reads as its path, or `null`.

## Asset values

`T.of(props)` is an asset as a value: a plain `{ $of, props }` that touches nothing until it is
written into a `Handle<T>` slot. Slots given the same type and props share one asset, compared by
the props' JSON with keys sorted, built on the first write, so a thousand enemies with
`StandardMaterial.of({ base_color: "#f80" })` make one material. A value is built only when its
slot is written (an insert or a changed prop), never per frame.

```tsx
<MeshMaterial3dStandardMaterial value={StandardMaterial.of({ base_color: "#f80", unlit: true })} />
```

`<material>` is a `StandardMaterial.of` of its props (docs/materials.md), so
`<material color="#f80" />` shares with the value above minus `unlit`. Only props `<material>`
passes through as they are share this way; docs/materials.md names the rest. A value whose slot
re-renders with other props takes the asset of those props; the old one stays for any slot still
using it.

`assets.set` on a shared asset changes every slot that shares it, `<material>` elements included.
To edit one entity's asset alone, give it its own with `assets.add`.

## Images and loader settings

`Image` is hand-mapped by its sampler alone: its pixels and texture descriptors are opaque to
reflection. `assets.add(Image, { sampler })` builds Bevy's default 1 x 1 image with that sampler,
`assets.get` reads `{ sampler }`, and `assets.set` changes the sampler and keeps the pixels. The
sampler is Bevy's `ImageSampler` in the writer's shape: `"Default"` (the `ImagePlugin`'s), or
`{ Descriptor: { address_mode_u: "Repeat", mag_filter: "Nearest", ... } }`. Unset fields keep the
current sampler's, or for a new one Bevy's default plugin sampler, `ImageSamplerDescriptor::linear()`:
linear filters, clamped to the edge. Name `mag_filter: "Nearest"` (and `min_filter`) for pixel art.

`Image.load(path, settings)` is a path loaded with loader settings, for any `Handle<Image>` slot:

```tsx
import { Image } from "bevy-ts/bevy";

const atlas = Image.load("textures/atlas.png", { sampler: { Descriptor: { address_mode_u: "Repeat", address_mode_v: "Repeat" } } });
const normals = Image.load("textures/crate_n.png", { is_srgb: false });
assets.add(StandardMaterial, { base_color_texture: atlas, normal_map_texture: normals });
```

- `settings` takes `sampler` and `is_srgb` (Bevy's `ImageLoaderSettings`); any other key fails.
  `Image.load(path)` with none is the plain path.
- Only `Image` has `load`: a `$load` in another type's slot fails naming the type.
- Bevy loads a path once: a later load of it with other settings gets the first load's image, and
  logs a warning naming both settings. Load one file one way.
- `Image.load` is a value like `T.of`: it loads when its slot is written, and reads back as its
  path.

## Loading and load state

A system that takes `server: Res<AssetServer>` gets Bevy's `AssetServer` (the engine's
`ScriptAssetServer` export, `crates/bevy-ts-cli/src/assets.rs`):

```ts
import type { AssetRef, AssetServer, Res } from "bevy-ts";
import { Mesh } from "bevy-ts/assets";

let fox: AssetRef<unknown> | undefined;
export function watch(server: Res<AssetServer>) {
  fox ??= server.load("models/fox.glb");
  if (server.isLoadedWithDependencies(fox)) { /* ... */ }
}
```

- `server.load(path)` is Bevy's load: a strong ref, typed by the loader of the path's extension.
  `server.load(path, Mesh)` names the type; a labelled path (`"fox.glb#Mesh0/Primitive0"`) needs
  it and throws without. Loading a path again gives the same ref.
- `loadState`, `dependencyLoadState` and `recursiveDependencyLoadState` give Bevy's variants by
  name: `"NotLoaded"`, `"Loading"`, `"Loaded"`, or `{ Failed: <Bevy's error text> }`.
  `isLoaded`, `isLoadedWithDirectDependencies` and `isLoadedWithDependencies` are Bevy's.
- Each takes an `AssetRef` or an `AssetId`: `ref.id`, or the id an `AssetEvent<A>` carries.
- Each is one host call; a failed state makes one more for its text.

## Lifetime

A ref is `{ $asset: n, type }`; `n` counts up from 1 and is never reused in a session. A ref goes
stale when the script drops it, removes it, or a hot reload applies; using a stale ref throws "stale asset ref".

- A ref from `server.load` or `meshes.add` holds Bevy's strong handle, so the asset stays while the
  script holds it, used or not. `assets.drop(ref)` lets go: Bevy sends `AssetEvent::Unused` and
  frees the asset once no component holds it either. Once a hot reload applies, it drops every
  strong ref the old script held; a reload that fails keeps them working and drops what it made. A
  restart or Stop drops them all; what an entity still uses stays.
  `commands.spawn` is deferred, so dropping a ref in the same system that spawns with it makes
  the spawn fail with "stale asset ref": drop it in a later system or frame.
- `assets.remove(ref)` (and `meshes.remove(ref)`) is Bevy's `Assets::remove`: the asset goes when
  the run ends (`meshes.remove`: at once), even while a component holds it, which then draws
  nothing.
- A script-made asset from `assets.add` or `T.of` is a UUID handle, as Bevy's are, and lives until
  restart or Stop: `drop` forgets the ref and keeps the asset. After a hot reload the reloaded
  script adds its assets again, so under `bevy-ts dev` each hot reload keeps one more copy of them
  until a restart or Stop; a shipped game never hot reloads.

`ref.id` is the asset's `AssetId<A>` as JSON, the value an `AssetEvent<A>` carries; it is one host
call; read it before `assets.drop`, and the value stays valid after. Compare two with
`JSON.stringify`. A system reads the events with `MessageReader<AssetEvent<A>>`, and each has
Bevy's `isAdded`, `isModified`, `isRemoved`, `isUnused` and `isLoadedWithDependencies`, taking a
ref or an id:

```ts
import { AssetEvent, Image } from "bevy-ts/bevy";

let tex: AssetRef<unknown> | undefined;
export function watch(server: Res<AssetServer>, events: MessageReader<AssetEvent<Image>>) {
  const ref = (tex ??= server.load("textures/grid.png"));
  events.each((e) => {
    if (e.isLoadedWithDependencies(ref)) console.log("ready");
    if ("Added" in e) console.log(server.loadState(e.Added.id!));
  });
}
```

An id knows its asset type, which its JSON leaves out: only an id from `ref.id` or an event works
here, and one of another asset type throws.

Adding an asset every frame grows memory, and a warning is logged past 10,000 refs: add once and
`set` or `getMut` after, or `drop` what you replace.

## Hand-mapped types

An asset type reflection cannot reach whole (`Image`, `GizmoAsset`, `AnimationGraph`, a material
crate's own type) is hand-mapped from Rust, by the crate that owns it (bevy-ts maps `Image`). `T` needs only `Asset`, no
`Reflect`:

```rust
use bevy_ts::prelude::*;

app.init_asset::<Note>().ts_hand_map::<Note>(
    |_slots, _source, note| Ok(serde_json::json!({ "text": note.text })),
    |_slots, base, json| {
        let mut note = base.cloned().unwrap_or_default();
        note.text = json["text"].as_str().ok_or("text is a string")?.into();
        Ok(note)
    },
);
```

Then `assets.add`, `get`, `set`, `T.of(props)` and refs reach `T` through the two functions, by
`T`'s type path. `write` gets the current asset as its base for a `set`, and none for an `add` or
a value. Both get `Slots`, whose `world` is the app's world for a map that reads its resources
(`ScriptMaterial`'s finds its kind there, [materials.md](materials.md#owned-instances)); it is
`None` in a load check and where a component is written in place. `read` gets the JSON a script
last wrote the asset from, `None` for a loaded asset. The built `T` goes whole into `Assets<T>` at its ref's id when the run ends. `set` merges its patch
over the current read at the top level: a key given replaces that key whole; when the new read
equals the old, nothing is queued. A type both hand-mapped and reflected uses its hand map.
Mapping a type again replaces its entry. The map is process-wide, like a type registration.

`bevy-ts/bevy` has classes only for types registered with `ReflectAsset`, so a script names a type
with no `Reflect` by its type path, with `asset` from `bevy-ts`, or the owning crate ships that
class:

```ts
import { asset, assets } from "bevy-ts";

const Note = asset<{ text: string }>("my_crate::Note");
const note = assets.add(Note, { text: "hi" });
```

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
  failed step adds nothing. The recorded calls hold your typed arrays, not copies: change an
  array before `meshes.add` and the mesh gets the change (Bevy copies at `insert_attribute`).
- `insertAttribute` takes the eight built-in `Mesh.ATTRIBUTE_*`: a `Float32Array` of the
  attribute's `components` per vertex (3, 3, 2, 2, 4, 4, 4), and a `Uint16Array` of 4 for
  `ATTRIBUTE_JOINT_INDEX`. `insertIndices` takes `Indices.U16(Uint16Array)` or
  `Indices.U32(Uint32Array)`. Also `removeAttribute`, `removeIndices`, `computeNormals`,
  `computeFlatNormals`, `computeSmoothNormals`, `computeAreaWeightedNormals`, `generateTangents`,
  `duplicateVertices`, `countVertices`, and `attribute(attr)` and `indices()`, which return copies.
- `meshes.getMut(ref)` is the live mesh: each call is one host call into the asset. Data of the
  same length and format as what is there is copied into the mesh's own buffer, with no
  allocation. However many calls one system makes on a mesh in a frame, Bevy
  sends one `AssetEvent::Modified` and re-uploads the whole mesh once; a second system that
  changes it sends another, as a second `get_mut` does in Rust. `meshes.get(ref)` is the same mesh,
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
