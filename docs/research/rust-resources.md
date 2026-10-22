# Rust resources from TS systems

Research for the rust-resources team, 2026-10-23. Goal: `Res<T>`/`ResMut<T>` of Bevy's own
resources from TS systems, with no recipe. Audit row 5 of `bevy-coverage.md`.

## Answer

Only `Time` works, and it works by a special case, not by the resource path. Every other Rust
resource fails at load with "no resource named X". The `ts-systems` spec already promises the
general case ("A Rust resource with a native layout SHALL be reachable the same way",
`openspec/specs/ts-systems/spec.md:134`). The engine half is a small fix; the pieces it needs
(native layouts for resources, the zero-copy row frame, change ticks) exist already.

Confidence: high on the code path (read end to end). Medium on each resource's tier in the table
below: it is read from Bevy 0.20's struct fields, not from a `bevy-ts types` run. Spec should run
`bevy-ts types` and paste the real tiers.

## How Time reaches TS today

1. The compile step reads `Res<Time>` as `{ kind: "res", resource }`
   (`crates/bevy-ts-cli/src/reader.rs:1075-1077`).
2. JS checks the class is a resource (`crates/bevy-ts-v8/js/core.js:907-912`). `Time` is a
   hand-written resource class (`core.js:538`), typed by hand in `packages/bevy-ts/index.d.ts:400`.
3. Rust resolves the param. `Time` (or any name ending `::Time`) becomes `RunParam::Time`; every
   other name goes to `ts(resource)`, which only knows the generation's TS components
   (`crates/bevy-ts/src/systems.rs:447-457`, lookup built at `generation.rs:366-368`).
4. Each run, `RunParam::Time` copies `dt` and `elapsed` from `frame_info` (`runtime.rs:201-205`)
   into a scratch buffer and hands JS a 24-byte row (`systems.rs:768-775`, `805-811`). Not the
   resource's memory: a copy.
5. TS resources take the real path: `RunParam::Res` finds the resource's entity, points the frame
   at its column row, and marks change ticks for `ResMut` (`systems.rs:748-766`). Zero copy.

## Why other Rust resources don't work

Three gaps, all on the same seam:

| Layer | Gap | Where |
|---|---|---|
| Param resolution | `Res`/`ResMut` only look up TS resources; Rust names fail | `systems.rs:455` |
| Shipped types | `bevy-ts/bevy` skips every resource, so scripts cannot import one | `crates/bevy-ts-cli/src/types.rs:327` |
| JS class | Generated native classes are components; `res` needs `KIND === "resource"` | `core.js:494`, `core.js:909` |

What already works: `layout_reflect::fill` gives resources native layouts, since in Bevy 0.20 a
resource is a component (`layout_reflect.rs:332`; `Time<()>` is proven layout tier at
`layout_reflect.rs:536`). Project `native.d.ts` already emits layout-tier resources as
`Resource` classes (`types.rs:269`, `414`). The row frame and tick marking in `RunParam::Res`
work for any component id with a resource entity.

The fix: resolve a Rust resource name through `NativeLayouts` as `resolve_ref` does for query
data (`systems.rs:169-185`), refuse `ResMut` when the layout is not writable, emit resource
classes into `bevy-ts/bevy` with the resource kind, and test read, write and change detection
against a Rust system.

## Resources a TS system should reach

Tiers: layout = a native layout with plain-data leaves, readable and writable in place by the
generic path. Reflect = no plain-data leaf, so only through reflect-coverage stage 3.
Holes are fields (such as `Color`) that only reflection can touch.

