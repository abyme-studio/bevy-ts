## 1. Slots on Bevy's FullscreenMaterial

- [x] 1.1 Spike in `crates/bevy-ts-material/tests/it/pass_render.rs` (a module in `tests/it/main.rs`, render app from `alpha_render::app_with` under its `GPU` lock): `ScriptPass<0>` in `crates/bevy-ts-material/src/pass.rs` implementing `FullscreenMaterial` on `Core2d` (`Core2dSystems::PostProcess`, after `tonemapping`), its shader a fixed UUID handle, a 256-byte uniform read by a smaller WGSL struct; a pass returning red turns a 320×180 `Camera2d` image red, and inserting a new shader at the handle redraws without a restart. If the smaller struct or the in-place swap fails, stop and report before going on
- [x] 1.2 Slots 0 to 3, each after the one before; `ScriptMaterialPlugin` (`src/lib.rs`) adds `FullscreenMaterialPlugin::<ScriptPass<n>>` for each and exports `pass`'s public types
- [x] 1.3 The frame fields in `pass.rs`: a `PostUpdate` system after `Pixel2dSystems` and Bevy's camera update writes `time`, `texel`, `size`, `view` into every `ScriptPass<n>`

## 2. Kinds, layout, WGSL

- [x] 2.1 `layout.rs`: a pass layout, the four frame fields at offsets 0, 4, 8, 16 then the params by WGSL uniform rules, capped at 256 bytes; reserved names refused; unit tests for offsets and the cap
- [x] 2.2 `wgsl.rs`: append the pass declarations (struct, `screen`, `screen_sampler`, `params` at group 0, bindings 0 to 2) and check with Bevy's fullscreen imports; errors map to the author's `file:line:col`, generated-part errors name the schema
- [x] 2.3 `pass.rs`: the `passes` manifest section, checked and applied as `kinds.rs` does `materials`; slots by `order` then name; a fifth kind refused at `pass()`; a slot change moves shader and camera components in one apply; `.wgsl` saves through `on_ts_files_changed`; schema changes re-encode by name and wait on a failing shader; renderer failures through `PipelineWatch`
- [x] 2.4 `crates/bevy-ts-material/ts/material.js` and `material.d.ts`: `pass(name, { params, fragment, order? })` and `PassKind`, a WGSL string rejected with the declaring line

## 3. TSX

- [x] 3.1 `crates/bevy-ts-tsx/src/intrinsics.rs` and `resolve.rs`: `<pass kind>` under a `Camera2d` entity inserts `ScriptPass<slot>` with the encoded props; bound props re-encode the frame they change; not a `Camera2d`, a second of one kind, or an unknown prop fails at the element; removal removes the component
- [x] 3.2 `packages/bevy-ts/jsx-runtime.d.ts` (and `jsx-dev-runtime.d.ts` if it lists intrinsics): `PassProps`, typed from the kind's params, bindable

## 4. Tests (docs/testing.md)

- [x] 4.1 `crates/bevy-ts-material/tests/it/pass.rs` (no renderer): `slots_follow_order_then_name_and_a_fifth_kind_is_refused`, `a_reorder_moves_each_cameras_params_with_its_kind`, `a_reserved_name_or_too_many_bytes_fails_at_pass`, `a_schema_change_re_encodes_by_name`
- [x] 4.2 `crates/bevy-ts-material/tests/it/pass_render.rs` (renders): `a_red_pass_turns_the_image_red_and_no_pass_leaves_it`, `a_passthrough_at_zoom_2_draws_the_same_pixels_as_no_pass`, `two_passes_run_in_order`, `the_frame_fields_under_a_pixel_camera`, `a_replaced_shader_draws_within_frames_and_a_bad_one_keeps_the_last`
- [x] 4.3 `crates/bevy-ts-cli/tests/it/passes.rs` (a module in its `main.rs`; render app from `common::render_app()`): `a_tsx_pass_on_a_camera2d_draws_and_a_bound_amount_writes_that_frame`, `a_pass_off_a_camera2d_or_twice_fails_at_the_element`, `a_wgsl_save_redraws_with_no_new_generation`, `a_wgsl_error_names_the_pass_and_the_authors_line`
- [x] 4.4 A `script-fullscreen-pass-test` process in `.abyme/processes.ts`, in a check slot: `bevy-ts-material`'s lib tests and `pass::`, `pass_render::`; `bevy-ts-cli`'s `passes::`, `two_d_doc`, `materials::`; `bevy-ts-tsx`'s tests; `pnpm check` in `packages/bevy-ts`; clippy `-D warnings` on the three crates

## 5. Bench (rule 8)

- [ ] 5.1 `bench/fullscreen-pass/` (`Cargo.toml` outside the workspace with the `bevy_render` patch as `bench/pixel-2d` has, `src/main.rs`, `run.sh` sourcing `scripts/slot.sh`): 1920×1080 image target, `PixelCamera` zoom 3, 2,000 sprites from a 16×16 atlas made in code; arms `off`, `passthrough`, `grade`; the pass span's GPU time from `RenderDiagnosticsPlugin` and the frame time, median and p95 over 600 frames after 120 warm-up, arms alternated
- [ ] 5.2 A `fullscreen-pass-bench` process in `.abyme/processes.ts`; run it; `bench/fullscreen-pass/results/README.md` and `runs.txt` with machine, load, commit and the verdict against the 0.15 to 0.6 ms estimate

## 6. Docs and example

- [ ] 6.1 `docs/materials.md`: "Fullscreen passes": declaring, `<pass>`, the appended WGSL and frame fields, crispness and snapping to texels (a fog example), order, hot reload and errors, the four-kind and 256-byte limits, the measured cost, and the follow-ups (second texture, render targets and `RenderLayers`, script `Material2d`, `Camera3d`); its intro's "Render passes ... stay Rust" narrowed
- [x] 6.2 `docs/2d.md`: a short "Fullscreen passes" section pointing at materials.md, and a coverage row (`FullscreenMaterial` on `Core2d`, `<pass kind>`, working, the 4.2 and 4.3 tests) that `two_d_doc` checks
- [x] 6.3 `examples/shapes-2d`: `src/dusk.wgsl` and a `<pass kind={Dusk}>` on its camera with a bound amount; repo-made art only
- [x] 6.4 `CHANGELOG.md`: one line under Unreleased
