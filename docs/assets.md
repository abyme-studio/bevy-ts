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
  and adds nothing. A type with no default (`Mesh`) is refused: `bevy-ts/bevy` gives it no `of`,
  and `add` rejects it at the type level.
- `assets.add(value)` makes an asset value (`T.of(props)`, a built `AnimationGraph`) its own
  asset; `assets.set(ref, value)` writes the value's props.
- `assets.get(ref)` reads the asset as JSON, or `undefined` once it is gone. Each call is one host
  call and a full read: keep values in JS rather than reading every frame.
- `assets.set(ref, patch)` writes `patch` over the asset's current value. A patch that changes
  nothing marks nothing changed: no `AssetEvent::Modified`, no re-prepare.

A hand-mapped type (`reflect_asset::HAND_MAPS`; `AnimationGraph`, [animation](animation.md))
is read and written whole by its entry, not field by field: a patch replaces each top-level key it
gives.

They work inside a system's run, during a TSX render and while the script loads, and throw
anywhere else (a timer callback).

`add` and `set` are visible to the script at once: `get` in the same run reads them, and a ref
works in a slot in the same run. The asset reaches `Assets<T>`, and `AssetEvent::Added` or
`Modified` is sent, when the run ends, before the run's commands apply. A run that throws still
adds its assets: their refs were handed out.

## Refs in slots

A ref goes in any `Handle<T>` slot the reflected writer reaches: a TSX prop
(`<MeshMaterial3dStandardMaterial value={mat} />`), a field of another asset
(`assets.add(StandardMaterial, { base_color_texture: img })`) or a protocol write
(`bevy-ts/mutate_components`). A ref of another asset type fails naming both types. A handle a script made reads back as its ref; any
other handle reads as its path, or `null`.

## Lifetime

A ref is `{ $asset: n, type }`; `n` counts up from 1 and is never reused in a session. A ref
works until the script reloads or stops; the asset can outlive it:

- Once a hot reload applies, every ref the old generation gave is stale: using one throws "stale
  asset ref". A reload that fails leaves the running generation's refs working, and drops what
  the failed one made. The assets stay for
  the entities the reload kept, until the next restart. The reloaded script adds its assets again,
  so under `bevy-ts dev` each hot reload keeps one more copy of them until a restart or Stop; a
  shipped game never hot reloads.
- A restart or a Stop removes every script-made asset.

Adding an asset every frame grows memory until a restart or Stop, and a warning is logged past 10,000 assets:
add once and `set` after. Freeing an asset sooner is script-assets' handle lifecycle, not here.

## Hand-mapped types

An asset type reflection cannot reach whole (`GizmoAsset`, `AnimationGraph`, a material crate's
own type) is hand-mapped from Rust, by the crate that owns it. `T` needs only `Asset`, no
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
a value. `read` gets the JSON a script last wrote the asset from, `None` for a loaded asset. The
built `T` goes whole into `Assets<T>` at its ref's id when the run ends. `set` merges its patch
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
