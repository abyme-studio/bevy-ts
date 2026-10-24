## 1. Test first

- [ ] 1.1 Add `crates/bevy-ts-cli/tests/it/occlusion_render.rs` (and `mod occlusion_render;` in
  `tests/it/main.rs`), built like `reflect_render.rs`: DefaultPlugins without winit, BevyTsPlugin,
  TsxPlugin, offscreen image target, the scratch project from `common::project`, waits by
  condition with a deadline (docs/testing.md rules 2 and 5).
- [ ] 1.2 Arm "alone": a TSX camera with `<Camera3d />` and `<OcclusionCulling />`, a cube in
  front of it. Assert, after 30 frames: the camera has `DepthPrepass`, the app has not exited, no
  `bevy_render` error was caught (check `AppExit` and that the picture shows the cube).
- [ ] 1.3 Arm "msaa": the same camera with default `Msaa`; assert it is `Msaa::Sample4` after 30
  frames and the picture shows the cube.
- [ ] 1.4 Arm "written": both `<OcclusionCulling />` and `<DepthPrepass />`; one `DepthPrepass`;
  a reload without `<OcclusionCulling />` keeps `DepthPrepass`.
- [ ] 1.5 Run 1.2 before the fix and confirm it fails with the "downsample depth bind group"
  error (all cargo through gate-slot).

## 2. Fix

- [ ] 2.1 In `BevyTsPlugin::build`, under `#[cfg(feature = "pbr")]`, call
  `app.try_register_required_components::<OcclusionCulling, DepthPrepass>()`; log an `Err` at
  debug with its reason. One comment naming bevyengine/bevy#22747.
- [ ] 2.2 Run the occlusion and reflect render modules green. No level or BRP arm: the
  requirement is ECS, so every insert path gets it; the render test is the one layer that shows
  the bug (docs/testing.md rule 1).

## 3. Docs

- [ ] 3.1 `docs/research/bevy-coverage.md`: a row (or the culling row) for `OcclusionCulling`:
  works from TSX, brings `DepthPrepass`, MSAA may stay on, removing `DepthPrepass` alone still
  crashes (Bevy #22747), deferred unsupported by Bevy.
