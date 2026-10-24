## Why

A camera with `<OcclusionCulling />` and no `<DepthPrepass />` quits the engine on its first frame: wgpu
refuses the "downsample depth bind group", because the camera's depth texture has no
`TEXTURE_BINDING` usage. Bevy 0.20's docs say occlusion culling "requires a `DepthPrepass`" and
that without one the component "will be ignored". The code does not ignore it: upstream issue
bevyengine/bevy#22747 is open on exactly this crash. The heimdall-scale probe hit it from TSX
(scale.md, gap 4).

Re-run on npm alpha.20, headless, 100 cubes:

| Camera | Result |
| --- | --- |
| `OcclusionCulling` | quits: "downsample depth bind group", RENDER_ATTACHMENT lacks TEXTURE_BINDING |
| `OcclusionCulling`, MSAA 4x (the default) | quits: same error on the "multisample" bind group |
| `OcclusionCulling` + `DepthPrepass`, MSAA 4x | runs, no error |
| `OcclusionCulling` + `DepthPrepass`, MSAA off | runs, no error |

So MSAA is not a prerequisite. The scale doc's "MSAA off" was a guess; only `DepthPrepass` is.

## What Changes

- bevy-ts registers `DepthPrepass` as a required component of `OcclusionCulling`, with Bevy's own
  `App::try_register_required_components`. Any path that inserts `OcclusionCulling` (TSX, a level,
  BRP, Rust) gets the `DepthPrepass` Bevy documents it needs. One written `DepthPrepass` is kept.
- MSAA is left as the camera sets it. No check, no insert.
- If the registration fails because Bevy already requires it (an upstream fix), bevy-ts logs it at
  debug and carries on.
- The coverage matrix row says `OcclusionCulling` brings its `DepthPrepass`, and that removing the
  `DepthPrepass` alone from a camera that keeps `OcclusionCulling` still crashes, as in Bevy.

## Capabilities

### New Capabilities

None.

### Modified Capabilities

- `native-components`: a Rust component bevy-ts exposes keeps the prerequisites Bevy documents for
  it; `OcclusionCulling` brings `DepthPrepass`.

## Impact

- `crates/bevy-ts/src/lib.rs` (`BevyTsPlugin::build`, under the `pbr` feature): one registration.
- A render test in `crates/bevy-ts-cli/tests/it/` (a new module beside `reflect_render.rs`).
- `docs/research/bevy-coverage.md`: the culling row.
- No new dependency. No per-frame cost: a required component costs once at insert.
- New pattern: bevy-ts has not called `register_required_components` before. It is Bevy's API for
  this, but it is a policy bevy-ts adds on a Bevy type, so it needs the operator's (or lead's) go.
- Upstream: a comment on bevyengine/bevy#22747 proposing `#[require(DepthPrepass)]` on
  `OcclusionCulling` is outward-facing and needs the operator's go. It is not part of this change.
