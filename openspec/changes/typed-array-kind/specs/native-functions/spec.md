## ADDED Requirements

### Requirement: Slices must be their declared typed array
A slice argument `&[T]` or `&mut [T]` SHALL be given as the typed array its declaration names (`Float32Array` for `f32`, `Vec2`, `Vec3` and `Vec4`, `Float64Array` for `f64`, and so on). Any other value, including a typed array of another element type, an `ArrayBuffer` or a `DataView`, SHALL throw before the call reaches Rust, with a message naming the param, method and argument, the expected type and the type given.

#### Scenario: A Float64Array for a Float32Array
- **WHEN** a TS system calls `cameras.worldsToViewport(camera, new Float64Array(3), out)`
- **THEN** the call throws `Cameras.worldsToViewport: points must be a Float32Array, not a Float64Array`, and the method does not run

#### Scenario: A wrong out array
- **WHEN** the same call passes a `Float32Array` as `points` and a `Float64Array` as `out`
- **THEN** it throws naming `out`, and nothing is written
