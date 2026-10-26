## Context

Source: `docs/research/bevy-coverage.md` at 50340d9ef. Four small fixes on the reflect tier, one change.

- `jsx-runtime.d.ts` `LibraryManagedAttributes` gives a component element `ElementProps<I> & TsxProps<C>`. `ElementProps` already makes every field `T | (() => T)`, so a reflect-tier struct's fields bind. `TsxProps` comes from `static readonly tsxProps`, written by `crates/bevy-ts-cli/src/types.rs` in two places: `hole_props` (layout-tier holes) and the enum branch of `emit_bevy` (`{ value?: XValue }`). Neither is `Bindable`.
- `resolve.rs` `resolve` refuses a binding on a layout-tier hole and accepts one on `Target::Reflect`, checked when the value arrives. `docs/research/script-2d.md:33` and `openspec/changes/script-2d/design.md` still say reflect-path props cannot be bound.
- `GlobalTransform` is reflect-tier: a tuple struct of `Affine3A`, so its row's `value` is 12 floats (`bevy.d.ts`). `core.js` `HOST.transform` takes `{ value: [12] }` as a `GlobalTransform`.
- `systems.rs` `reflect_term` checks `storage_type() != Table` before `ReflectResource`; Bevy 0.20 stores a resource as a sparse-set component, so a reflect-tier resource gets the sparse-set error. `resolve_ref` checks `n.resource` only for layout-tier types; an untiered resource also reaches `sparse()` first.

## Goals / Non-Goals

**Goals:** the four items in the proposal, each with the test that would have caught it.

**Non-Goals:** binding a layout-tier hole (its write path is the layout write, then reflection once); `GlobalTransform` constructors or `.of` (Bevy computes it; writes stay refused); `TransformHelper`; `UiGlobalTransform` methods; a layout tier for `GlobalTransform`.

## Decisions

1. **Bindable only where the runtime binds.** `types.rs`'s enum branch emits `tsxProps: { value?: Bindable<XValue> }`; `hole_props` is unchanged. `Bindable` is imported into the generated `bevy.d.ts` header from `bevy-ts/jsx-runtime` (or re-exported by `bevy-ts`; the build picks whichever the header already imports from). The alternative, wrapping `TsxProps<C>` in `jsx-runtime.d.ts`, would also type layout holes as bindable, which the runtime refuses.
2. **`GlobalTransform` is a shipped class with a row view, not a new tier.** `index.d.ts` declares `class GlobalTransform extends ReflectComponent` with the read methods; `core.js` (both runtimes) makes `GlobalTransform` a reflect class whose read row wraps the cached `value` in a view whose prototype has the methods. The decomposition ports glam's `Affine3A::to_scale_rotation_translation` (determinant sign on x scale, `Quat::from_mat3` of the normalised columns), so tests compare against Bevy within 1e-5. `emit_bevy` skips `GlobalTransform` and re-exports it, as it does `Transform` and `Time`. Rejected: a layout tier over `Affine3A` (12 `f32` columns, read-only). It would be faster on many rows, but needs a new column kind for `Vec3A`/`Mat3A` padding; nobody reads enough rows to need it yet.
3. **Methods return fresh values.** `translation()` returns a new `Vec3`, as Bevy returns by value. A cached `value` is shared and must not be handed out mutable.
4. **Resource first, then storage.** `reflect_term` checks `ReflectResource` before the storage type; `resolve_ref` checks "is a resource" (layout `n.resource`, or the component info / registry for an untiered id) before `sparse()`. The message is the one layout-tier resources already get: `X is a resource, not a component; take Res<X>`.

## Risks / Trade-offs

- [The generated `.d.ts` changes shape for every enum component] → tsc tests in `packages/bevy-ts/test` cover both a plain and a bound `value`; `reflect_coverage.rs` regenerates and compares.
- [A JS port of glam's decomposition drifts from Bevy] → the read test compares every method against Bevy's on a rotated, scaled, parented entity.
