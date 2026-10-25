## Context

`crates/bevy-ts-tsx/src/reflected.rs` `insert` builds a component from `ReflectDefault` and sets props over it. `VisibilityRange` (`bevy_camera` 0.20, `visibility/range.rs:78`) is `#[derive(Default, Reflect)]` with `#[reflect(Component, PartialEq, Hash, Clone)]`: Rust has a default, reflection does not. Its fields are `Range<f32>`, which `bevy_reflect` registers opaque with `Clone` only. Stock BRP (`bevy_remote`, served by `BevyTsRemotePlugin`) deserializes with `TypedReflectDeserializer`, which needs `ReflectDeserialize` for an opaque type.

reflect-coverage (branch `reflect-shapes`, team reflect-enums-color) already gives `reflect_value` a `Range<T>` arm: read and write as `{ start, end }`, a `0..0` default, TS `{ start: number; end: number }`, and a hole check over every registered type. `bevy.d.ts:194` on that branch already types `VisibilityRange`. What it does not fix: `ReflectDefault` on the component, and serde type data for stock BRP and scenes.

## Goals / Non-Goals

**Goals:**
- `<VisibilityRange start_margin={{ start: 0, end: 10 }} end_margin={{ start: 40, end: 50 }} />` inserts and fades.
- BRP `world.insert_components` and scenes take `VisibilityRange`.
- Every reflected component can be inserted from TSX, or the hole check names why not.

**Non-Goals:**
- `Range<T>` JSON shape, typing or default in `reflect_value` (reflect-coverage's).
- A bevy-ts LOD feature. Bevy's `VisibilityRange` is the feature; the cut between rows is Heimdall's.
- Making TSX insert a component with no default by reading every field. That would be a second path; the gap is Bevy's type data.

## Decisions

1. **Register Bevy's missing type data, don't special-case it.** One module, `bevy_ts::type_data`, calls `app.register_type_data::<T, ReflectDefault>()` for each Bevy component that implements `Default` without reflecting it, and `register_type_data::<Range<N>, ReflectSerialize>()` / `ReflectDeserialize` for every numeric `N` (the same list reflect-coverage's arm covers). Every consumer (TSX, levels, BRP, scenes, the editor's `constructible` flag) gains it with no change. Each entry names the upstream file and line it covers, and is deleted when Bevy reflects it. Alternative rejected: a `Default` fallback in `reflected.rs`; it fixes TSX only.
2. **The list comes from the hole check, not from memory.** The `no default` kind in `coverage.rs` lists every registered component without `ReflectDefault` or `ReflectFromWorld`. The implementer runs it once, adds each that implements `Default` to the table, and puts the rest on `reflect_value::NO_DEFAULT` with a reason (for example, a required `Entity`). Not `UNCOVERED`: its types are neither read nor written, and these read and write fine. Resources are skipped; TSX inserts components. After that the check fails on any new one.
3. **Serde form equals the JSON shape.** serde writes `Range` as `{ "start", "end" }`, the same shape reflect-coverage's arm reads and writes, so BRP and TSX agree. `reflect_value`'s `Range` arm stays first; nothing in it changes.
4. **Proof in an app that renders.** The cross-fade is a dither in the PBR shader, so the test renders (docs/testing.md rule 5): a TSX scene with a red detail cube (`end_margin` 8..12) and a blue impostor (`start_margin` 8..12) at one spot, an offscreen camera, as `reflect_render.rs` does. At 4 m only red, at 20 m only blue (by `ViewVisibility` and by pixels), at 10 m both colours in the frame. Started from `common::render_app()`.

## Risks / Trade-offs

- A Bevy upgrade that adds the type data upstream makes an entry redundant → registering it twice is harmless; the module's test fails when an entry is already reflected, so it is deleted.
- Lands only after reflect-coverage → if reflect-coverage stalls, this waits. Its `Range` arm is built; only landing remains.
- Dither at a margin is noisy per pixel → assert counts of red and blue pixels over thresholds, never exact pixels.
