## 1. Compile step: the annotation reader

- [x] 1.1 Read exported function signatures from oxc's AST: inline param types, `Mut`/`Option`/`Entity` data, `With`/`Without`/`Changed`/`Added` filters (single or tuple), `Res`/`ResMut`/`Local`/`Commands`/`MessageReader`/`MessageWriter`/`State`/`NextState`/`On`/`App`
- [x] 1.2 Follow type aliases (generic ones by substituting arguments as written), interfaces and object types as param bundles, and relative imports, with names scoped to the module whose text writes them
- [x] 1.3 Append `__bevyts.bind` and `__bevyts.declare` after the module's code with identities `<root-relative path>#<export>`; leave bodies and line numbers unchanged; check breakpoints still map
- [x] 1.4 Errors with file and line: computed types (`ReturnType`, `Pick`, conditional), names imported with `import type`, non-exported registered functions; carried lazily as `error` declarations
- [x] 1.5 A port param-kind registry in the compiler crate (empty for now) and `export const x = system(...)` identities
- [x] 1.6 Unit tests against `bench/dx/examples/systems.ts` and `fallback.ts` (adapted to real imports): the declarations match the prototype reader's output

## 2. Declarations in the runtime

- [x] 2.1 `Component(name, fields)` and `Tag(name)` as class bases with static `componentName` and `source`; `new C({...})` values for commands; field types as values and TS types
- [x] 2.2 `Resource`, `Message`, `Event`, `EntityEvent({ propagate })` and `States(name, values)` classes; `S.Value` values
- [x] 2.3 Resolve declaration refs at the end of load; manifest entries for resources, messages, events, states and the new system shape (`id`, `schedule`, `params`, `conditions`, `read`/`write` union)
- [x] 2.4 Remove `app.system`, `app.startup` and `component()`; keep `section`, `app.entry`, `Commands.custom`/`mount`/`unmount`

## 3. Systems over the new seam

- [x] 3.1 `ParamFrame` in the backend trait and the fake backend; Rust builds one query state per query param, with `Option` columns and `Entity` data
- [x] 3.2 Per-identity last-run ticks; `Changed`/`Added` as per-table row lists from tick scans (no summary-tick skip: Rust `Mut` writes do not update summary ticks)
- [x] 3.3 JS query objects: `each(...row)`, `for…of`, `single`, `get(entity)`, `len`, `tick`, `tables` with `rows`; overlapping queries alias one column; the dev guard hashes only unwritten columns
- [x] 3.4 Read-only accessors for read data; glam-style vector methods (`set`, `copy`, `add`, `sub`, `scale`, `addScaled`, `normalize`, `lerp`, `subVectors`, `rotate`, `length`, `dot`, `distance`; quat `copy`, `lookTo`, `rotateY`, `slerp`) and `vec3()`/`quat()` scratch values
- [x] 3.5 Commands: `spawn(...instances | element)`, `entity(e).insert/remove/setParent/despawn`, `trigger`, `write`, `spawnNative`
- [x] 3.6 Param bundles rebuilt in JS; the "compiled without bevy-ts" error at registration
- [x] 3.7 Tests: the spec scenarios for queries, overlap, Changed precision, one call per run, bundles, not exported, compiled without bevy-ts

## 4. Resources and Local

- [x] 4.1 TS resources as dynamic components on resource entities; `insertResource` at apply when absent; values kept across reloads
- [x] 4.2 `Res`/`ResMut` one-row views; `Res<Time>` computed view (`delta`, `elapsed`, `frame`); `ResMut<Time>` refused
- [x] 4.3 `Local` storage keyed by identity, migrated by field name, reset on restart
- [x] 4.4 Tests: score scenario, Local survives an edit, rename resets Local

## 5. Scheduling

- [x] 5.1 `app.addSystems(schedule, ...)` with `Startup`, `PreUpdate`, `Update`, `PostUpdate`, `FixedUpdate`; `systems()`, `chain()`, `.after`, `.before`, `.inSet`, `.runIf`, `systemSet`
- [x] 5.2 Run conditions `inState`, `resourceExists`, `onMessage`, `not` evaluated in Rust; a skipped system makes no JS call
- [x] 5.3 `Startup` systems once per identity per world; restart forgets them
- [x] 5.4 Tests: chain order, set ordering, cycle error, skipped system zero calls

## 6. Messages

- [x] 6.1 Message arenas in Rust, double-buffered, swapped in `First`; `app.addMessage`
- [x] 6.2 `MessageWriter.write`/`next` through command ops; `MessageReader.each`/`for…of`/`len` with per-identity cursors
- [x] 6.3 Tests: two readers see the same messages once; messages expire after two frames

## 7. Observers

- [x] 7.1 `app.addObserver(fn)` for `On<E>`, `On<EntityEvent>` and `On<Add|Insert|Remove<C>>`; Bevy lifecycle observers queue Rust-side triggers
- [x] 7.2 The observer queue drained after each TS system's commands and at each stage start; `commands.trigger` ops; propagation up `ChildOf` with `propagate(false)`
- [x] 7.3 Tests: the explode scenario; a lifecycle observer on a Rust insert runs at the next stage boundary

## 8. States

- [x] 8.1 State machines in Rust; `app.initState`; `NextState.set` and `State.current`
- [x] 8.2 One exclusive system in `StateTransition` running `OnExit` then `OnEnter` TS systems
- [x] 8.3 Tests: the entering-Playing scenario

## 9. Plugins and the fallback

- [x] 9.1 `app.addPlugins(fn)` re-run every generation; `insertResource` and the rest callable from plugins
- [x] 9.2 `system(setup)` with `query(...).with/without/changed/added`, `res`, `resMut`, `commands`, `local`; declaring calls outside a setup fail with their location
- [x] 9.3 Tests: a plugin's new system on reload keeps its resources; the computed-type error and its fallback

## 10. Types, tooling and protocol

- [x] 10.1 Move `packages/bevy-ts` types to the ruled surface (`bench/dx/bevy-ts/mod.ts`), keep `bevy-ts/protocol` and the JSX types; type tests from `bench/dx/examples`
- [x] 10.2 `bevy-ts transform <file>`; Vite, vitest and esbuild plugins in `packages/bevy-ts`; `bevy-ts/testing` stand-in; a vitest test that calls a system
- [x] 10.3 Protocol 1.1.0: `SystemInfo` gains `id`, `schedule` and `params`; regenerate

## 11. Migration

- [x] 11.1 Move the V8, CLI and protocol tests to the new surface
- [x] 11.2 Rewrite `examples/cube/src/game.ts` in the ruled form; walk all six demo steps headless; rerun `bob_bench` and update the recorded numbers (walk: all six steps pass on release; bob_bench rerun pending a quiet machine, see docs/research/change-detection.md)
- [x] 11.3 Tell bevy-ts-tsx when the surface is on main (main 8871413)
