# StandardMaterial's 18 blocked texture fields: can bevy-ts turn them on?

Question from `standard-material-fields` (2026-10-24): with Bevy's four `pbr_*_textures` features
on, `pbr_opaque_mesh_pipeline` binds 18 textures and 18 samplers to the fragment stage
(`bench/standard-material-features/results/README.md`). Read from Bevy 0.20 and wgpu 30.0.1
source; no builds, no hardware other than the M5 Max.

Answer: keep them off. With them on, every M1 and M2 Mac, every Intel Mac, and Linux on GL fail
every `StandardMaterial`. darwin-arm64, the one platform shipping, is among them.

## 1. Limits: we already ask for the adapter's

bevy-ts never sets `WgpuSettings` outside tests, so it gets Bevy's default:
`WgpuSettingsPriority::Functionality`, which requests `adapter.limits()`
(`bevy_render/src/renderer/mod.rs:322`), on `Backends::all()` (`settings.rs:81`). Raising limits
is not a lever: the adapter's are already in use.

What the adapters report, from wgpu-hal 30.0.1:

| Backend | Sampled textures / stage | Samplers / stage | Source |
|---|---|---|---|
| Metal, every Mac | 96 | 16, hard-coded | `metal/adapter.rs:847`, `:1418` |
| DX12 | 128 or more | 2048 (tier 1 adapters are not exposed) | `dx12/adapter.rs:243`, `:841` |
| Vulkan | the driver's `maxPerStageDescriptor*` | the driver's; 32 on llvmpipe and old Intel | `vulkan/adapter.rs:1592`, `:1648` |
| GL (Linux fallback, VMs, old GPUs) | 16 | 16 | `gles/mod.rs:135` |

On Metal the sampler cap is 16 on every Mac, the M5 too. The M5 passes only because of bindless
(below).

## 2. Bindless decides it

`StandardMaterial` is bindless-capable (`#[bindless(index_table(range(0..31)))]`). With bindless
on, its textures and samplers are binding arrays. wgpu counts those against
`max_binding_array_*_per_shader_stage`, not the per-stage limits (`wgpu-core
binding_model.rs:457`). So bindless takes all of the material's 15 textures out of the count.

Bevy turns bindless on when the device has `TEXTURE_BINDING_ARRAY | BUFFER_BINDING_ARRAY` and
`max_binding_array_sampler_elements_per_shader_stage >= sampler_binding_count * slots`
(`bevy_render_macros as_bind_group.rs:990`). `sampler_binding_count` counts each texture *and*
each sampler (`:643`, `:769`). `slots` is 64 on macOS, 2048 elsewhere (`render_resource/bindless.rs:25`).

| | Features off | Features on |
|---|---|---|
| Material textures + samplers | 6 + 6 = 12 | 15 + 15 = 30 |
| Sampler array elements needed, macOS (x64) | 768 | 1920 |
| Needed elsewhere (x2048) | 24 576 | 61 440 |

Metal's sampler-array cap (`metal/adapter.rs:864`): 500 000 on Apple9 (M3 and later), 1024 on
Apple7/8 (M1, M2) and Mac2 (Intel Macs), and bindless needs argument-buffer tier 2 and MSL 3.0
(`:1234`). That gives:

| GPU | Features off | Features on |
|---|---|---|
| M3, M4, M5 | bindless, works | bindless, works |
| M1, M2 | bindless, works | 1920 > 1024: no bindless, 18 samplers > 16: fails |
| Intel Mac, tier 2 | bindless, works | same as M1: fails |
| Intel Mac, tier 1 (integrated Intel) | 9 samplers, works | 18 > 16: fails |
| DX12 (sampler heap 2048 < 24 576, never bindless) | works | works (2048 samplers) |
| Vulkan desktop, descriptor indexing | bindless, works | bindless, works |
| Vulkan without it, GL | works | fails where the driver caps at 16 (GL always) |

Sure: the M1/M2 and Intel rows follow from source, not hardware; nobody here has an M1/M2. The
Intel tier split per GPU is from Apple's docs (tier 2 on discrete GPUs), not checked per model.
The Vulkan low end is unmeasured.

## 3. Choosing at runtime

The features are `#[cfg]` on `StandardMaterial`'s fields and its derived bind group layout, and set
shader defs from `cfg!` (`bevy_pbr render/mesh.rs:3470`). Nothing in Bevy picks them per adapter.
Two local patches could:

- Patch `bevy_pbr`: replace the derived `AsBindGroup` with a hand-written one that leaves out the 9
  extra bindings, and their shader defs, when the device cannot take them. A few hundred lines in
  `pbr_material.rs` and `render/mesh.rs`, a vendored `bevy_pbr`, redone on every Bevy release.
- Patch `bevy_render`: macOS slots 64 to 32 (30 x 32 = 960 <= 1024). M1, M2 and tier 2 Intel keep
  bindless. One constant, but a vendored `bevy_render`, smaller slabs (more bind group switches)
  on every Mac, and tier 1 Intel, GL and low Vulkan still fail.

## 4. Recommendation

Keep the features off. Bevy's own `3d` profile leaves them off too.

| Option | Lost platforms | Maintenance | Performance |
|---|---|---|---|
| Keep off (recommended) | none | none | none |
| Turn on, minimum M3 / bindless | M1, M2, all Intel Macs, Linux GL, low Vulkan | none | none measured |
| Patch `bevy_render` slots to 32 | tier 1 Intel Macs, Linux GL, low Vulkan | vendored crate, every Bevy bump | smaller slabs on Macs |
| Patch `bevy_pbr` runtime layout | none | vendored crate, several hundred lines, every bump | none expected |

Off costs the 18 fields, and glTF transmission, clearcoat, specular and anisotropy textures, which
render without those maps. Revisit only if users ask for them; then the `bevy_pbr` patch is the
only option that loses nothing.

If anyone revisits: `standard_material_limits.rs` only checks wgpu's default limits. A case at
M1's real limits (16 samplers, 1024 sampler-array elements) would show the M1 failure without an
M1.
