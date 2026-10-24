## ADDED Requirements

### Requirement: OcclusionCulling brings its DepthPrepass
bevy-ts SHALL register `DepthPrepass` as a required component of `OcclusionCulling` when its
`pbr` feature is on, through Bevy's `App::try_register_required_components`, so that inserting
`OcclusionCulling` on a camera by any path (TSX, a level, BRP, Rust) also inserts a default
`DepthPrepass` unless one is given. A `DepthPrepass` the caller writes SHALL be kept. bevy-ts
SHALL NOT change a camera's `Msaa`: occlusion culling runs with MSAA on. If Bevy already requires
`DepthPrepass` for `OcclusionCulling`, the registration's error SHALL be logged at debug and the
engine SHALL start as usual.

#### Scenario: OcclusionCulling alone renders
- **WHEN** a TSX camera has `<Camera3d />` and `<OcclusionCulling />` and no `<DepthPrepass />`,
  and draws to an offscreen image for 30 frames
- **THEN** the camera entity has `DepthPrepass`, no render error is logged, the app does not quit,
  and the image shows the cube in front of the camera

#### Scenario: MSAA stays on
- **WHEN** the same camera keeps Bevy's default `Msaa` (4 samples)
- **THEN** its `Msaa` is still 4 samples after 30 frames and no render error is logged

#### Scenario: A written DepthPrepass is kept
- **WHEN** a TSX camera has both `<OcclusionCulling />` and `<DepthPrepass />`
- **THEN** the camera has one `DepthPrepass`, and removing `<OcclusionCulling />` on a reload
  leaves the written `DepthPrepass` in place
