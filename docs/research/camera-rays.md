# Camera rays

Scope for `camera-rays` (audit row 15): Bevy's viewport-to-world, NDC and mesh ray-cast API, for
TS, on world-to-screen's `Cameras`. Bevy 0.20 (`Cargo.toml:13`). Sources read: `bevy_camera-0.20.0`
`src/camera.rs`, `bevy_shape-0.20.0` `src/ray.rs`, `bevy_picking-0.20.0` `src/mesh_picking`,
`bevy_ecs-0.20.0` `system_param.rs`, the world-to-screen branch (69b3c7a), the mesh-picking branch
(8f3f04f), and main.

## 1. Bevy's category

| Bevy 0.20 | Signature | TS today | Proposed |
|---|---|---|---|
| `Camera::world_to_viewport` | `(&GlobalTransform, Vec3) -> Result<Vec2, ViewportConversionError>` | world-to-screen | — |
| `Camera::world_to_viewport_with_depth` | `-> Result<Vec3, _>`, z = view depth | missing | `worldToViewportWithDepth` |
| `Camera::viewport_to_world` | `(&GlobalTransform, Vec2) -> Result<Ray3d, _>`; ray starts on the near plane | missing | `viewportToWorld` |
| `Camera::viewport_to_world_2d` | `-> Result<Vec2, _>` | missing | `viewportToWorld2d` |
| `Camera::world_to_ndc` | `(&GlobalTransform, V) -> Option<V>` | missing | `worldToNdc` |
| `Camera::ndc_to_world` | `-> Option<V>` | missing | `ndcToWorld` |
| `Camera::viewport_to_ndc` | `(Vec2) -> Result<Vec2, _>` | missing | `viewportToNdc` |
| `ndc_to_viewport` | not in 0.20 | — | none (Bevy has none) |
| `Ray3d` (`bevy_shape`, not `bevy_math`, in 0.20) | `{ origin: Vec3, direction: Dir3 }`, `get_point`, `intersect_plane` | missing | a `HostRecord` `{ origin, direction }` |
| `MeshRayCast` (SystemParam) | `cast_ray(&mut self, Ray3d, &MeshRayCastSettings) -> &[(Entity, RayMeshHit)]`, nearest first | missing (editor uses it in Rust) | `MeshRayCast.castRay` / `castRayAll` |
| `MeshRayCastSettings` | `visibility: RayCastVisibility` (`Any`, `Visible`, `VisibleInView` default), `filter: &dyn Fn(Entity)->bool`, `early_exit_test: &dyn Fn(Entity)->bool` (default: exit at nearest) | — | see 4 |
| `RayMeshHit` | `point`, `normal` (not normalised under scale), `barycentric_coords`, `distance`, `triangle: Option<[Vec3;3]>`, `uv: Option<Vec2>`, `triangle_index: Option<usize>` | — | see 4 |
| `RayCastBackfaces`, `SimplifiedMesh` components | per-entity cast tuning | unchecked from TSX | follow reflect-coverage |

`ViewportConversionError`: `NoViewportSize`, `PastNearPlane`, `PastFarPlane`, `InvalidData`. All
become `undefined`, as in world-to-screen.

## 2. Where it goes: world-to-screen's `Cameras`

`Cameras` is `crates/bevy-ts-cli/src/camera.rs` (not bevy-ts-native: native builds Bevy without its
camera). Module `bevy-ts/camera`, `CamerasPlugin` in `base_app` and `types_app`. Rules to copy:
non-camera entity throws `Cameras.<method>: entity <bits> is not a camera`; every Bevy miss is
`undefined` (the host maps any `Option` to `undefined`); batched forms take flat `&[f32]` in and
`&mut [f32]` out, throw on a ragged or short array, fill `NaN` for a miss and return the hit count.
One-frame lag from `Update`, documented in docs/native.md.

The six camera functions above are `&self` methods on `Cameras`, same query, same file, no new
field. Batched `viewportsToWorld(camera, points, out)` (2 in, 6 out per point) follows
`worldsToViewport`. camera-rays waits for world-to-screen to land.

## 3. Overlap: what exists, what we reuse

- `ReadRapierContext.castRay` / `castRays` (`crates/bevy-ts-rapier3d/src/lib.rs:148-210`, docs/native.md:166-180):
  ray against Rapier colliders, `{ entity, toi }` or `undefined`, filter record
  `{ excludeRigidBody?, excludeCollider?, excludeSensors }`, batched with `-1`/`0` for a miss.
  Physics colliders only, not meshes. It is the shape model for the mesh cast, and
  `viewportToWorld`'s ray feeds it unchanged.
- Editor: `crates/bevy-ts-viewport/src/edit.rs:614-623` calls `MeshRayCast` in Rust (filter,
  `never_early_exit`, `RayCastVisibility::Visible`) via `SystemState`. The reference call. Not
  script-facing.
- Picking on main: five `onPointer*` props (`crates/bevy-ts-tsx/src/picking.rs`) send the pointer's
  screen `position` only, never `HitData.position`/`normal`/`depth`. So "world points from picking"
  does not exist on main. Adding the hit's world point to the event is a picking-payload change,
  bevy-ui-coverage's (stage 3, all events), not ours.
