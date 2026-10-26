## 1. Read types in the generator

- [ ] 1.1 `crates/bevy-ts-cli/src/types.rs`: `ts_type_inner` takes a mode; `ts_read_type` reads as `reflect_value::read` gives (design 1)
- [ ] 1.2 `types.rs`: `hole_members` emits `get`/`set` pairs where read and write differ (design 2, 3)
- [ ] 1.3 `types.rs`: reflect-tier classes (struct, newtype, enum, resource) emit the row type as `ReflectClass`'s third argument (design 4)
- [ ] 1.4 `types.rs` unit test: `Color` in read mode, expanded through the registry, equals `index.d.ts`'s `ColorRead` (design 7)

## 2. Declarations

- [ ] 2.1 `packages/bevy-ts/index.d.ts`: `ColorRead`; `ReflectClass<V, F, A>` and its row brand; `ReflectRow` reads the row type; `Res`/`ResMut` gain holes and the row type (design 4, 5)
- [ ] 2.2 Regenerate `packages/bevy-ts/bevy.d.ts`, `packages/bevy-ts/test/engine/*.d.ts` with `scripts/gen-bevy-types.sh`

## 3. Tests

- [ ] 3.1 `packages/bevy-ts/test/reflect.ts`: under `Mut<Sprite>`, `const i: number = sprite.texture_atlas!.index` checks and `sprite.texture_atlas = { index: "3" }` checks; `light.color` reads as `ColorRead` and takes `"#fff"` on `Mut`, refuses it on a read row; a reflect-tier resource and a layout resource hole on `Res`/`ResMut`; `<Sprite texture_atlas={{ index: "3" }}>` still checks
- [ ] 3.2 `tsc -p test` and the examples pass (`pnpm check`), timed before and after

## 4. Docs

- [ ] 4.1 `docs/2d.md` and `examples/shapes-2d/src/game.tsx`: drop `Number()`
- [ ] 4.2 `docs/native.md` "Reflected rows in TS systems": the table of what each tier reads and takes, and the out-of-range exception
- [ ] 4.3 `docs/reflection.md` matrix: a "TS read type" column
- [ ] 4.4 `docs/research/bevy-coverage.md`: the coverage row for read types
