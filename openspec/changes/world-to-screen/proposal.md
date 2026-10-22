## Why

A TS system cannot place a UI node over a 3D entity: nothing exported maps a world point to the screen. Bevy has `Camera::world_to_viewport`; TS cannot reach it. Agent town (slice-3, task 7.3) needs name tags over agents, and its `--headless` walks check those positions over BRP.

## What Changes

- The engine exports a read-only system param `Cameras` from module `bevy-ts/camera`, wrapping `Query<(&Camera, &GlobalTransform)>`, on the existing `#[export]` pattern (as Rapier's `ReadRapierContext`).
- `cameras.worldToViewport(camera, point)` returns a `Vec2` in logical pixels, top-left origin (the units of a UI node's `left`/`top`), or `null` when Bevy cannot project the point (behind the camera, past the far plane, no viewport size yet, bad data). Points in front but off screen return coordinates outside the viewport; there is no bounds check.
- `cameras.logicalViewportSize(camera)` returns the viewport's logical size, or `null` before it is known.
- `cameras.worldsToViewport(camera, points, out)` projects many points in one host call and returns how many projected.
- Every engine adds it: `bevy-ts dev`, `bevy-ts play`, a shipped game and `bevy-ts types`.
- docs/native.md names the built-in module and the one-frame lag.

## Capabilities

### New Capabilities

None.

### Modified Capabilities

- `native-functions`: the engine itself exports a `Cameras` param that projects world points to the viewport.

## Impact

- Code: a new `camera` module in `crates/bevy-ts-native` (or `crates/bevy-ts-cli` if the derive's `::bevy_ts_native` paths cannot resolve inside its own crate); `crates/bevy-ts-cli/src/cli.rs` `base_app` and `src/types.rs` `types_app` add its plugin.
- Tests: a new `crates/bevy-ts-cli/tests/cameras.rs`, headless (960x540 image target).
- Protocol: no schema change. `Cameras` appears in the existing params list; take the next free minor only if the build finds a protocol surface touched.
- Docs: docs/native.md.
- Cost: one host call per point, or per batch. No new dependency, service or pattern.
- Ask-first: none. It follows the exported-param pattern and adds no dependency, infrastructure or measurable cost.
