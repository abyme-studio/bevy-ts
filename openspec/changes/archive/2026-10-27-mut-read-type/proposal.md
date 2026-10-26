## Why

A reflected value in a TS system is typed as TSX writes it, not as a row reads it. Under
`Mut<Sprite>`, `sprite.texture_atlas.index` is `number | string | undefined`, so the sprite-sheet
pattern in `docs/2d.md` reads it through `Number()`. `light.color` reads as `ColorValue` (a string,
a tuple or Bevy's shape) though it always reads in Bevy's shape. Every hole and every reflect-tier
field has the same gap: the read type is the write type.

## What Changes

- `bevy-ts types` emits two types per reflected member: the type a row reads (what
  `bevy_ts::reflect_value::read` gives) and the type an assignment takes (what `write` takes).
  A member whose two differ is a get/set accessor pair; one whose two agree stays a plain field.
- Layout-tier holes (`Holes<...>`) and reflect-tier rows (`value` and a struct's fields) read with
  the read type on every row kind: query data, `Mut`, `Ref`, `Option`, `Res`, `ResMut`,
  `ref.get`. Assignment on `Mut` and `ResMut` keeps the write type. TSX props and `.of()` keep the
  write type.
- `Res`/`ResMut` of a layout-tier resource gain its holes, which the runtime already gives.
- The read type of a 64-bit integer is `number`, of a float `number`, of a `Color` `ColorRead`
  (Bevy's shape), of a `Handle<T>` its path, ref or `null`; struct fields are all present.
- `docs/2d.md` drops `Number()`; `docs/native.md` gains a table of what each tier reads and takes.
- No runtime change and no protocol change.

## Capabilities

### New Capabilities

None.

### Modified Capabilities

- `ts-systems`: "Reflected rows in TS systems" says what a reflected member reads as and what it
  takes.

## Impact

- `crates/bevy-ts-cli/src/types.rs` (the read type, the generated members).
- `packages/bevy-ts/index.d.ts` (`ColorRead`, `ReflectClass`, `ReflectRow`, `Res`, `ResMut`).
- Generated: `packages/bevy-ts/bevy.d.ts`, `packages/bevy-ts/test/engine/*.d.ts`.
- Type tests in `packages/bevy-ts/test/reflect.ts`.
- Docs: `docs/2d.md`, `docs/native.md`, `docs/reflection.md`, `docs/research/bevy-coverage.md`.
- `examples/shapes-2d` if it reads `Number(atlas.index)`.
