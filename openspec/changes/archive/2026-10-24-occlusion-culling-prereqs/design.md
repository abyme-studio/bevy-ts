## Context

Bevy 0.20 (`bevy_render::occlusion_culling::OcclusionCulling`) derives `Component, Reflect,
Default` with no `#[require]`. Its docs say it needs a `DepthPrepass` and is ignored without one.
The code disagrees:

- `configure_occlusion_culling_view_targets` (bevy_core_pipeline `core_3d/mod.rs`) adds
  `TEXTURE_BINDING` to the camera's depth texture only `With<DepthPrepass>`.
- `prepare_view_depth_pyramids` and `prepare_downsample_depth_view_bind_groups`
  (`mip_generation/experimental/depth.rs`) build the depth pyramid and bind the depth texture for
  every view `With<OcclusionCulling>`, prepass or not.

Without `DepthPrepass` the bind group is invalid, and `bevy_render::error_handler` quits the app.
Upstream issue bevyengine/bevy#22747 (open since 2026-01-30) reports it; its thread has not
settled whether the fix is a requirement or a texture flag.

The downsample has a multisampled path (`DownsampleDepthPipelineKey::MULTISAMPLE`), and the
re-run probe shows `OcclusionCulling` + `DepthPrepass` with MSAA 4x renders. MSAA is not a
prerequisite on Metal. (WebGL2 cannot bind a multisampled depth texture, but WebGL2 also lacks GPU
preprocessing, so occlusion culling never runs there.)

TSX's insert does not bypass required components: `OcclusionCulling` has none to bypass.

## Goals / Non-Goals

**Goals:** `<OcclusionCulling />` alone works from TSX, levels and BRP; MSAA stays the user's.

**Non-Goals:** patching Bevy; guarding the removal of `DepthPrepass` from a camera that keeps
`OcclusionCulling`; deferred-shading compatibility (Bevy calls it unspecified); measuring what
occlusion buys (scale.md says the overhead camera needs it little).

## Decisions

1. **Require, the Bevy way.** `app.try_register_required_components::<OcclusionCulling,
   DepthPrepass>()` in `BevyTsPlugin::build` under `pbr`. This is Bevy's mechanism for "B comes
   with A", it covers every insert path at once, and it matches Bevy's documented contract.
   It is what `#[require(DepthPrepass)]` upstream would do.
   - Alternative: refuse `OcclusionCulling` without `DepthPrepass` with an error in TSX, levels and
     BRP. Three places to check, Rust inserts still crash, and the user must write a component Bevy
     says is needed anyway. Rejected.
   - Alternative: a system that inserts `DepthPrepass` when it sees `OcclusionCulling`. A frame
     late (the crash is on that frame's render), and a per-frame query. Rejected.
   - Alternative: add `TEXTURE_BINDING` without a prepass (the #22747 thread's idea). That patches
     Bevy's render code. Rejected.
2. **Register in `build`, not `finish`.** Bevy panics if `OcclusionCulling` was ever inserted
   before the registration; the try form returns `ArchetypeExists`. `build` runs before any
   spawn. A `DuplicateRegistration` (a future Bevy with the require) is logged at debug.
3. **MSAA untouched.** The probe disproved the need. Bevy forces MSAA off only for deferred
   (`check_msaa`); bevy-ts adds nothing like it.

## Risks / Trade-offs

- [Removing `<DepthPrepass />` alone while `<OcclusionCulling />` stays crashes again] → Bevy's
  required components do not guard removal. Documented in the coverage row; the fix is upstream.
- [A depth prepass costs a pass] → It is the cost Bevy's occlusion culling already has; there is
  no working `OcclusionCulling` without it.
- [Upstream adds the require differently, or drops the prepass need] → the try registration
  degrades to a debug line; delete it when Bevy fixes #22747.
