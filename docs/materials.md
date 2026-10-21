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
  written as `"#f80"` or sRGB `[r, g, b, a?]`) and `array(of, len)`. Unset params are zero; colors
  are white.
- `textures: { noise: texture2d }` adds a 2D texture and a filtering sampler; the prop is an asset
  path (`noise="textures/noise.png"`). Unset, it binds a white fallback.
- `fragment` (required) and `vertex` are `.wgsl` imports, resolved relative to the importing file
  and watched. A WGSL string is rejected. `alphaMode` is `"opaque"` (default), `"blend"`,
  `"premultiplied"`, `"add"`, `"multiply"`, `"alphaToCoverage"` or `{ mask: 0.5 }`.
- Equal props share one material instance. Props cannot be bindings yet.

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
  components migrate by field name. A param that is gone is dropped, with a warning.
- Before a save replaces a shader that draws, bevy-ts checks it: it compiles the file with
  Bevy's modules and the shader defs the live pipeline uses, then validates it with naga. An error
  names the material and your `file:line:col`, and the previous shader keeps drawing. An error in
  the generated part names the material's schema in TS instead. With no shader drawing yet, the
  renderer's error is mapped the same way and the material draws nothing until fixed.
- The check skips files that use inline module paths (`a::b::c(...)`) or `@if` outside the
  imports; those reach the renderer unchecked.

## How it works

One Rust asset type, `ScriptMaterial`, implements `ErasedRenderAsset` by hand (Bevy's
`manual_material.rs`). Every kind's instances sit behind its one non-bindless bind group allocator,
which takes each allocation's layout, so kinds with different layouts coexist. Each kind gets its
own material key, or kinds would share one specialized pipeline.