| Resource | Shape (Bevy 0.20) | Tier | Owner |
|---|---|---|---|
| `Time` | f32/f64 seconds, `Duration` holes | layout (special case today) | path: us; semantics: script-time |
| `Time<Real>`, `Time<Virtual>`, `Time<Fixed>` | as `Time` plus context | layout, fields read-only | script-time, on our path |
| `Time<Virtual>` pause, speed; `Time<Fixed>` timestep | methods | exported param | script-time via writable-native-params |
| `GlobalAmbientLight` | `color: Color`, `brightness: f32`, `bool` | layout, `color` a hole | us (brightness); `color` via reflect stage 3 |
| `UiScale` | `f32` | layout | us |
| `ClearColor` | `Color` only | reflect | reflect-enums-color (stage 3) |
| `DirectionalLightShadowMap`, `PointLightShadowMap` | `usize` (opaque) | reflect | reflect-enums-color (stage 3) |
| `ButtonInput<KeyCode>` | HashSets | typed param | text-input (`Keys`) |
| `AccumulatedMouseMotion`, `AccumulatedMouseScroll`, `MouseScrollPixelsPerLine` | `Vec2`, `f32` | layout | path: us; docs and tests: script-input-devices |
| `ButtonInput<MouseButton>`, `Touches`, `Window::cursor_position` | HashSets, no `Reflect`, methods | typed param | script-input-devices |
| Gamepads | entities with `Gamepad` components, not a resource | query | script-input-devices |
| `Window`, `PrimaryWindow` | components on the window entity, not resources | query; enum fields reflect | reflect-enums-color |
| `Assets<T>` | handle store, methods | exported param | writable-native-params; per-asset teams |
| Project and port resources (e.g. Rapier) | any layout | layout | us (same path) |

Our docs table lists every row; reflected rows point at `docs/reflection.md`.

## Boundaries agreed

All replied and agreed by message on 2026-10-23.

- reflect-enums-color (lead): they own `reflect_value` read/write, the JSON shape per kind, the
  hole check, and reflect-tier `Res`/`ResMut` (reflect-coverage task 3.2). That stage waits on the
  operator's go, so nothing here may build on reflected `Res` reads. We own layout-tier `Res`/
  `ResMut` of Rust resources, the resource coverage table, and typed bindings no one else owns.
- script-time: we own the generic param path; they own what is Time-specific on top (the
  `FixedUpdate` swap, `Time<Real/Virtual/Fixed>`, timers, `on_timer`, Time typings), and whether
  `RunParam::Time` folds onto the generic path.
- script-input-devices: `ButtonInput<MouseButton>`, touches, gamepads and cursor position stay
  theirs as exported params with Bevy's methods. The plain-data mouse resources
  (`AccumulatedMouseMotion`, `AccumulatedMouseScroll`, `MouseScrollPixelsPerLine`) get no
  exported param: they go through our `Res<T>`, and script-input-devices documents and tests them
  once it lands.
- writable-native-params: they own the exported-param path (`#[export]`, `HostParams`). Line: a
  resource with a native layout reaches TS as `Res`/`ResMut` (us); one without a layout, or whose
  API is methods (`Assets<T>`, `Time<Virtual>::pause`), goes through their path.
- ambient-light-script: owns no resource or param, only `encodeWithLayout` default bytes.
  `GlobalAmbientLight` is ours. Our path does not encode through `encodeWithLayout` (it writes in
  place), so no ordering dependency.
- bevy-ui-coverage, mesh-primitives, script-gltf, text-input, world-to-screen, material-alpha: no
  `Res` overlap found. text-input's `Keys` is the `ButtonInput<KeyCode>` binding.

## Should it become a change

Yes, a small one: "Rust resources as `Res`/`ResMut`" in `ts-systems` and `native-components`.
Files: `systems.rs` (resolution), `types.rs` and `bevy.d.ts` (resource classes), `core.js` and
`packages/bevy-ts/runtime/core.js` (resource kind for native classes), a test in
`crates/bevy-ts-cli/tests`, `docs/native.md` (the coverage table).

## Open questions

- None for the operator. No new dependency or pattern; the path is the existing zero-copy row, so
  no per-frame cost beyond what TS resources pay today.
- For spec: should a `Res<R>` whose Rust resource is absent fail the run (today's TS-resource
  behaviour, `systems.rs:762-764`) or skip the system like Bevy's `Option<Res<R>>`? Recommend
  keep today's behaviour plus `runIf(resourceExists(R))`, which already works by id.
- For spec: ship short names (`GlobalAmbientLight`) in `bevy-ts/bevy` the way components ship,
  colliding names prefixed by crate (`types.rs:276-289`).
