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
add once and `set` after. Freeing an asset sooner is engine-core's handle lifecycle, not here.
