## 1. Bind the image's sampler

- [x] 1.1 `crates/bevy-ts-material/src/render.rs` `prepare_asset`: for a set texture bind `img.sampler.clone()` beside `img.texture_view`; for an unset one `fallback.d2.sampler.clone()` beside `fallback.d2.texture_view`. `SamplerBindingType::Filtering` stays
- [x] 1.2 Same file: remove `ScriptMaterialSampler`, its creation in `init_render_resources` (and the then unused `SamplerDescriptor`/`Sampler` imports), and `SRes<ScriptMaterialSampler>` from `Param`
- [x] 1.3 Same file: `struct AfterImages;` implementing `bevy::render::erased_render_asset::ErasedRenderAssetDependency` as `render_app.add_systems(Render, system.after(prepare_assets::<GpuImage>))`; the plugin adds `ErasedRenderAssetPlugin::<ScriptMaterial, AfterImages>`

## 2. Rebind on a sampler change

- [x] 2.1 Same file: `#[derive(Resource, Default)] struct ImageSamplers(HashMap<AssetId<Image>, ImageSampler>)`, and a system `rebind_samplers` in `PostUpdate` after `AssetEventSystems`, reading `MessageReader<AssetEvent<Image>>`, `Res<Assets<Image>>`, `ResMut<Assets<ScriptMaterial>>`. On `Modified { id }` for an image some `ScriptMaterial` holds: equal to the entry, nothing; else store it and `get_mut` each `ScriptMaterial` whose `textures` holds `id`. `Removed`/`Unused` drop the entry. Doc comment: why (GpuImage makes a new sampler; the material only re-prepares on its own event)
- [x] 2.2 Look up the entry before scanning materials, so a `setData` with an entry costs a lookup and a compare

## 3. Tests

- [x] 3.1 `crates/bevy-ts-cli/tests/it/material_params_render.rs`: a game with a `game::Edge` kind (`textures: { tex: texture2d }`, fragment `return textureSample(tex, tex_sampler, vec2<f32>(1.01, 0.5));`), a 2x1 `Rgba8UnormSrgb` image (left red, right blue) from `Image.new` with a sampler the test chooses, on a cuboid, using the module's `app`, `pixels`, `red`, `blue`. Scratch folder from `project`, renders from `common::render_app()`
- [x] 3.2 Test `a_clamping_image_draws_its_edge_texel`: ClampToEdge, Linear: the middle pixel is blue with red below 20 (fails on main's code, which repeats)
- [x] 3.3 Test `a_repeating_image_blends_with_its_far_edge`: Repeat, Linear: red and blue both above 80
- [x] 3.4 Test `an_image_with_no_sampler_takes_the_image_plugins`: no sampler, the module's default `ImagePlugin`: blue, red below 20
- [x] 3.5 Test `a_changed_sampler_rebinds_the_same_instance`: starts Repeat (blend), a system at frame 120 calls `assets.set(tex, { sampler: ClampToEdge })`; the pixel turns blue and `Assets<ScriptMaterial>.len()` is unchanged
- [x] 3.6 Run the `it` binary's `material_params_render`, `materials`, `scene_material`, `reflect_assets` and `world_text_render` (bevy-ts-material) modules via `~/dev/abyme/scripts/gate-slot`; all pass, `an_image_from_bytes_is_sampled_and_redrawn_after_set_data` and `writing_every_frame_builds_no_new_pipeline` unchanged
- [x] 3.7 Grep `examples/` and `templates/` WGSL for textures sampled past `[0, 1]` (`uv *`, `fract` absent) whose image has no repeating sampler; give each image a Repeat sampler, or record none found here. None found: no script WGSL in `examples/` samples a texture, and there is no `templates/`

## 4. Docs

- [x] 4.1 `docs/materials.md` textures bullet: `<name>_sampler` is the image's sampler (`Image.new`'s or `Image.load`'s `sampler`, or `assets.set`); unset, Bevy's default, which clamps to the edge; `assets.set` of a sampler rebinds the next frame
- [x] 4.2 `docs/materials.md` "Tiling textures": say it holds for script materials too
- [x] 4.3 No coverage table lists script material bindings (`script-assets`' row "Loader settings, image samplers" is owned by reflect-assets and unchanged), so no row is added; the spec delta and the tests in 3.x are the record
