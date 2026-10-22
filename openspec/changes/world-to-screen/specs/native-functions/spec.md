## ADDED Requirements

### Requirement: The engine exports Cameras
Every engine (`bevy-ts dev`, `bevy-ts play`, a shipped game, `bevy-ts types`) SHALL export a read-only param `Cameras` from module `bevy-ts/camera`, over Bevy's `Query<(&Camera, &GlobalTransform)>`, through the exported-param mechanism. It SHALL have these methods, all in logical pixels with the origin at the viewport's top-left, the units of a UI node's `left` and `top`:

- `worldToViewport(camera: Entity, point: Vec3): Vec2 | undefined`, Bevy's `Camera::world_to_viewport`. It SHALL return `undefined` for every viewport conversion error (no viewport size, past the near plane, past the far plane, invalid data). It SHALL NOT bounds-check: a point in front of the camera but off screen SHALL return coordinates outside `[0, size]`.
- `logicalViewportSize(camera: Entity): Vec2 | undefined`, `undefined` while the viewport size is unknown.
- `worldsToViewport(camera: Entity, points: Float32Array, out: Float32Array): number`, one call for many points: `points` holds three floats per point, `out` receives two per point (`NaN, NaN` for one that does not project), and it returns how many projected.

An `Entity` that is not a camera SHALL throw, naming `Cameras`, the method and the entity. A method SHALL read the `GlobalTransform` as last propagated, so from `Update` it reflects the previous frame's transforms.

#### Scenario: A point on screen
- **WHEN** a headless 960x540 engine has a camera at `[0, 0, 10]` looking at the origin, and a system calls `cameras.worldToViewport(cam, [0, 0, 0])`
- **THEN** it returns `[480, 270]`, within a pixel

#### Scenario: Off screen in front
- **WHEN** the system projects a point in front of the camera but far to its right
- **THEN** it returns an `x` greater than 960, not `undefined`

#### Scenario: Behind the camera
- **WHEN** the system projects `[0, 0, 20]`, behind the camera
- **THEN** it returns `undefined`

#### Scenario: Before the viewport has a size
- **WHEN** a system projects a point on the frame the camera is spawned, before Bevy has computed its viewport
- **THEN** `worldToViewport` and `logicalViewportSize` return `undefined`, and a later frame returns values

#### Scenario: Many points in one call
- **WHEN** a system calls `cameras.worldsToViewport(cam, points, out)` with one point on screen and one behind
- **THEN** the backend records one host call, it returns `1`, `out` holds the first point's coordinates then `NaN, NaN`

#### Scenario: Not a camera
- **WHEN** a system calls `cameras.worldToViewport(e, p)` with an entity that has no `Camera`
- **THEN** the call throws an exception naming `Cameras`, `worldToViewport` and the entity
