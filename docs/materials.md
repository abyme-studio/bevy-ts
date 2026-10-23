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
  written as `"#f80"` or sRGB `[r, g, b, a?]`) and `array(of, len)`. Unset params are zero;
  colors, and the elements of a color array, are white.
- `textures: { noise: texture2d }` adds a 2D texture and a filtering sampler; the prop is an asset
  path (`noise="textures/noise.png"`). Unset, it binds a white fallback. Param and texture names are
  WGSL identifiers, not reserved words and not starting with `__`; a texture's name and its
  `<name>_sampler` may not be a param, `params`, the struct's name or another texture's. Otherwise
  the load fails at the `material()` call.
- `fragment` (required) and `vertex` are `.wgsl` imports, resolved relative to the importing file
  and watched. A WGSL string is rejected. `alphaMode` is `"opaque"` (default), `"blend"`,
  `"premultiplied"`, `"add"`, `"multiply"`, `"alphaToCoverage"` or `{ mask: 0.5 }`.
- `shadows` and `prepass` say whether the kind casts shadows and draws in a camera's depth and
  normal prepass, as Bevy's `Material::enable_shadows` and `enable_prepass`. Both default to true
  for an opaque kind with no `vertex`, and to false otherwise: both passes draw with Bevy's default
  prepass shader, which would cast the shadow of the unmoved mesh and fill in cut-outs. Set either
  to `true` to take that anyway. Per entity, Bevy's `NotShadowCaster` and `NotShadowReceiver` apply.
- Equal props share one material instance. Props cannot be bindings yet.
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

## Toon and ink

A painted, cel-shaded, outlined look is a script material and a camera; no engine feature.
docs/research/toon-look.md has the reasoning and a three-panel render.

The camera, in TSX:

```tsx
import { Camera3d, DepthPrepass, NormalPrepass, Projection, Tonemapping, Transform } from "bevy-ts/bevy";

<entity name="camera">
  <Camera3d />
  <Projection value={{ Orthographic: { scaling_mode: { FixedVertical: { viewport_height: 13 } } } }} />
  <DepthPrepass />
  <NormalPrepass />
  <Tonemapping value="None" />
  <Transform translation={[0, 20, 14]} rotation={[-0.4617, 0, 0, 0.887]} />
</entity>
```

`Tonemapping value="None"` lands the palette exactly. The prepass markers are only for ink.

The material: cel bands from the sun's shadowed N·L, and ink where the prepass jumps. Under an
orthographic camera depth is linear, so one `depth_k` holds across the screen.

```wgsl
import bevy_pbr::render::forward_io::VertexOutput;
import bevy_pbr::render::mesh_view_bindings::{view, lights};
import bevy_pbr::render::shadows::fetch_directional_shadow;
import bevy_pbr::prepass::utils::{prepass_depth, prepass_normal};

@fragment
fn fragment(in: VertexOutput) -> @location(0) vec4<f32> {
    let n = normalize(in.world_normal);
    let sun = lights.directional_lights[0];
    let view_z = dot(vec4<f32>(view.view_from_world[0].z, view.view_from_world[1].z, view.view_from_world[2].z, view.view_from_world[3].z), in.world_position);
    let shadow = fetch_directional_shadow(0u, in.world_position, n, view_z, in.position.xy);
    let l = max(dot(n, sun.direction_to_light), 0.0) * shadow;
    // Two soft cel steps: shade, mid, lit. A ramp texture sampled by `l` works too.
    let b = 0.5 * smoothstep(0.08, 0.14, l) + 0.5 * smoothstep(0.55, 0.61, l);
    var c = mix(params.color.rgb * params.shade.rgb, params.color.rgb, b);

    // Ink: compare depth and normal with four neighbours, clamped to the viewport.
    let p = in.position;
    let d0 = prepass_depth(p, 0u);
    let n0 = prepass_normal(p, 0u);
    var edge = 0.0;
    let offs = array<vec2<f32>, 4>(vec2(1.5, 0.0), vec2(-1.5, 0.0), vec2(0.0, 1.5), vec2(0.0, -1.5));
    for (var i = 0; i < 4; i++) {
        let q = vec4<f32>(clamp(p.xy + offs[i], view.viewport.xy + 0.5, view.viewport.xy + view.viewport.zw - 0.5), p.zw);
        let dd = abs(prepass_depth(q, 0u) - d0) * params.depth_k;
        let dn = 1.0 - dot(prepass_normal(q, 0u), n0);
        edge = max(edge, max(smoothstep(0.4, 0.8, dd), smoothstep(0.25, 0.5, dn)));
    }
    c = mix(c, params.ink.rgb, edge * 0.9);
    return vec4<f32>(c, 1.0);
}
```

with `params: { color: color, shade: color, ink: color, depth_k: f32 }`, a cool `shade` (as a
painter glazes shadows) and `depth_k` near 400 for a scene some tens of units deep.

- Only toon-material meshes get ink: a `StandardMaterial` mesh writes the prepass but does not
  read it. Every outlined thing uses the toon kind.
- Stair-stepped shadow edges: raise `DirectionalLightShadowMap` size, use one cascade sized to the
  view, or widen the `smoothstep`.
- Cost, 100 animated skinned foxes at 1080p on an M5 Max (`bench/toon/results/2026-10-23.md`):
  3.5 ms a frame with `StandardMaterial`, 4.2 ms toon, 4.5 ms toon with prepass and ink. The ink's
  texel loads are per pixel, not per character.

## Known limits

- The check skips files that use inline module paths (`a::b::c(...)`) or `@if` outside the
  imports; those reach the renderer unchecked, and its failures are reported with its message.
- The check validates each stage alone, with every capability. A vertex output that does not
  match the fragment's input fails at pipeline creation, which Bevy does not catch: wgpu treats it
  as a device error.
- Prepass and shadows use Bevy's default prepass shader; a kind cannot bring its own yet (Bevy's
  `prepass_vertex_shader`/`prepass_fragment_shader`); change `script-material-prepass-shaders`.
- A script material on a glTF scene root gives every mesh one instance, so meshes lose their own
  textures; change `scene-material-textures`.
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

Each kind registers the prepass and shadow draws a typed material gets and sets its own
`shadows_enabled`/`prepass_enabled`; Bevy's `prepass_specialize` is private in 0.20, so the crate
keeps a copy.
