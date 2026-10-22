## MODIFIED Requirements

### Requirement: Rust exports system params to TS
A Rust crate (the project's own or a port) SHALL be able to export a read-only Bevy `SystemParam` to TS by putting `#[bevy_ts::export(ts = "Name", module = "bevy-ts/<module>")]` on an `impl` block of the param and calling `app.ts_param::<P>()`. Every `pub fn` in that block SHALL become a method of the TS type `Name`. The macro SHALL generate the host function per method, the argument and result codecs, the TS declaration and the JS glue. Method signatures SHALL be restricted to `&self` or `&mut self` plus scalars, `bool`, `Vec2`, `Vec3`, `Quat`, `Entity`, `NativeLayout` records, `&[T]` and `&mut [T]` of those, and `Option` of those; any other type SHALL be a compile error naming the parameter. `&mut self` SHALL reach only the param item `SystemState::get` returns for that call (its `Local` state), never world data. `P` SHALL implement `ReadOnlySystemParam`, or the crate fails to compile.

#### Scenario: Rapier's raycast
- **WHEN** the Rapier port exports `impl ReadRapierContext { pub fn cast_ray(&self, origin: Vec3, dir: Vec3, max_toi: f32, solid: bool, f: RayFilter) -> Option<RayHit> }`
- **THEN** a TS system taking `rapier: ReadRapierContext` can call `rapier.castRay(origin, dir, 1.1, true, { excludeRigidBody: e })`, typed from the Rust signature

#### Scenario: A method with local scratch
- **WHEN** a crate exports `pub fn cast_ray(&mut self, ...)` on a param holding Bevy's `MeshRayCast`, whose cast fills `Local` vectors
- **THEN** the crate compiles and the TS method calls it

#### Scenario: A mutable param
- **WHEN** a crate exports an `impl` on a param holding `ResMut<NavMesh>`, with `&self` or `&mut self` methods
- **THEN** the crate fails to compile, saying the param must be read-only

## ADDED Requirements

### Requirement: Cameras converts between world, viewport and NDC
The engine's `Cameras` param (module `bevy-ts/camera`) SHALL have, beside world-to-screen's methods, one method per Bevy 0.20 `Camera` conversion, each over the camera's `GlobalTransform` as last propagated, in logical pixels from the viewport's top-left where a viewport point is involved:

- `worldToViewportWithDepth(camera: Entity, point: Vec3): Vec3 | undefined`, `Camera::world_to_viewport_with_depth`, `z` the view-space depth.
- `viewportToWorld(camera: Entity, point: Vec2): Ray3d | undefined`, `Camera::viewport_to_world`, a ray from the near plane with a unit `direction`.
- `viewportToWorld2d(camera: Entity, point: Vec2): Vec2 | undefined`, `Camera::viewport_to_world_2d`.
- `worldToNdc(camera: Entity, point: Vec3): Vec3 | undefined` and `ndcToWorld(camera: Entity, ndc: Vec3): Vec3 | undefined`, `Camera::world_to_ndc` and `ndc_to_world`.
- `viewportToNdc(camera: Entity, point: Vec2): Vec2 | undefined`, `Camera::viewport_to_ndc`.
- `viewportsToWorld(camera: Entity, points: Float32Array, out: Float32Array): number`, one call for many points: two floats in and six out (origin, direction) per point, `NaN` six times for a miss, returning how many converted.

`Ray3d` SHALL be a record `{ origin: Vec3, direction: Vec3 }`, Bevy's `Ray3d`. Every Bevy conversion error or `None` SHALL be `undefined`. An `Entity` that is not a camera SHALL throw, naming `Cameras`, the method and the entity. A batch SHALL process whole chunks of both slices, the shorter bounding the count, as `worldsToViewport`.

#### Scenario: The centre pixel's ray
- **WHEN** a headless 960x540 engine has a camera at `[0, 0, 10]` looking at the origin, and a system calls `cameras.viewportToWorld(cam, [480, 270])`
- **THEN** it returns a ray whose `direction` is `[0, 0, -1]` and whose `origin` lies on the line from the camera through the origin, within 1e-4

#### Scenario: Round trips
- **WHEN** the system projects a world point in view with `worldToNdc` and converts back with `ndcToWorld`, and projects one with `worldToViewport` and converts that with `viewportToNdc`
- **THEN** `ndcToWorld` returns the point, and `viewportToNdc` equals `worldToNdc`'s `x` and `y`, within 1e-4

#### Scenario: Depth
- **WHEN** the system calls `worldToViewportWithDepth(cam, [0, 0, 0])`
- **THEN** it returns `[480, 270, z]` with `z` the camera's distance to the origin, 10, within a pixel and 1e-3

#### Scenario: Before the viewport has a size
- **WHEN** a system calls `viewportToWorld` on the frame the camera is spawned
- **THEN** it returns `undefined`, and a later frame returns a ray

#### Scenario: Many rays in one call
- **WHEN** a system calls `cameras.viewportsToWorld(cam, points, out)` with two points
- **THEN** the backend records one host call, it returns `2`, and `out` holds each point's origin and direction

#### Scenario: Not a camera
- **WHEN** a system calls `cameras.viewportToWorld(e, p)` with an entity that has no `Camera`
- **THEN** the call throws an exception naming `Cameras`, `viewportToWorld` and the entity

### Requirement: The engine exports MeshRayCast
Every engine (`bevy-ts dev`, `bevy-ts play`, a shipped game, `bevy-ts types`) SHALL export a param `MeshRayCast` from module `bevy-ts/camera`, over Bevy's `MeshRayCast`, with:

- `castRay(ray: Ray3d, settings: RayCastSettings | undefined, exclude: Float64Array): RayMeshHit | undefined`, the nearest hit.
- `castRayAll(ray: Ray3d, settings: RayCastSettings | undefined, exclude: Float64Array, hits: Float32Array, entity: Float64Array): number`, every hit nearest first: seven floats per hit (point, normal, distance) in `hits`, the entity's bits in `entity`; it SHALL return the number written, dropping hits past the room left and keeping the nearest.

`RayCastSettings` SHALL be `{ visibility: "Any" | "Visible" | "VisibleInView", earlyExit: boolean }`, Bevy's `RayCastVisibility` and its always or never early exit; `undefined` SHALL be Bevy's default, `VisibleInView` with early exit. `exclude` SHALL hold entity bits the cast skips, empty for none. `RayMeshHit` SHALL be `{ entity, point, normal, distance, uv?, triangleIndex? }`, with `normal` as Bevy gives it. A ray whose `direction` is zero or not finite SHALL throw, naming `MeshRayCast` and the method. A cast SHALL NOT call back into JS. A mesh SHALL be hit from the frame after it gets its `Aabb` (Bevy computes it in `PostUpdate`).

#### Scenario: The cursor's mesh
- **WHEN** a headless 960x540 engine has a camera at `[0, 0, 10]` looking at a unit cube at the origin, and a system casts `viewportToWorld(cam, [480, 270])` with `meshRayCast.castRay(ray, { visibility: "Any", earlyExit: true }, new Float64Array())`
- **THEN** it returns the cube's entity, `point` `[0, 0, 0.5]`, `normal` `[0, 0, 1]` and `distance` from the ray's origin, within 1e-3

#### Scenario: A miss
- **WHEN** the system casts a ray that passes beside every mesh
- **THEN** `castRay` returns `undefined`

#### Scenario: Every hit, nearest first
- **WHEN** two cubes lie on the ray and the system calls `castRayAll` with `earlyExit: false` and room for four hits
- **THEN** it returns `2`, the nearer cube's entity and point first

#### Scenario: Excluded entities
- **WHEN** the system casts the same ray with the nearer cube's bits in `exclude`
- **THEN** `castRay` returns the farther cube

#### Scenario: A mesh spawned this frame
- **WHEN** a cube is spawned in the frame that casts at it
- **THEN** the cast misses, and the same cast a frame later hits

#### Scenario: A bad ray
- **WHEN** a system calls `castRay` with `direction` `[0, 0, 0]`
- **THEN** the call throws an exception naming `MeshRayCast` and `castRay`

#### Scenario: Declared for TS
- **WHEN** `bevy-ts types` runs
- **THEN** module `bevy-ts/camera` declares `MeshRayCast`, `Ray3d`, `RayCastSettings` and `RayMeshHit`