- mesh-picking (landing): the `meshPicking` switch in `bevy-ts.json` (`project.rs`, through
  `base_app`, the one place `MeshPickingPlugin` is added), the headless pointer and window-less
  `PrimaryWindow` (`crates/bevy-ts-viewport/src/offscreen.rs`), docs/picking.md. We reuse the
  headless engine for tests; we add no plugin.
- Nothing wraps `MeshRayCast` for TS today. Nothing builds a ray from TS.

`MeshRayCast` needs neither `MeshPickingPlugin` nor a `PrimaryWindow` (high confidence, from the
source): it reads `Assets<Mesh>`, `Aabb`, `InheritedVisibility`, `ViewVisibility`,
`GlobalTransform`, `Mesh3d`/`Mesh2d`/`SimplifiedMesh`. The window gotcha (`pointer.rs:228`) is
`Location::is_in_viewport`, picking's pointer path only. `viewportToWorld` needs the camera's
viewport size, which the headless 960x540 image target gives (world-to-screen's tests prove
`logicalViewportSize` headless). The `3d` feature already pulls in `mesh_picking`; no Cargo change.

Lags: `Aabb` comes from `calculate_bounds` in `PostUpdate`, so a mesh spawned this frame is not hit
until the next. `NoFrustumCulling` meshes get no `Aabb` and are never hit. `VisibleInView` needs a
camera that rendered the entity last frame.

## 4. Crossing to TS

- Ray: `#[derive(HostRecord)] struct Ray3d { origin: Vec3, direction: Vec3 }`, the TS type
  `Ray3d`. Records hold `Vec3` (the `exported.rs` test has one). In: `Dir3::new(direction)`; a zero
  or NaN direction throws.
- Nearest hit: `castRay(ray, settings) -> RayMeshHit | undefined`, record
  `{ entity, point, normal, distance, uv?, triangleIndex? }`. Leave `triangle` (an array of
  `Vec3`) and `barycentricCoords` out until asked.
- All hits: no `Vec<record>` return crosses. `castRayAll(ray, settings, hits: &mut [f32], entity:
  &mut [f64]) -> u32`, 7 floats per hit (point, normal, distance), entity bits as `f64`, as
  `castRays`. Returns the count written; more hits than room are dropped, nearest kept.
- Settings record: `{ visibility: "Any" | "Visible" | "VisibleInView", earlyExit: bool }`, plus
  `exclude: &[f64]` (entity bits) as an optional filter. Fieldless enums cross.

Blocker in the derive: `#[export]` requires `&self` (`bevy-ts-native-derive/src/lib.rs:203`) and
binds `let __p = __st.get(__world)` (line 270). `MeshRayCast::cast_ray` takes `&mut self` for its
`Local` scratch vectors. `MeshRayCast` is a `ReadOnlySystemParam` (`Local` is one,
`system_param.rs:1007`) and `SystemState::get` hands back an owned item, so the fix is two lines:
allow `&mut self` and bind `let mut __p`. It writes no world data, so it is not
writable-native-params. Without it, the only path is copying `cast_ray`'s ~70 lines over Bevy's
public `ray_aabb_intersection_3d` and `ray_mesh_intersection`: Bevy built twice.

Ask-first, flagged not decided: a TS callback for `filter` or `early_exit_test`. The host passes
values, not closures, so it would need a re-entrant call from Rust back into V8 for each AABB
candidate, mid host call, with live column views open. That is a new mechanism and a per-candidate
cost. The settings above avoid it: cast with `earlyExit: false` and filter the hits in TS.

## 5. Open questions for spec

1. Where does `MeshRayCast` live? Recommend: its own exported type `MeshRayCast` (Bevy's name) in
   `bevy-ts/camera`, same file and plugin as `Cameras`. A system that only projects then does not
   fetch `Assets<Mesh>` and the scratch vectors.
2. Allow `&mut self` on exported methods of a read-only param? Recommend yes, the two-line derive
   change, with a compile-fail doc test that `ResMut` still fails. Confirm with the lead whether it
   counts as a pattern change; I read it as not one (no world write, no new borrow).
3. Filter shape? Recommend `visibility`, `earlyExit`, `exclude` entities; no TS callback (operator
   ask-first if wanted).
4. Default visibility? Recommend Bevy's `VisibleInView`, so a cast matches what the camera drew.
5. Hit fields? Recommend `entity, point, normal, distance, uv?, triangleIndex?`; the normal stays
   un-normalised as Bevy's, documented.
6. A cursor convenience (`castFromViewport(camera, point, settings)`)? Recommend no: TS composes
   `viewportToWorld` then `castRay`, two host calls, as Bevy composes them.
7. Hit world point on `onPointer*` events? Recommend not here; hand to bevy-ui-coverage.
8. Tests? Recommend extending `crates/bevy-ts-cli/tests/cameras.rs`: headless 960x540, a cube at a
   known place, `viewportToWorld` of the centre pixel hits it with `castRay`; frame 1 gives
   `undefined`; non-camera throws; `castRayAll` through two cubes returns two, nearest first.
