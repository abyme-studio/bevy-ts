## Context

TS reaches Rust behaviour only through exported read-only params (native-functions). Rapier's `ReadRapierContext` (`crates/bevy-ts-rapier3d/src/lib.rs:148-210`) is the model: a `#[derive(SystemParam)]` struct, `#[export(ts, module)]` on its `impl`, `app.ts_param::<P>()` in a plugin, `#[derive(HostRecord)]` records and a batched slice method. Bevy's `Camera` already projects: `world_to_viewport(&GlobalTransform, Vec3) -> Result<Vec2, ViewportConversionError>` and `logical_viewport_size() -> Option<Vec2>`. Headless engines render to a 960x540 image target (`crates/bevy-ts-cli/src/headless.rs:13`). Every engine app is built by `base_app` (`cli.rs:197`, used by dev and play) or `types_app` (`types.rs:21`).

## Goals / Non-Goals

**Goals:**
- TS can position a UI node over a world point with one host call.
- The same param in dev, play, a shipped game and `bevy-ts types`.

**Non-Goals:**
- An anchored UI node that follows an entity by itself. Bevy lacks one; the upstream work is a draft (bevyengine/bevy#21812). bevy-ts follows Bevy and adds none.
- `viewport_to_world` (picking rays). Out of scope until a consumer asks.
- Fixing the one-frame lag in the param. A game that needs the current frame orders its system after transform propagation.

## Decisions

1. One bevy-ts-owned param, `Cameras`, module `bevy-ts/camera`, over `Query<(&Camera, &GlobalTransform)>`. Alternatives: a TS-side projection from `Projection` and `GlobalTransform` (duplicates Bevy, drifts from it), or a port crate (camera is core Bevy, not third-party). The param names the camera by `Entity`, as Bevy's API takes a camera, so multiple cameras work.
2. `null` for every `ViewportConversionError`. TS cannot act on the variants differently; each means "do not draw". A non-camera entity is a programming error, so it throws (a `Result` error), as `ReadRapierContext` throws before its context exists.
3. No bounds check. Callers clamp or hide; a tag sliding in from the edge needs the outside coordinates.
4. Batched form `worldsToViewport(camera, points, out) -> u32`, three floats in and two out per point, `NaN` for a miss (unlike `castRays`' `-1`, which is a valid coordinate here). One host call per batch.
5. Location: `crates/bevy-ts-native/src/camera.rs`, with a `CamerasPlugin` that calls `ts_param`. The derive emits `::bevy_ts_native::...` paths, which do not resolve inside `bevy-ts-native` itself; `extern crate self as bevy_ts_native;` in its `lib.rs` makes them resolve. If that fails, the module goes in `crates/bevy-ts-cli` instead (it depends on `bevy-ts-native`). `base_app` and `types_app` add the plugin.

## Risks / Trade-offs

- [One-frame lag from `Update`] → documented in docs/native.md; a system after `TransformSystems::Propagate` reads the current frame.
- [Frame 1 has no viewport size] → `null`, tested; callers skip the frame.
- [Protocol] → no schema change expected. If the build finds `Cameras` touches a protocol surface (params list in a snapshot, a version check), take the next free minor at landing.
