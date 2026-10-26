# Script materials

A custom material is data: a params schema in TS and WGSL in a `.wgsl` file. The prebuilt engine
draws it with no Rust and no project engine. The code is `crates/bevy-ts-material`; the TS module
is `bevy-ts/material`. Render passes, compute and other render-world work stay Rust, in the native
crate.

`bevy-ts dev examples/cube/src/glow.tsx` runs the cube example with one, `game::Glow`, and
`node examples/cube/glow-walk.mjs` walks editing it live.

## Declaring one

```tsx
import { color, f32, material } from "bevy-ts/material";
import glow from "./glow.wgsl";

export const Glow = material("game::Glow", { params: { color: color, pulse: f32 }, fragment: glow });

<entity>
  <mesh shape="cuboid" />
  <material kind={Glow} color="#ff8800" pulse={2} />
</entity>
```

- `params` take `f32`, `i32`, `u32`, `vec2f`, `vec3f`, `vec4f`, `mat4x4f`, `color` (linear RGBA,
  written in any colour form: `"#f80"`, sRGB `[r, g, b, a?]`, `srgb(...)`, `linear(...)`,
  `hsl(...)` or Bevy's reflected `Color`, as in [native.md](native.md#reflected-values-in-tsx))
  and `array(of, len)`. Unset params are zero;
  colors, and the elements of a color array, are white.
- `textures: { noise: texture2d }` adds a 2D texture and a filtering sampler; the prop is an asset
  path (`noise="textures/noise.png"`). Unset, it binds a white fallback. Param and texture names are
  WGSL identifiers, not reserved words and not starting with `__`; a texture's name and its
  `<name>_sampler` may not be a param, `params`, the struct's name or another texture's. Otherwise
  the load fails at the `material()` call.
- `fragment` (required) and `vertex` are `.wgsl` imports, resolved relative to the importing file
  and watched. A WGSL string is rejected. `alphaMode` is `"opaque"` (default), `"blend"`,
  `"premultiplied"`, `"add"`, `"multiply"`, `"alphaToCoverage"` or `{ mask: 0.5 }`. The fragment's
  colour is premultiplied: with `"add"` it is added as returned, so its alpha does not dim it; scale
  the colour itself to fade an additive glow.
- Equal props share one material instance. New props take another instance, never write the
  shared one, so no other element's material changes. An instance no entity holds is freed at the
  end of the frame. Params and textures cannot be bindings; instance params can
  ([below](#instance-params)).
- The shape is a `<mesh>` ([meshes.md](meshes.md)).
- `kind`, `asset` and `gltfMaterial` are props of `<material>` itself, so no param, texture or
  instance param may take any of those names.

## Owned instances

An instance one script owns and writes in place, for values that change often:

```tsx
import { app, assets, Update, type Local, type u32 } from "bevy-ts";
import { color, f32, material } from "bevy-ts/material";
import glow from "./glow.wgsl";

export const Glow = material("game::Glow", { params: { color: color, pulse: f32 }, fragment: glow });
export const g = Glow.add({ color: "#ff0000", pulse: 1 });

// Every entity given g draws the new pulse: the same asset, written in place.
export function beat(frames: Local<{ n: u32 }>) {
  frames.n += 1;
  assets.set(g, { pulse: 1 + (frames.n % 60) / 30 });
}
app.addSystems(Update, beat);

app.scene("main", () => (
  <entity>
    <mesh shape="cuboid" />
    <material asset={g} />
  </entity>
));
```

- `Kind.add(props)` makes one `ScriptMaterial` outside the shared cache and returns its asset ref
  (docs/assets.md). Call it in a system, a render or while the script loads. Its schema goes with
  it, so one added while the script loads is encoded before the generation's kinds apply.
- `assets.set(g, patch)` writes the patch over the instance at the top level and keeps its
  `AssetId`: meshes keep their handle. A patch that changes nothing marks nothing changed. A name
  the kind lacks, or another `kind`, fails with the kind's names.
- `assets.get(g)` reads `{ kind, ...params, ...textures }`: params decoded from the uniform bytes,
  colours as `{ LinearRgba: { red, green, blue, alpha } }`, textures as their paths or `null`.
- `<material asset={g} />` takes only `asset` and `gltfMaterial`. A ref of another asset type, or
  any other prop, fails the load at the element. A stale ref (refs go stale when the script
  reloads) is an error at the element, and the mesh keeps what it had.
- A changed params schema re-encodes owned instances by param name, as it does shared ones.
- It lives as other script-made assets do: kept for its entities over a hot reload, removed on a
  restart or stop.
- A write is not free. Each changed frame makes a new uniform buffer and bind group, and every
  entity drawing it re-specialises from the pipeline cache: no shader compiles, no new pipeline.
  A texture changed to an unloaded image draws nothing until it loads.
- From Rust, the write is `bevy_ts_material::owned`, registered with `app.ts_hand_map`.

## Instance params

Values per entity that keep one material and one batch, for hover, team colour or damage flashes:

```tsx
import { app, signal } from "bevy-ts";
import { color, f32, material } from "bevy-ts/material";
import glow from "./glow.wgsl";

export const Glow = material("game::Glow", { params: { color: color }, instance: { hover: f32 }, fragment: glow });
export const [hovered, setHovered] = signal(0);

app.scene("main", () => (
  <>
    <entity><mesh shape="sphere" /><material kind={Glow} color="#f80" hover={0} /></entity>
    <entity><mesh shape="sphere" /><material kind={Glow} color="#f80" hover={() => hovered()} /></entity>
  </>
));
```

```wgsl
import bevy_pbr::render::forward_io::VertexOutput;

@fragment
fn fragment(in: VertexOutput) -> @location(0) vec4<f32> {
    return params.color * (1.0 + instance(in.instance_index).hover);
}
```

- `instance` takes the types `params` takes, laid out by the same rules, at most 256 bytes. Its
  names may not be a param's, a texture's or its sampler's, `instance`, `instances` or the
  generated `<Name>Instance`; otherwise the load fails at the `material()` call.
- Instance props on `<material kind>` go to the entity's `ScriptInstance`, never to the asset or
  its key: both spheres above hold one `ScriptMaterial`. Unset values are zero, colours white.
- They may be bound (`hover={() => hovered()}`): a new value writes the entity, never the asset.
  Params and textures still fail the load when bound.
- The WGSL gets, appended, `struct GlowInstance { hover: f32, ... }`, a read-only storage array
  `instances` after the textures, and `fn instance(i: u32) -> GlowInstance`, read as
  `instance(in.instance_index)`. bevy-ts also imports Bevy's `get_tag` at the start of the file's
  first line (as `bevy_ts_get_tag`), so errors on that line keep their columns.
- On a glTF scene root, each mesh it swaps takes the root's values and follows them.
- An owned instance (`<material asset>`) takes no instance props; its entities read the defaults.
- Limits: bevy-ts packs every entity's values into one storage buffer and gives each entity a
  `MeshTag`, its row. An entity may have one `MeshTag`: one bevy-ts did not set is kept, the
  entity takes no instance params, and an error names the entity and element. A tag set after
  bevy-ts gave the entity its row is not caught, and the entity then draws the row it names: set
  none on an entity with instance params. A kind's own prepass or shadow shader cannot read
  instance values. Any change uploads the whole buffer once that frame, 256 bytes per entity
  (2.5 MB at 10k).

## What lives where

| Field | Where it lives | Set by | Written in place | Bindable | Batches with others |
|---|---|---|---|---|---|
| `params` | the asset's uniform | keyed props, or `Kind.add`/`assets.set` | owned only | no | equal asset only |
| `textures` | the asset's bind group | keyed props, or `Kind.add`/`assets.set` | owned only | no | equal asset only |
| `instance` | the entity's `ScriptInstance` | instance props | yes | yes | yes, one asset |
| `alphaMode` | the kind | `material()` | no (hot reload) | no | per kind |
| Fade, `alpha` | `MaterialAlpha`, `StandardMaterial` only | not on script materials | n/a | n/a | n/a |

## On a glTF scene

On an entity that spawns a glTF scene, the material draws the scene's meshes:

```tsx
import { WorldAssetRoot } from "bevy-ts/bevy";

<entity>
  <WorldAssetRoot {...{ "0": "models/ship.glb#Scene0" }} />
  <material kind={MaskTint} gltfMaterial="Hull" base="textures/ship.png" mask="textures/mask.png" colors={[a, b, c]} />
</entity>
```

- Once the scene is ready, each mesh it spawned swaps its `StandardMaterial` for the script
  material. `gltfMaterial` limits that to meshes whose glTF material has exactly that name; without
  it, every mesh takes it. Meshes on other materials, and TSX children of the entity, keep theirs.
- New props reach every swapped mesh the same frame. A new `gltfMaterial` puts back the meshes
  that stop matching. Removing the `<material>` puts back every mesh's own material. A scene
  spawned again (its file saved) is swapped again.
- From Rust: `ScriptMaterial3d` on the root, and `GltfMaterialFilter("Hull")` for the name. The
  swapped meshes and their own materials are in `SceneMaterialTargets` on the root.

## The WGSL file

Bevy 0.20 compiles material shaders as WESL, so a file imports Bevy's modules by path:

```wgsl
import bevy_pbr::render::forward_io::VertexOutput;
import bevy_pbr::render::mesh_view_bindings::globals;

@fragment
fn fragment(in: VertexOutput) -> @location(0) vec4<f32> {
    return params.color * (0.5 + 0.5 * sin(globals.time * params.pulse));
}
```

bevy-ts appends the declarations after your last line, so the file does not write them:

```wgsl
struct GlowParams { color: vec4<f32>, pulse: f32 }
@group(3) @binding(0) var<uniform> params: GlowParams;
@group(3) @binding(1) var noise: texture_2d<f32>;   // per texture: the texture,
@group(3) @binding(2) var noise_sampler: sampler;   // then its sampler
```

Params follow WGSL's uniform rules: `vec3f` aligns to 16, the struct rounds to 16, arrays stride
to 16. Scalar and `vec2f` array elements are declared as `vec4`, so read `params.w[i].x`. This
layout is separate from components, which keep C rules.

## Hot reload and errors

- Saving a `.wgsl` file replaces the shader of every material that names it, in place. No JS runs
  and no generation starts.
- A changed params schema rebuilds the kind and re-encodes every instance by param name, as
  components migrate by field name. A param that is gone is dropped, with a warning. If the
  shaders no longer check out under the new schema (a renamed param the file still reads), the
  kind keeps drawing as it was and the error is reported; saving the fixed `.wgsl` then applies
  the new schema with no TS change. Until then, a save the old schema accepts still updates the
  shader that draws. A newly added `vertex` file is checked the same way, under the defs the
  fragment draws with.
- Before a save replaces a shader that draws, bevy-ts checks it: it compiles the file with
  Bevy's modules and the shader defs the live pipeline uses, then validates it with naga. An error
  names the material and your `file:line:col`, and the previous shader keeps drawing. An error in
  the generated part names the material's schema in TS instead. With no shader drawing yet, the
  renderer's error is mapped the same way and the material draws nothing until fixed.
- A failure the check cannot see (a device capability such as `f64` on Metal) is reported with
  the renderer's own message. A renderer failure from a source older than the current one is
  never reported against the current file.

## Alpha and fading

`<material>` without `kind` is Bevy's `StandardMaterial`: `color`, `metallic`, `roughness`,
`emissive`, `unlit`, `alpha` and `alphaMode`.

```tsx
<material color="#ff0000" alpha={0.5} />
```

- `alpha` (0 to 1) replaces the colour's alpha. A colour written with alpha (`"#ff000080"`) keeps
  its own when `alpha` is unset. Outside 0 to 1, the load fails at the element.
- `alphaMode` takes a script material's names. Unset, it is `"blend"` when the alpha is below 1
  and `"opaque"` otherwise, as Bevy's `StandardMaterial::from(Color)`.

These set a shape's resting look; they cannot be bound. To fade over time, put `MaterialAlpha`
(from `bevy-ts/bevy`) on the entity and bind its field:

```tsx
import { MaterialAlpha } from "bevy-ts/bevy";

<entity>
  <mesh shape="cuboid" />
  <material color="#ff0000" />
  <MaterialAlpha alpha={() => k()} />
</entity>
```

- It fades the entity's own `MeshMaterial3d<StandardMaterial>` and, on a glTF scene root
  (`WorldAssetRoot`), every mesh its scene spawned, again whenever the scene is ready. TSX children
  are not faded; give each its own. A mesh drawing a script material (above) is left alone.
- Each faded mesh draws with a copy of its material owned by the entity: a `FadeMaterial`
  (`ExtendedMaterial<StandardMaterial, Fade>`), its base a clone of the original. While faded, the
  mesh holds `MeshMaterial3d<FadeMaterial>` and no `MeshMaterial3d<StandardMaterial>`. Other
  entities sharing the material are not faded. A changed `alpha` edits the copies in place; it
  makes no asset. A material changed under it (a TS save, a glTF saved again) is faded again.
- The copy dithers: it keeps the original's alpha mode and draws about `alpha` (clamped to 0..1)
  of its pixels, picked by a 4×4 ordered (Bayer) map on screen, as Bevy's `VisibilityRange`
  cross-fade does. Depth stays written, so a model fades as one solid, its inner faces hidden. At
  1 it draws as the original; at 0, nothing.
- Alpha steps in sixteenths, and the pattern shows: we run no TAA to smooth it.
- Its shadow follows: the shadow pass drops the same pixels, so the shadow lightens as `alpha`
  falls and is gone at 0. `MaterialAlpha` never touches `NotShadowCaster`; one on a mesh keeps it
  from casting at any alpha.
- An original that blends already (`blend`, `premultiplied`, `add`, `multiply`) keeps its mode and
  fades through its colour: its alpha times `alpha`, no dither on screen. Only its shadow dithers.
- Removing it gives every mesh its own material back and drops the copies. Remove it when not
  fading: while it is on, even at 1, the entity's meshes do not batch with others sharing their
  material, a discard costs their early depth test, and their shadow runs a fragment shader.
- The first fade of each distinct material compiles its pipelines: a visible hitch, about half a
  second on an M5 Max for a Synty building (main's `Blend` fade cost about 0.3 s). Fade it once
  off-screen, or early, to warm it.

## Known limits

- `MaterialAlpha`'s fade on screen needs forward rendering. Under `DeferredPrepass` a faded mesh
  draws solid; only its shadow fades.
- The check skips files that use inline module paths (`a::b::c(...)`) or `@if` outside the
  imports; those reach the renderer unchecked, and its failures are reported with its message.
- The check validates each stage alone, with every capability. A vertex output that does not
  match the fragment's input fails at pipeline creation, which Bevy does not catch: wgpu treats it
  as a device error.
- A renderer failure is reported a frame after it happens: Bevy may apply a saved shader a frame
  late, so failures wait one frame to be sure they belong to the current source.

## How it works

One Rust asset type, `ScriptMaterial`, implements `ErasedRenderAsset` by hand (Bevy's
`manual_material.rs`). Every kind's instances sit behind its one non-bindless bind group allocator,
which takes each allocation's layout, so kinds with different layouts coexist. Each kind gets its
own material key, or kinds would share one specialized pipeline.

On a scene root, `scene.rs` reacts to `WorldInstanceReady` and to changes on the root, as
scene-animation does. It walks the instance's own entities (`iter_instance_entities`), not the
root's descendants, which is how TSX children stay untouched. That walk is
`bevy_ts_material::scene_entities(world, root)`. Every swap runs in `SceneMaterialSet`
(`PostUpdate`), so other per-root work on a scene's meshes can run after it.

`MaterialAlpha` is `alpha.rs`, with its shader `fade.wesl`. It runs after `SceneMaterialSet` on
the same walk, so a mesh scene-material swapped holds neither material and is skipped. `alpha.rs`
is the one place that knows a faded mesh's original: when scene-material swaps a faded mesh, it
asks `alpha::standard_of` and keeps the original, and when it puts that back under a faded root,
`MaterialAlpha` fades it again. Each removal, in either order, leaves what the other still asks.

The shadow pass runs a material's prepass fragment shader only when it may discard, which an
opaque base does not; `Fade::specialize` gives those pipelines `fade.wesl`'s prepass fragment, so
the shadow drops the same pixels.

## Standard materials

`<material>` without `kind` is Bevy's `StandardMaterial`. Every field but the 18 blocked below is a
prop, under its Bevy name in camelCase, two of them under shorter names: `color` is `base_color` and
`roughness` is `perceptual_roughness`. An unset prop keeps `StandardMaterial::default()`. The
element is the asset value `StandardMaterial.of` of its props, mapped onto Bevy's fields by one
table (`MATERIAL_PROPS` in `bevy-ts-tsx`): equal props share one material with every other
`<material>`, and with a `Handle<StandardMaterial>` slot given the same value (docs/assets.md).
`<material color="#fff" roughness={0.3} />` and
`StandardMaterial.of({ base_color: "#fff", perceptual_roughness: 0.3 })` are one material. Only
props the table passes through as they are share with a script value: `emissive`, the UV channels,
`parallaxMappingMethod`, `opaqueRenderMethod`, `uvTransform`, `alpha` and `alphaMode` are written in
Bevy's read shape, so `<material emissive="#f00" />` is not `StandardMaterial.of({ emissive: "#f00" })`.
A texture path is the `Image.load` value of it, so `<material baseColorTexture="a.png" />` is
`StandardMaterial.of({ base_color_texture: Image.load("a.png") })`. A material with `cullMode`
shares only with other elements: Bevy does not reflect it, so it is set in Rust after the write.

```tsx
<entity>
  <mesh shape="cuboid" />
  <material baseColorTexture="textures/crate.png" normalMapTexture="textures/crate_n.png" roughness={0.8} />
</entity>
```

| Kind | Props |
|---|---|
| Colour (as `color`) | `color`, `emissive`, `specularTint`, `attenuationColor` |
| Number | `emissiveExposureWeight`, `roughness`, `metallic`, `reflectance`, `diffuseTransmission`, `specularTransmission`, `thickness`, `ior`, `attenuationDistance`, `clearcoat`, `clearcoatPerceptualRoughness`, `anisotropyStrength`, `anisotropyRotation` (radians), `depthBias`, `parallaxDepthScale`, `maxParallaxLayerCount`, `lightmapExposure`, `deferredLightingPassId` (0 to 255) |
| Boolean | `unlit`, `flipNormalMapY`, `doubleSided`, `fogEnabled` |
| sRGB texture | `baseColorTexture`, `emissiveTexture` |
| Linear texture | `metallicRoughnessTexture`, `normalMapTexture`, `occlusionTexture`, `depthMap` |
| UV channel, `"uv0"` or `"uv1"` | `baseColorChannel`, `emissiveChannel`, `metallicRoughnessChannel`, `normalMapChannel`, `occlusionChannel` |
| `cullMode` | `"back"` (Bevy's default), `"front"` or `null` (no culling) |
| `opaqueRenderMethod` | `"forward"`, `"deferred"` or `"auto"` |
| `parallaxMappingMethod` | `"occlusion"` or `{ relief: { maxSteps: 5 } }` |
| `uvTransform` | `{ scale?: [x, y], rotation?: radians, translation?: [x, y] }`, Bevy's `Affine2::from_scale_angle_translation` |

`alpha` replaces `color`'s alpha, 0 to 1, and `alphaMode` names the mode; unset, it is `"blend"` below alpha 1 and `"opaque"` otherwise (Alpha and fading, above).

- A texture is an asset path, loaded by the `AssetServer`, or anything a `Handle<Image>` slot takes:
  a ref, `Image.of(...)` or `Image.load(path, settings)` (docs/assets.md). A path is
  `Image.load(path)` for the colour textures (base colour, emissive), which load as sRGB, and
  `Image.load(path, { is_srgb: false })` for the rest, data loaded linear as Bevy's glTF loader
  does. One file keeps the settings of its first load, so keep colour and data textures in
  separate files. A path that fails to load is an error at the element, naming the path, and the mesh stays undrawn (Bevy draws no material with an unloaded texture). A saved texture
  redraws by Bevy's asset reload.
- A two-sided leaf needs both `doubleSided` and `cullMode={null}`, as in Bevy: `doubleSided` alone
  lights the back face but still culls it.
- `uvTransform` with a scale above 1 tiles only with a repeating sampler; a path samples with
  Bevy's default sampler, which clamps to the edge. Offsets, rotation and sub-rectangles work
  either way. See Tiling textures.
- `normalMapTexture` or a non-zero `anisotropyStrength` give the entity's `<mesh>` vertex tangents, which Bevy needs for them. The
  tangent mesh is built once per shape and shared; any other mesh stays as it is.
- Bad values fail the load at the element: an unknown prop or enum name, a non-finite number, a bad
  colour, a malformed `uvTransform`, an empty texture path, or a texture with no `AssetServer`.

### Tiling textures

A texture repeats under a `uvTransform` scale above 1 when its image's sampler repeats. Load it with
one: `Image.load`'s `sampler` is Bevy's `ImageSampler` in the writer's shape, its fields unset
keeping Bevy's default plugin sampler, `ImageSamplerDescriptor::linear()` (linear filters). Pixel art
names its filters.

```tsx
import { Image } from "bevy-ts/bevy";

// A pixel-art atlas, tiled four times across the face.
const atlas = Image.load("textures/atlas.png", {
  sampler: { Descriptor: { address_mode_u: "Repeat", address_mode_v: "Repeat", mag_filter: "Nearest", min_filter: "Nearest" } },
});

<entity>
  <mesh shape="cuboid" />
  <material unlit baseColorTexture={atlas} uvTransform={{ scale: [4, 4] }} />
</entity>
```

A path keeps the settings of its first load, so a file tiled in one place and clamped in another
needs two files.

### Blocked on Bevy (16-sampler default limit)

These 18 `StandardMaterial` fields are not props. Bevy compiles them only behind four features
(`pbr_transmission_textures`, `pbr_specular_textures`, `pbr_multi_layer_material_textures`,
`pbr_anisotropy_texture`), and with those on, Bevy's opaque PBR pipeline binds 18 sampled textures
and 18 samplers to the fragment stage, set or not. wgpu's default limit is 16, so a device held
to the defaults rejects every standard material, glTF ones included. Without the features it binds
9. The features cost no measurable frame or GPU time. Numbers, and the command:
[bench/standard-material-features/results](../bench/standard-material-features/results/README.md).
`crates/bevy-ts-cli/tests/it/standard_material_limits.rs` checks the pipeline fits the defaults.
Per platform: [docs/research/standard-material-texture-limit.md](research/standard-material-texture-limit.md).
Related upstream: [bevyengine/bevy#26028](https://github.com/bevyengine/bevy/issues/26028) (PCSS
passes the same limit). No upstream issue names these features yet (TODO: link it once filed).

| Field | Feature |
|---|---|
| `diffuse_transmission_texture`, `diffuse_transmission_channel` | `pbr_transmission_textures` |
| `specular_transmission_texture`, `specular_transmission_channel` | `pbr_transmission_textures` |
| `thickness_texture`, `thickness_channel` | `pbr_transmission_textures` |
| `specular_texture`, `specular_channel` | `pbr_specular_textures` |
| `specular_tint_texture`, `specular_tint_channel` | `pbr_specular_textures` |
| `clearcoat_texture`, `clearcoat_channel` | `pbr_multi_layer_material_textures` |
| `clearcoat_roughness_texture`, `clearcoat_roughness_channel` | `pbr_multi_layer_material_textures` |
| `clearcoat_normal_texture`, `clearcoat_normal_channel` | `pbr_multi_layer_material_textures` |
| `anisotropy_texture`, `anisotropy_channel` | `pbr_anisotropy_texture` |

The scalar props for the same effects (`specularTransmission`, `thickness`, `clearcoat`,
`anisotropyStrength` and the rest) work: only their textures are blocked.
