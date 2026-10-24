## Context

world-to-screen (landed, e215b77) exports `Cameras` from `crates/bevy-ts-cli/src/camera.rs`, module `bevy-ts/camera`, over `Query<(&Camera, &GlobalTransform)>`, with `worldToViewport`, `logicalViewportSize` and the batched `worldsToViewport`. It lives in the engine crate because `bevy-ts-native` builds Bevy without its camera. `CamerasPlugin` is added in `base_app` and `types_app`.

Bevy 0.20's conversions are `&self` methods on `Camera` taking the camera's `GlobalTransform`. Bevy's mesh ray cast is the `MeshRayCast` system param (`bevy_picking::mesh_picking::ray_cast`): `cast_ray(&mut self, Ray3d, &MeshRayCastSettings) -> &[(Entity, RayMeshHit)]`, nearest first. It is a `ReadOnlySystemParam` (its `Local` scratch is one), and needs neither `MeshPickingPlugin` nor a window. The editor already calls it in Rust (`crates/bevy-ts-viewport/src/edit.rs:614-623`). Rapier's `ReadRapierContext.castRay`/`castRays` is the shape model for a script-facing ray cast.

The `#[export]` derive rejects any receiver but `&self` (`crates/bevy-ts-native-derive/src/lib.rs:203`) and binds `let __p = __st.get(__world)` (line 270).

## Goals / Non-Goals

**Goals:**
- Every Bevy camera conversion reachable from TS, on the one `Cameras` param.
- A script casts a ray against meshes with Bevy's own `MeshRayCast`, nearest hit or all hits.
- The cursor-to-mesh path composes from two calls: `viewportToWorld`, then `castRay`.

**Non-Goals:**
- A TS callback for Bevy's `filter` or `early_exit_test`.
- A `castFromViewport` convenience. Bevy has none; TS composes the two calls.
- The hit's world point on `onPointer*` events: a picking-payload change, bevy-ui-coverage's.
- Bevy's viewport-info getters (rects, physical sizes, scaling factor): not conversions; a later `Cameras` change.

## Coverage of Bevy's category

| Bevy 0.20 | TS | Reason if out |
|---|---|---|
| `Camera::world_to_viewport` | `Cameras.worldToViewport`, `worldsToViewport` | world-to-screen |
| `Camera::world_to_viewport_with_depth` | `Cameras.worldToViewportWithDepth` | |
| `Camera::viewport_to_world` | `Cameras.viewportToWorld`, `viewportsToWorld` | |
| `Camera::viewport_to_world_2d` | `Cameras.viewportToWorld2d` | |
| `Camera::world_to_ndc` | `Cameras.worldToNdc` (`Vec3`) | |
| `Camera::ndc_to_world` | `Cameras.ndcToWorld` (`Vec3`) | |
| `Camera::viewport_to_ndc` | `Cameras.viewportToNdc` | |
| `ndc_to_viewport` | none | not in Bevy 0.20 |
| `Camera::depth_ndc_to_view_z`, `_2d` | out | converts a depth-buffer value; TS reads no depth buffer |
| `Camera::logical_viewport_size` | `Cameras.logicalViewportSize` | world-to-screen |
| `logical_viewport_rect`, `physical_viewport_rect`, `physical_viewport_size`, `logical_target_size`, `physical_target_size`, `target_scaling_factor`, `clip_from_view` | out | viewport info, not a conversion; a later `Cameras` change |
| `Ray3d { origin, direction }` | record `Ray3d` | |
| `Ray3d::get_point`, `intersect_plane` | out | math on the record, no engine state: `origin + direction * t` in TS |
| `MeshRayCast::cast_ray` | `MeshRayCast.castRay` (nearest), `castRayAll` (all) | |
| `MeshRayCastSettings.visibility` (`Any`, `Visible`, `VisibleInView`) | `settings.visibility` | |
| `MeshRayCastSettings.early_exit_test` | `settings.earlyExit: boolean` (always / never) | a per-entity closure needs a TS callback (ask-first) |
| `MeshRayCastSettings.filter` | `exclude` entity list | a closure needs a TS callback (ask-first); filter the hits in TS |
| `RayMeshHit.point`, `normal`, `distance`, `uv`, `triangle_index` | `RayMeshHit` fields | |
| `RayMeshHit.barycentric_coords`, `triangle` | out | no consumer; `triangle` is an array of `Vec3`, which no record holds yet |
| `ray_mesh_intersection`, `ray_aabb_intersection_3d` | out | free functions over mesh data TS does not hold; `MeshRayCast` uses them |
| `RayCastBackfaces`, `SimplifiedMesh` components | out here | components, set from TSX through reflect; reflect-coverage owns them |
| `MeshPickingPlugin`, `MeshPickingSettings` | out here | the `meshPicking` switch (mesh-picking) |
| `Ray2d` | out | Bevy has no 2D mesh ray cast taking it; `viewportToWorld2d` covers 2D |

