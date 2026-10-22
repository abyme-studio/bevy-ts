## Why

A TS system cannot turn a screen point into a world ray, or ask which mesh a ray hits. Bevy has `Camera::viewport_to_world` and the `MeshRayCast` param; TS reaches neither. world-to-screen exported `Cameras` with one direction (world to viewport); camera-rays adds the rest of Bevy's conversions and its mesh ray cast, so a script can click on the ground, aim at a mesh, or place a thing under the cursor. Audit row 15; research in docs/research/camera-rays.md.

## What Changes

- `Cameras` (module `bevy-ts/camera`, `crates/bevy-ts-cli/src/camera.rs`) gains Bevy's other camera conversions: `worldToViewportWithDepth`, `viewportToWorld`, `viewportToWorld2d`, `worldToNdc`, `ndcToWorld`, `viewportToNdc`, and the batched `viewportsToWorld`. World-to-screen's rules hold: a non-camera entity throws, a Bevy miss is `undefined`, a batch is flat `Float32Array`s with `NaN` for a miss and returns the hit count.
- A new record `Ray3d { origin, direction }`, Bevy's `Ray3d`. A zero or non-finite direction passed in throws.
- A new exported param `MeshRayCast` (Bevy's name), same module and plugin, over Bevy's `MeshRayCast`: `castRay(ray, settings, exclude)` returns the nearest `RayMeshHit` or `undefined`; `castRayAll(ray, settings, exclude, hits, entity)` writes every hit, nearest first, and returns the count. Settings are `{ visibility, earlyExit }`; `exclude` is a list of entities. No TS callback.
- The `#[export]` derive accepts `&mut self` on a param whose item is owned (`SystemState::get` returns it by value), so `MeshRayCast::cast_ray`'s local scratch vectors work. The param still must be `ReadOnlySystemParam`; `ResMut` still fails to compile.
- docs/native.md (where `Cameras` is documented) describes the new methods and `MeshRayCast`; docs/picking.md points to them.

## Capabilities

### New Capabilities

None.

### Modified Capabilities

- `native-functions`: `Cameras` gains Bevy's camera conversions; the engine exports `MeshRayCast`; an exported method may take `&mut self`.

## Impact

- Depends on world-to-screen (not landed: its `Cameras`, its `native-functions` requirement "The engine exports Cameras", docs/native.md section). Build starts after it lands. docs/picking.md comes from mesh-picking; the pointer waits for it too.
- Code: `crates/bevy-ts-cli/src/camera.rs` (methods, `Ray3d`, `MeshRayCast`, records; `CamerasPlugin` adds the second `ts_param`), `crates/bevy-ts-native-derive/src/lib.rs` (receiver check and `let mut __p`).
- Tests: `crates/bevy-ts-cli/tests/cameras.rs`, headless 960x540; a compile-fail test for `ResMut`.
- Protocol: no schema change expected; take the next free minor only if the build finds a protocol surface touched.
- Docs: docs/native.md, docs/picking.md.
- Cost: one host call per conversion, per batch, or per cast. A cast walks Bevy's AABB candidates as Bevy does. No new dependency: the engine's `3d` feature already pulls in `mesh_picking`.
- Ask-first: none that needs the operator's go. The `&mut self` derive change extends the existing export pattern (no world write, no new borrow, no new mechanism); the lead confirms. A TS callback in the ray loop (filter or early-exit) would be ask-first (a new re-entrant mechanism and a per-candidate cost) and is left out.
