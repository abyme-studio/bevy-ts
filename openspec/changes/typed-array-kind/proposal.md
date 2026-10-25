## Why

`Cameras.worldsToViewport(camera, points, out)` takes a `Float64Array`: 1,000 points report 1,750
projected and `out` fills with values near 2e17. A slice argument crosses to Rust as bytes, so Rust
cannot see the element type, and the glue checks only that the argument is a typed array.

## What Changes

- The generated glue passes each slice argument's typed array class to `H.bytes`, which refuses any
  other class (another element type, an `ArrayBuffer`, a `DataView`, a non-array) before the call
  crosses, with `Param.method: arg must be a Float32Array, not a Float64Array`.
- Every typed-array param gets it: `Cameras.worldsToViewport` (`points`, `out`),
  `Cameras.viewportsToWorld` (`points`, `out`), `MeshRayCast.castRayAll` (`exclude`, `hits`,
  `entity`), `ReadRapierContext.castRays` (`rays`, `toi`, `entity`), and any param a crate exports.

## Impact

- Code: `crates/bevy-ts/src/host.rs` (glue), `crates/bevy-ts-v8/js/core.js` and its synced
  `packages/bevy-ts/runtime/core.js` (`HOST.bytes`).
- Tests: `crates/bevy-ts-cli/tests/it/cameras.rs`, `exported.rs`.
- Docs: `docs/native.md`. Spec: native-functions.
- A script passing an `ArrayBuffer` where a typed array is declared now throws.
