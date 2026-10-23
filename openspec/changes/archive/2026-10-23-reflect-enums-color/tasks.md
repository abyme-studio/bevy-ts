## 1. Colour parser

- [x] 1.1 Add `crates/bevy-ts/src/color.rs` with `parse(&Value) -> Result<Color, String>`: hex (`#rgb`, `#rgba`, `#rrggbb`, `#rrggbbaa`), `[r, g, b, a?]` sRGB, and Bevy's reflected `Color` shape for every variant with `alpha` defaulting to 1; unit tests per form and for the missing-channel error
- [x] 1.2 Point `ui.rs::color`, `intrinsics.rs::color`, `recipes.rs::color` and `bevy-ts-material/src/layout.rs::color` (still returning linear `[f32; 4]`) at it; delete the copies' parsing; existing tests stay green

## 2. Reflected-value writer

- [x] 2.1 Add `crates/bevy-ts/src/reflect_value.rs`: `write(registry, server, slot, base, json)` per design decision 1 (Color, Handle, Option, enum unit/newtype/struct/tuple variants, struct merge, deserializer fallback), with the `Handle<T>` rule moved from `reflected.rs::field_value`
- [x] 2.2 Base from a fresh `ReflectDefault` of the component at the same path; fall back to the current value without one (decision 2)
- [x] 2.3 Errors name the type, the unknown key or variant, and the variants or missing fields
- [x] 2.4 Unit tests: `Projection` Perspective → Orthographic with one field, nested `ScalingMode` unit and data variants, `Option` null and value, a dropped field returning to its default, a typo'd key

## 3. TSX paths

- [x] 3.1 Rebase on bevy-ui-coverage if it has landed (it had not: its newtype rule is its own to add); keep its `value` → `.0` newtype mapping in `reflected.rs::set()` and add `value` → whole component for enum components (decision 4); `set()` calls `reflect_value::write`
- [x] 3.2 `NativeComponent` records its writable holes (not opaque); `resolve` splits a layout-tier element's hole props into a `reflected` map on `Resolved`; an opaque hole or a bound hole prop fails to load naming the field
- [x] 3.3 `reconcile::insert` writes hole props through reflection after the bytes; `update` writes a changed hole prop and tracks its provenance like a controlled reflect-tier field
- [x] 3.4 `camera3d` takes `projection` through `reflect_value::write` on a default `Projection`; update its `spawnNative` overload in `packages/bevy-ts/index.d.ts`

## 4. Types and TS helpers

- [x] 4.1 `types.rs`: `bevy_color::color::Color` → `ColorValue`; layout-tier classes declare writable holes as optional TSX-only props (not on query rows); enum reflect-tier components type `value`
- [x] 4.2 Export `ColorValue`, `srgb`, `linear`, `hsl`, `hex` from `bevy-ts` (types in `index.d.ts`, functions in the V8 JS runtime)
- [x] 4.3 Regenerate `packages/bevy-ts/bevy.d.ts`; check that `<DirectionalLight color>`, `<Projection value>` type-check and the bad forms in the spec do not (in `packages/bevy-ts/test/tsx/reflect.tsx`, run by `pnpm check`: `typescript.rs` runs no `tsc`)

## 5. Headless tests

- [x] 5.1 `crates/bevy-ts-cli/tests/reflect_values.rs`: TSX `<Projection value={{ Orthographic: ... }} />` gives an orthographic clip-from-view; reload to `"WindowSize"` and to `{ Orthographic: {} }`; unknown variant and typo errors
- [x] 5.2 Same file: `<DirectionalLight color>` inserted, reloaded to a new colour writes only `color`; a TS system's `DirectionalLight` row has no `color`; opaque hole error
- [x] 5.3 Same file: every colour form through `<material>`, `<mesh>`, a script material param and the `cuboid` recipe gives the same colour
- [x] 5.4 Rendered test after `viewport_thumbnail.rs` (macOS, offscreen target, no window shown): an orthographic camera draws two equal cubes at different depths the same size; a warm `DirectionalLight` makes a white cube's lit pixel red above blue
- [x] 5.5 `camera3d { projection }` spawns an orthographic camera; a bad variant fails naming the variants

## 6. Docs and landing

- [x] 6.1 `docs/native.md`: enums, `Option`s, colours and writable holes in TSX, the default-merge rule; `docs/materials.md`: the colour forms
- [x] 6.2 `examples/arena`: warm the sun with `color`, to prove the path in a real project
- [x] 6.3 Regenerate `protocol/bevy-ts.openrpc.json`; if it differs, bump `1.24.0` to the next free minor (no difference: no bump)
- [x] 6.4 `cargo test` for the touched crates and `pnpm check` green; validate with `openspec validate reflect-enums-color --strict`
