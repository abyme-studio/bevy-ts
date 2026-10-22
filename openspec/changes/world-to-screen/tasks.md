## 1. Location

- [ ] 1.1 Add `extern crate self as bevy_ts_native;` to `crates/bevy-ts-native/src/lib.rs` and check an `#[export]` compiles inside the crate; if not, put the module in `crates/bevy-ts-cli` and note it in design.md

## 2. The param

- [ ] 2.1 Write `camera.rs`: `#[derive(SystemParam)] Cameras` over `Query<(&Camera, &GlobalTransform)>`, exported as `Cameras` from `bevy-ts/camera`
- [ ] 2.2 `world_to_viewport(camera, point) -> Result<Option<Vec2>, String>`: `Ok(None)` for every `ViewportConversionError`, `Err` naming the entity when it is not a camera
- [ ] 2.3 `logical_viewport_size(camera) -> Result<Option<Vec2>, String>`
- [ ] 2.4 `worlds_to_viewport(camera, points: &[f32], out: &mut [f32]) -> Result<u32, String>`, `NaN` pair for a miss
- [ ] 2.5 `CamerasPlugin` calling `app.ts_param::<Cameras<'static, 'static>>()`; add it in `base_app` (`cli.rs`) and `types_app` (`types.rs`)

## 3. Tests

- [ ] 3.1 `crates/bevy-ts-cli/tests/cameras.rs`, headless 960x540: a point on screen at the centre, off screen in front (x > 960), behind (`null`), the spawn frame (`null` for both methods, values a frame later), the batched form (one host call, count and `NaN` pair), and a non-camera entity throwing
- [ ] 3.2 `bevy-ts types` declares `Cameras` in module `bevy-ts/camera`
- [ ] 3.3 Check whether any protocol snapshot or params list changed; if so, take the next free minor

## 4. Docs

- [ ] 4.1 docs/native.md, "Ports and built-in modules": the built-in `bevy-ts/camera` module, its units, `null` cases and the one-frame lag
