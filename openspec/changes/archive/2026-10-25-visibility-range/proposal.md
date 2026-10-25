## Why

Heimdall's scale probe could not insert `VisibilityRange`, Bevy's HLOD component, from TSX, levels or BRP. Within-row level of detail waits on it.

Two Bevy gaps cause it, both in Bevy 0.20 and on Bevy `main` today:

- `VisibilityRange` derives `Default` but its `#[reflect(...)]` omits `Default`, so no `ReflectDefault` is registered. TSX and levels build a component from `ReflectDefault` and refuse it: "VisibilityRange has no reflected Default".
- `Range<f32>` reflects as opaque with `Clone` only (`bevy_reflect` `impls/core/ops.rs`), with no `ReflectSerialize` or `ReflectDeserialize`. Stock `bevy_remote` and scenes deserialize through serde type data, so BRP `world.insert_components` refuses it.

Other Bevy components hit the same two gaps. This change closes the category, not one component.

## What Changes

- bevy-ts registers the type data Bevy leaves out, the Bevy way (`App::register_type_data`), in one place: `ReflectDefault` for Bevy components that implement `Default` but do not reflect it, and `ReflectSerialize`/`ReflectDeserialize` for numeric `Range<T>` and `RangeInclusive<T>`, whose serde form is `{ start, end }`.
- reflect-coverage's hole check gains one gap kind, `no default`: a registered component TSX cannot insert (no `ReflectDefault`, no `ReflectFromWorld`). Every such component is fixed by the table above or listed with a reason. A new Bevy gap fails CI.
- `VisibilityRange` is typed in `bevy-ts/bevy` (no longer `unknown`) and documented with an HLOD example.
- A render test proves an HLOD cross-fade from TSX: a detail mesh and its impostor swap by camera distance, both drawn inside the margin.

Not in this change: the `{ start, end }` JSON shape, typing and default of `Range<T>` in `reflect_value`. reflect-coverage (the reflect-shapes branch) owns them and has built them. This change depends on it and does not repeat them.

## Capabilities

### New Capabilities

None.

### Modified Capabilities

- `native-components`: Bevy type data that Bevy omits is registered once; every reflected component is insertable from TSX or listed; `VisibilityRange` inserts from TSX, levels and BRP.

## Impact

- Code: a new `crates/bevy-ts/src/type_data.rs` (registered by `BevyTsPlugin`); `crates/bevy-ts-cli/src/coverage.rs` (the `no default` kind); a render test in `crates/bevy-ts-cli/tests/it/`; `docs/reflection.md`; regenerated `packages/bevy-ts/bevy.d.ts`.
- Depends on reflect-coverage landing first (its `Range` arm and hole check).
- Performance: none per frame. Type data is registered once at startup.
- A new place for upstream-gap type data. It is Bevy's own mechanism, but it is a new file other teams will add to: the lead decides whether that needs the operator's go.
- Optional, outward-facing: an upstream Bevy PR adding `Default` to `VisibilityRange`'s `#[reflect]` and serde type data to numeric ranges. Needs the operator's go; this change does not wait on it.
