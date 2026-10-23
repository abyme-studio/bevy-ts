## 1. Location

- [x] 1.1 Add `extern crate self as bevy_ts_native;` to `crates/bevy-ts-native/src/lib.rs` and check an `#[export]` compiles inside the crate; if not, put the module in `crates/bevy-ts-cli` and note it in design.md

## 2. The param

- [x] 2.1 Write `camera.rs` (in `crates/bevy-ts-cli`, see 1.1): `#[derive(SystemParam)] Cameras` over `Query<(&Camera, &GlobalTransform)>`, exported as `Cameras` from `bevy-ts/camera`
- [x] 2.2 `world_to_viewport(camera, point) -> Result<Option<Vec2>, String>`: `Ok(None)` for every `ViewportConversionError`, `Err` naming the entity when it is not a camera
- [x] 2.3 `logical_viewport_size(camera) -> Result<Option<Vec2>, String>`
- [x] 2.4 `worlds_to_viewport(camera, points: &[f32], out: &mut [f32]) -> Result<u32, String>`, `NaN` pair for a miss
- [x] 2.5 `CamerasPlugin` calling `app.ts_param::<Cameras<'static, 'static>>()`; add it in `base_app` (`cli.rs`) and `types_app` (`types.rs`)

## 3. Tests

- [x] 3.1 `crates/bevy-ts-cli/tests/cameras.rs`, headless 960x540: a point on screen at the centre, off screen in front (x > 960), behind (`undefined`), the spawn frame (`undefined` for both methods, values a frame later), the batched form (one host call, count and `NaN` pair), and a non-camera entity throwing
- [x] 3.2 `bevy-ts types` declares `Cameras` in module `bevy-ts/camera`
- [x] 3.3 Check whether any protocol snapshot or params list changed; if so, take the next free minor (none: no schema changed; `Cameras` is a value in the existing params record)

## 4. Docs

- [x] 4.1 docs/native.md, "Ports and built-in modules": the built-in `bevy-ts/camera` module, its units, `undefined` cases and the one-frame lag