## Decisions

1. Conversions on `Cameras`, same query, same file, no new field. Each is `&self`, finds the camera like `worldToViewport`, and maps Bevy's `Result`/`Option` to `Option`. `worldToNdc`/`ndcToWorld` take and return `Vec3` (Bevy's generic `V`, the only shape a script needs).
2. `MeshRayCast` is its own exported type in `bevy-ts/camera`, added by `CamerasPlugin`. A system that only projects then fetches no `Assets<Mesh>` and no scratch. Bevy's name, Bevy's param inside: `#[derive(SystemParam)] struct MeshRayCaster { cast: MeshRayCast }`, exported as `MeshRayCast`.
3. `Ray3d` is a `HostRecord { origin: Vec3, direction: Vec3 }`. Out, `direction` is Bevy's unit `Dir3`. In, `Dir3::new(direction)` normalises; a zero or non-finite direction throws `MeshRayCast.castRay: ray direction ...`.
4. Settings: `RayCastSettings { visibility: RayCastVisibility, early_exit: bool }`, a fieldless enum crossing as `"VisibleInView" | "Visible" | "Any"`. The host codec gains `Kind::Enum` (one slot, the variant's index; the JS glue maps names to indices and throws on any other string), and `#[derive(HostRecord)]` takes a fieldless enum. A record that leaves an enum field out gets its first variant, as a missing number gets zero, so `VisibleInView` comes first. Exclusions are a separate slice `exclude: &[f64]` of entity bits (as `castRays`' entity slice), empty for none; records do not hold slices. `earlyExit: true` is Bevy's default (stop at the nearest hit); `false` is `never_early_exit`.
5. `settings` is `Option<RayCastSettings>`: `undefined` gives Bevy's defaults, `VisibleInView` and early exit at the nearest hit, so a cast matches what the camera drew.
6. `castRay` returns `Option<RayMeshHit>`, `{ entity, point, normal, distance, uv?, triangleIndex? }`. `normal` stays as Bevy gives it, not re-normalised under non-uniform scale; documented.
7. `castRayAll(ray, settings, exclude, hits: &mut [f32], entity: &mut [f64]) -> u32`: seven floats per hit (point, normal, distance) and one entity per hit, nearest first. Returns the number written; more hits than room are dropped, nearest kept. It casts with the given `earlyExit`, so a script wanting every hit passes `false`.
8. Batches follow `worldsToViewport` as landed: a ragged `points` or a short `out` throws, `NaN` for a miss. `viewportsToWorld(camera, points, out)` reads two floats and writes six (origin, direction) per point. `castRayAll` throws on a ragged `hits`; less room than hits drops the farthest.
9. `&mut self` came with main's writable exported params (landed before this change); `MeshRayCaster::cast_ray` uses it for Bevy's `Local` scratch. No derive change here.
10. No TS callback. Bevy's closures would need a re-entrant call from Rust into V8 per AABB candidate, mid host call, with column views open: a new mechanism the native-functions spec forbids ("SHALL NOT ... call back into JS") and a per-candidate cost. Cast with `earlyExit: false` and filter in TS.

## Risks / Trade-offs

- [A mesh spawned this frame has no `Aabb` until `PostUpdate`] → not hit until the next frame; documented, tested.
- [`NoFrustumCulling` meshes get no `Aabb`] → never hit; documented.
- [`VisibleInView` needs a camera that drew the entity last frame] → frame-1 casts with the default miss; documented; `Any` hits at once.
- [`viewportToWorld` needs a viewport size] → `undefined` on the camera's spawn frame, as `worldToViewport`.
- [world-to-screen changes before landing] → rebase on it once landed; this change adds to its requirement and does not restate it.
