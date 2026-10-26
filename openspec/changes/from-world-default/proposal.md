## Why

reflect-coverage's bench (task 3.9) measured one `Mut<Projection>` write a frame at about 30 µs under load (15–44), far more than a row's write. It is not a flush. `reflect_value::default_of` gives a type with no `ReflectDefault` its `ReflectFromWorld` value, run on a new `World::new()`, on every call. `OrthographicProjection` reflects `FromWorld` only (bevy_camera `projection.rs:579`), so each write of an `Orthographic` projection built a world.

## What Changes

- `default_of` makes a type's `FromWorld` value once, on an empty world, and hands out a `reflect_clone` of it after. A type that does not clone through reflection is still made fresh each time. A `from_world` that panicked is remembered as no default.
- Unit test in `reflect_value.rs`: a `FromWorld` default is made once and cloned after.
- `docs/reflection.md`: the write's numbers before and after.

Measured on a quiet machine with `reflect_bench`, release, 3 alternated rounds: one `Mut<Projection>` write a frame went from 7, 7, 6 µs to 4, 3, 3 µs. The timer counts whole µs. The other arms did not move.

## Capabilities

### Modified Capabilities

- `native-components`: a `FromWorld` default is made once, not per write.

## Impact

- `crates/bevy-ts/src/reflect_value.rs` (`default_of`, `from_world`).
- `.abyme/processes.ts`: `from-world-bench`.
- No protocol change. One cached value per `FromWorld`-only type, per thread.
