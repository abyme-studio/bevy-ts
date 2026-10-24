## 1. Before starting

- [ ] 1.1 Confirm reflect-coverage (branch `reflect-shapes`) has landed on main; rebase on it. Its `Range` arm and hole check are not repeated here.

## 2. Type data Bevy omits

- [ ] 2.1 Add `no default` to `crates/bevy-ts-cli/src/coverage.rs`: a registered component with neither `ReflectDefault` nor `ReflectFromWorld`. Run it on the prebuilt engine and record the list.
- [ ] 2.2 Add `crates/bevy-ts/src/type_data.rs`, registered by `BevyTsPlugin`: `ReflectDefault` for `VisibilityRange` and each listed component that implements `Default`, each with the upstream file it covers; `ReflectSerialize` and `ReflectDeserialize` for `Range<N>` and `RangeInclusive<N>` of every numeric `N`.
- [ ] 2.3 Put each listed component that has no Rust `Default` on `reflect_value::UNCOVERED` with its reason. The hole check passes with no `no default` gap.
- [ ] 2.4 A unit test in `crates/bevy-ts`: every entry in the table is still missing upstream (fails when Bevy adds it, so the entry is deleted).

## 3. Proof

- [ ] 3.1 A `bevy-ts-tsx` test: `<VisibilityRange>` with both margins inserts and reads back `{ start, end }`.
- [ ] 3.2 A `bevy-ts-cli` BRP test: `world.insert_components` with a `VisibilityRange` succeeds and `world.get_components` returns it.
- [ ] 3.3 A render test in `crates/bevy-ts-cli/tests/it/visibility_range.rs` (a `mod` line in `main.rs`, `common::render_app()`): red detail and blue impostor from TSX; only red at 4 m, only blue at 20 m, both at 10 m.

## 4. Docs and types

- [ ] 4.1 Regenerate `packages/bevy-ts/bevy.d.ts`; `VisibilityRange` is typed, not `unknown`.
- [ ] 4.2 `docs/reflection.md`: the type data table and how to add an entry; an HLOD example in TSX.
