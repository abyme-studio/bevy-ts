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
  `"premultiplied"`, `"add"`, `"multiply"`, `"alphaToCoverage"` or `{ mask: 0.5 }`.
- Equal props share one material instance. Props cannot be bindings yet.
- The shape is a `<mesh>` ([meshes.md](meshes.md)).
- `kind` and `gltfMaterial` are props of `<material>` itself, so no param or texture may take
  either name.

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
- Each faded mesh draws with a copy of its material owned by the entity, at the original's alpha
  times `alpha` (clamped to 0..1), in `Blend` below 1 when the original was opaque, masked or
  alpha-to-coverage. Other entities sharing the material are not faded. A changed `alpha` edits
  the copies in place; it makes no asset. A material changed under it (a TS save, a glTF saved
  again) is faded again.
- Removing it gives every mesh its own material back and drops the copies. Remove it when not
  fading: while it is on, even at 1, the entity's meshes do not batch with others sharing their
  material, and each changed frame prepares the copies again.
- Bevy casts no shadow from blended meshes, and sorts them per mesh, so parts of one model may
  overlap in the wrong order mid-fade.

## Known limits

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

`MaterialAlpha` is `alpha.rs`. It runs after `SceneMaterialSet` on the same walk, so a mesh
scene-material swapped has no `StandardMaterial` and is skipped. When scene-material swaps a mesh
that holds a fade copy, it keeps the copy as the mesh's own material; removing `MaterialAlpha`
rewrites those back to the originals in `SceneMaterialTargets`, so each removal, in either order,
leaves what the other still asks.
