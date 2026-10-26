## 1. Hole props bind

- [x] 1.1 `crates/bevy-ts-tsx/src/resolve.rs` `resolve`: drop the reflect-hole refusal; a bound name is a layout field or a reflect hole, else "has no field" listing both; only a column leaves `r.defaults`
- [x] 1.2 `crates/bevy-ts-tsx/src/host.rs` `patch`: a native component's reflect hole writes through `crate::reflected::write` with the component's type path, before the column branch
- [x] 1.3 `crates/bevy-ts-cli/src/types.rs` `hole_props`: each field `name?: T | (() => T)`; update its unit test if one pins the shape
- [x] 1.4 Regenerate `packages/bevy-ts/bevy.d.ts` (`scripts/gen-bevy-types.sh`) and `packages/bevy-ts/test/engine/native.d.ts`
- [x] 1.5 tsc test in `packages/bevy-ts/test/tsx/reflect.tsx` `Bound`: `<DistanceFog color={() => (night() ? "#101020" : "#c0d0ff")} />` passes; `<DistanceFog color={() => 3} />` is a `@ts-expect-error`; the `DirectionalLight color={() => "#fff"}` line loses its `@ts-expect-error`
- [x] 1.6 Rust test in `crates/bevy-ts-cli/tests/it/reflect_values.rs` `a_bound_fog_colour_follows_its_signal`: a camera with `<DistanceFog color={() => ...}>`, a system flips the signal at runs 3 and 6; the colour reads `#c0d0ff`, `#101020`, `#c0d0ff` and `directional_light_exponent` is unchanged; a binding returning `{ Srgba: { red: 1 } }` logs an error naming the missing channels
- [x] 1.7 `reflect_values.rs` `an_opaque_hole_loads_and_a_bound_one_fails_naming_it`: rename to `an_opaque_hole_loads_and_a_bad_colour_fails_naming_it`, drop the bound-colour error case

## 2. Per-field WARN

- [ ] 2.1 New `crates/bevy-ts/src/bound.rs` (`mod bound` in `lib.rs`): `BoundFields` resource, per `ComponentId` the bound entities, each field (column offset and size, or hole name) and site (label, `SourceLocation`)
- [ ] 2.2 `crates/bevy-ts-tsx/src/lib.rs`: delete `warn_two_writers`; under dev, rebuild `BoundFields` from `TsxTree::reactive.targets` after each reconcile, each slot apply (`host.rs` `slot_apply`) and on restart
- [ ] 2.3 `crates/bevy-ts/src/bevy_system.rs` `TsBevySystem`: at `initialize`, resolve the write set to ids and declare a read of `BoundFields` when dev; in `run_unsafe`, snapshot bound columns before `run_body`, compare after, and drain the run's hole hits; warn once per (system, component, field, site) per generation
- [ ] 2.4 The host op that writes a hole from a TS system (`REFLECT_WRITE`, from `core.js` `reflectSet`, in `crates/bevy-ts/src`): under dev, compare each bound hole of the entity before and after the write and record a hit; if hole writes take more than one path, snapshot holes in 2.3 instead and say so in design.md
- [ ] 2.5 WARN tests in `crates/bevy-ts-cli/tests/it/tsx_sites.rs`, read from `ScriptLog`, waiting by frames with a deadline: `writing_texture_atlas_next_to_a_bound_sprite_colour_does_not_warn`; `writing_the_bound_sprite_colour_warns_once` (names the system, `Sprite.color`, the entity and the site's file and line; one warning over 10 frames); `a_bound_translation_warns_only_the_system_that_writes_it` (one system writes `scale`, another `translation`); `no_warning_without_dev`
- [ ] 2.6 Ignored release bench in `tsx_sites.rs` `bound_field_check_costs_under_half_a_millisecond`: 10,000 sprites with a bound `Transform.translation`, one system writing `scale` on all of them, dev on vs off; write the number into design.md decision 4

## 3. Docs and specs

- [ ] 3.1 `docs/native.md` "Reflected values in TSX": hole props bind (drop "a hole prop takes a value, not a binding" and "A layout-tier hole prop (below) takes a value only"); a line on the dev WARN being per field
- [x] 3.2 `docs/research/bevy-coverage.md`: row "Layout-tier holes" gains binding and its tests; row "Reflect-tier TSX props bound to a function" drops "a layout-tier hole takes a value only"; the "Docs and specs that claim more than the code" line about script-2d keeps only reflect-tier wording
- [x] 3.3 Cargo jobs through `~/dev/abyme/scripts/gate-slot`
- [ ] 3.4 Sync the deltas into `openspec/specs` at archive
