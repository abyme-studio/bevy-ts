## Context

`Res<R>`/`ResMut<R>` in a TS system resolves in `build_one` (`crates/bevy-ts/src/systems.rs:447-457`). `Time` goes to `RunParam::Time`, a per-run copy. Every other name goes to `ts(resource)`, which knows only the generation's TS types, so a Rust name fails. At run time `RunParam::Res` (`systems.rs:748-766`) already works for any component id with a resource entity: it points the frame at the row and, for `ResMut`, at the change tick.

Native layouts already cover resources: `layout_reflect::fill` tiers every `ReflectComponent` type, and a Bevy 0.20 resource is one. The resolution JSON sent to JS (`generation.rs:397-402`) already carries every layout-tier type by path and TS name. What is missing: `NativeLayouts` does not say which entries are resources; `bevy-ts/bevy` skips resources (`types.rs:327`); JS native classes are always `KIND = "component"` (`core.js:509`), and `res` checks `KIND === "resource"` (`core.js:909`).

## Goals / Non-Goals

**Goals:** layout-tier Rust resources as `Res`/`ResMut` with no copy; the same refusals and naming as query data; `Option<Res<R>>` as Bevy has it; resource classes in `bevy-ts/bevy`; a coverage table.

**Non-Goals:** reflect-tier `Res` (reflect-coverage stage 3); anything about `Time` (script-time); input params with methods (script-input-devices); `Assets<T>` and method APIs (writable-native-params).

## Decisions

1. **Resolve through `NativeLayouts`, after TS.** `build_one` tries `ts(name)`, then `native.get(name)` (path or unambiguous short name), as `resolve_ref` does for query data. A hit gives `(id, layout.size)` into the unchanged `RunParam::Res`. One lookup path for components and resources; no second registry.

2. **A `resource: bool` on `NativeComponent` and `ReflectTier`.** Set in `layout_reflect::tier` from `ReflectResource`. Bevy 0.20 has no resource flag on `ComponentInfo` that holds before insertion (`IsResource` exists only on a live resource entity), so the type registry is the source. A derive-only type has no registration, so `ts_resource_layout::<T: NativeLayout + Resource>()` registers it with the flag set; the bound proves it. Rejected: deciding at run time from `world.resource_entities()`: a typo'd component name would then pass the load and fail every run.

3. **Refusals at load**, in this order: reflect-tier resource (its `reason`); layout-tier but `!resource` ("a component, not a resource"); `ResMut` and `!writable` ("immutable"); `native.ambiguous(name)` (lists paths); else "no resource named R". A layout-tier resource named as query data or in a filter refuses the other way ("a resource, not a component"); `resourceExists` refuses an ambiguous short name as `Res` does. Holes do not refuse `ResMut`: accessors never touch holes, as for `Mut<C>`. A resource row frame has no raw table view, so the holes' raw-write rule does not arise.

1a. **Precedence (agreed with script-time and writable-native-params, 2026-10-23):** the `Time<..>` route, then exported params claiming a TS spelling (`HostParams` claims), then this generic path. The reader resolves claims (a claimed spelling reads as the export's port param), so `build_one`'s generic path never sees a claimed spelling.

1b. **A Rust resource's row is in a sparse set.** `#[derive(Resource)]` stores the component as `SparseSet`, unlike a TS resource's table column. `RunParam::Res` reads the row and its changed-tick cell from `ComponentSparseSet::get_with_ticks` when the id has a sparse set, and from the table otherwise. Same zero-copy frame either way.

4. **`Time` keeps its route.** script-time owns it and chose a per-run value for all four clocks (`docs/research/script-time.md` §1). The route (script-time's `Clock::of`) takes every `Time` and `Time<..>` spelling and runs before decision 1, so the generic path never sees a `Time`; `bevy-ts/bevy` emits no class for the clocks.

5. **Missing resource: as Bevy.** Bevy 0.20's `Res<T>` fails param validation when absent and the default error handler panics; `Option<Res<T>>` gets `None`. Here `Res`/`ResMut` keep today's run failure (`systems.rs:762`), which is our panic: the error is logged with the system's location and the generation keeps running. `Option<Res<R>>`/`Option<ResMut<R>>`:
   - reader (`reader.rs:1075`): `Option<Res<R>>` → `{ kind: "res", resource, optional: true }`.
   - `RunParam::Res` gains `optional`; absent + optional → `ParamFrame::None`, no failure.
   - JS: an optional res param with a `None` frame passes `undefined`.
   - Types (`index.d.ts`): `Res<T>` and `ResMut<T>` carry a phantom brand (`readonly [resBrand]: true`, a declared unique symbol never read), and `Option<T>` maps a branded `T` to `T | undefined`; for anything else it stays the query-data marker. Query data never carries the brand, so `Option<Mut<C>>` is unchanged.
   This applies to TS resources too; one rule for both.

6. **Names: the components' rule.** `named()` in `types.rs` already includes layout-tier resources and prefixes colliding short names by crate. `emit_bevy` stops skipping them and writes `export class GlobalAmbientLight extends nativeResource("bevy_light::ambient_light::GlobalAmbientLight", {...}) {}` and a `.d.ts` class extending `Resource`. Bevy names everything else; `nativeResource` is the one new export, the sibling of `nativeComponent`.

7. **`nativeResource(name, fields)` in core.js** shares `nativeComponent`'s body with `KIND = "resource"` (one private factory, two exports). `commit` already resolves `NATIVE` classes from `res.components`, which holds every layout-tier type, so no resolution change. `packages/bevy-ts/runtime/core.js` is synced by `sync-runtime.mjs`.

8. **Change detection** needs nothing new: `RunParam::Res` with `write` pushes the table and id to `written`, and the existing tick path marks it. The test asserts it against a Rust `Res<R>::is_changed()`.

## Risks / Trade-offs

- [Tiers in the coverage table come from Bevy's struct fields, not a `bevy-ts types` run] → task 5.1 runs `scripts/gen-bevy-types.sh` and the table takes its tiers from the generated `bevy.d.ts`.
- [A short name of a new resource collides with an existing component's short name, renaming a shipped export] → `named()` prefixes both; the `--check` diff shows any rename, and the implementer lists renames in the commit. None expected for the resources in the table.
- [`ResMut` of a resource some Bevy system also writes, such as `UiScale`] → same as `Mut<Transform>` today: TS runs in its own stage, exclusive, so no aliasing.

## Open Questions

None.
