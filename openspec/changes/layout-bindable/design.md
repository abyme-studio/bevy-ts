## Context

- `crates/bevy-ts-tsx/src/resolve.rs` `resolve` refuses a binding on a prop in `NativeComponent::reflect_holes` ("written through reflection, so it takes a value, not a binding"). A plain hole prop already works: it lands in `Resolved::reflected` and `reconcile.rs` `holes` writes it with `crate::reflected::write(world, e, type_path, field, value)` after the bytes.
- `crates/bevy-ts-tsx/src/host.rs` `patch` writes a bound value: a UI prop, a script material param, a reflect-tier field through `reflected::write`, else a column through its layout field. A hole has no layout field, so it would fail "has no field" even with the refusal gone.
- `crates/bevy-ts-cli/src/types.rs` `hole_props` types each hole `name?: T`. The enum `value` prop is already `T | (() => T)` inline (reflect-bindable); `bevy.d.ts` imports no `Bindable`.
- `crates/bevy-ts-tsx/src/lib.rs` `warn_two_writers` runs after each reconcile under dev: any TS system whose declared `write` set holds a component with a bound field warns, once per site and generation. It has no spec and no test.
- Since 7303c5bbb each TS system is its own Bevy system (`crates/bevy-ts/src/bevy_system.rs` `TsBevySystem`), with `run_unsafe` around `systems::run_body`. That is where one system's writes start and end.
- A TS system writes a column in place through a shared view (no host call per write), and a hole through `reflectSet` (`packages/bevy-ts/runtime/core.js`, the `REFLECT_WRITE` host op), which writes the whole reflected value or the hole at once.

## Goals / Non-Goals

**Goals:** every writable hole kind binds in TSX and in the types; the WARN is per field and per entity; the tests the item names.

**Non-Goals:** reads of holes in systems (already done, one reflected read cached by tick); binding a hole on the uncovered list; catching a Rust system that writes a bound field (Bevy has no hook; the research doc says so); `ref.get` changes.

## Decisions

1. **A bound hole is a reflected write, the same call as a plain hole.** `resolve` stops refusing; a bound name passes if it is a layout field or a reflect hole, else "has no field" lists both. `patch` gains a branch before the column branch: the key is a native component and `prop` is one of its `reflect_holes` → `reflected::write(world, e, &n.type_path, &prop, &v)`. `patch` already returns early when the value equals the last applied one, so an unchanged binding writes nothing. No kind gets code of its own: the writer is `bevy_ts::reflect_value`. Rejected: encoding a hole into the column bytes; holes have no stable bytes.
2. **Types inline the function, as the enum branch does.** `hole_props` emits `name?: T | (() => T)`. Rejected: wrapping `TsxProps<C>` in `jsx-runtime.d.ts`; it would also make non-hole `tsxProps` (asset props) bindable where the runtime refuses.
3. **The WARN checks at the writer, not the declaration.** `bevy-ts-tsx` publishes its bound targets to a new resource in `bevy-ts`, `bound::BoundFields`: per `ComponentId`, the entities with a bound field and for each its field (a column's offset and size, or a hole's name) and its site (label and source location). It is rebuilt only when `TsxTree::reactive.targets` changes, and only under dev. Then, for a `TsBevySystem` whose write set (resolved to ids at `initialize`) meets `BoundFields`:
   - **Columns**: before `run_body`, copy each bound column's bytes per entity; after, compare. A column write has no host call, so a snapshot is the only exact check. Cost: a memcpy of a few bytes per bound entity per run.
   - **Holes**: the `REFLECT_WRITE` handler, under dev, compares each bound hole of that entity before and after its write (`reflect_partial_eq`) and pushes a hit (component, entity, field) to the runtime. Cost: only on a write to a bound entity.
   - After the run, hits become one `Pending::Warn` each, deduplicated per (system, component, field, site) for the generation, at the site's location.
   `initialize` declares a read of `BoundFields` when dev, so Bevy's access stays exact. `warn_two_writers` goes. Rejected: per component (today: false warnings); a static field list from the TS source (misses `Object.assign`, in-place edits, writes through helpers); a change-tick filter instead of a snapshot (blames a TS system for a Rust system's earlier write).
4. **Per entity, because the cost is fair.** Release and `dev(false)` skip it whole (one bool). In dev, a system that writes no component with a bound field pays one empty lookup. The worst case measured is 10,000 sprites with a bound column written each frame by one system: the check SHALL add under 0.5 ms per run in a release build, measured by an ignored bench in `tsx_sites.rs`, its number written here before land.

## Risks / Trade-offs

- [A binding and a system both own a hole for a frame before the WARN] → dev only, and the warning names both; the runtime keeps "last writer wins", as for columns.
- [`BoundFields` drifts from `TsxTree`] → rebuilt from `reactive.targets` after each reconcile and each slot apply, and dropped on restart; an entity despawned since is skipped.
- [The snapshot misses a write made by a native function the system calls] → it runs inside `run_body`, so the snapshot spans it.
