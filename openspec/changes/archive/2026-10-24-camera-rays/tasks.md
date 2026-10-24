## 0. Start

- [x] 0.1 Wait for world-to-screen to land; rebase on main and re-read its `Cameras` and spec requirement; if either changed, update this change first

## 1. Derive

- [x] 1.1 `&mut self` on exported methods: already on main (writable exported params); nothing to do
- [x] 1.2 `#[derive(HostRecord)]` on a fieldless enum, `Kind::Enum` in the host codec, crossing as variant names (replaces the `ResMut` compile-fail test: main now allows `ResMut`)

## 2. Conversions on Cameras

- [x] 2.1 `#[derive(HostRecord)] Ray3d { origin: Vec3, direction: Vec3 }` in `crates/bevy-ts-cli/src/camera.rs`
- [x] 2.2 `world_to_viewport_with_depth`, `viewport_to_world`, `viewport_to_world_2d`, `world_to_ndc`, `ndc_to_world`, `viewport_to_ndc`, each `Result<Option<_>, String>` on the existing camera lookup
- [x] 2.3 `viewports_to_world(camera, points: &[f32], out: &mut [f32]) -> Result<u32, String>`, two in, six out, `NaN` for a miss

## 3. MeshRayCast

- [x] 3.1 `RayCastSettings` record with a fieldless `RayCastVisibility` enum; `RayMeshHit` record
- [x] 3.2 `#[derive(SystemParam)]` over Bevy's `MeshRayCast`, exported as `MeshRayCast` from `bevy-ts/camera`; `cast_ray(&mut self, ray, settings: Option<_>, exclude: &[f64])` and `cast_ray_all(&mut self, ray, settings, exclude, hits: &mut [f32], entity: &mut [f64]) -> Result<u32, String>`; `Dir3::new` failure throws
- [x] 3.3 `CamerasPlugin` adds `ts_param` for it

## 4. Tests

- [x] 4.1 `crates/bevy-ts-cli/tests/it/cameras.rs`, headless 960x540: centre ray, round trips, depth, spawn frame, batched rays (one host call), non-camera throws
- [x] 4.2 Same file: cursor ray hits the cube, a miss, two cubes nearest first, `exclude`, a cube spawned this frame, a zero direction throws
- [x] 4.3 `bevy-ts types` declares `MeshRayCast`, `Ray3d`, `RayCastSettings`, `RayMeshHit` in `bevy-ts/camera`
- [x] 4.4 Check any protocol snapshot or params list; take the next free minor only if one changed: none changed (no protocol surface touched; the gate's gen-protocol --check confirms)

## 5. Docs

- [x] 5.1 docs/native.md, the `bevy-ts/camera` section world-to-screen wrote: the conversions, `Ray3d`, `MeshRayCast` (settings, `exclude`, the un-normalised normal, the `Aabb` frame lag, `NoFrustumCulling` never hit, `VisibleInView` needing a drawn frame), and the cursor example (`viewportToWorld` then `castRay`)
- [x] 5.2 docs/picking.md (from mesh-picking): one line pointing to docs/native.md for ray casts from scripts
