## 1. Bridge prerequisites (N1 to N5)

- [ ] 1.1 Fill bridge's native layout registry at startup from reflection (decision 5: offsets from a value's field references, bounds and leaf-size checks, enum variant bytes read from the build), make it public, and route every Rust-component check in TS systems through it (N1); `Transform` takes the same path
- [ ] 1.2 Turn `bevy-ts-cli`'s `main` into `bevy_ts_cli::run(native: Option<fn(&mut NativeApp)>)` and make the prebuilt binary a one-line caller (N4)
- [ ] 1.3 Make the exclusive stage system constructible for any schedule and ordering (N3); bridge's four stages use it unchanged
- [ ] 1.4 Add the call-host-function seam operation to the backend trait, the fake backend and `bevy-ts-v8` (deno_core op over views of JS memory) (N2)
- [ ] 1.5 Add engine identity, exposed sets and native functions to `bevy-ts/hello` and `bevy-ts/schema` in `protocol/bevy-ts.openrpc.json`, minor bump, regenerate both sides (N5)

## 2. Native crate and project engine

- [ ] 2.1 Create `bevy-ts-native` with `NativeApp`, the `native!` macro (one per crate, link-time check) and a `bevy` re-export; publish plan at the npm version
- [ ] 2.2 Read `native` from `bevy-ts.json`; generate `.bevy-ts/engine/` (Cargo.toml depending on the native crate by path, `main` calling `bevy_ts_cli::run`)
- [ ] 2.3 Check `Cargo.lock` for exact `bevy-ts-native` and `bevy` versions before building; error naming crate, expected and found
- [ ] 2.4 Compute the engine hash (bevy-ts version, target, profile, features, `Cargo.lock`, workspace manifests, packaged native files); unit-test that TS and asset edits leave it unchanged
- [ ] 2.5 Local cache `.bevy-ts/engines/<hash>/`; `cargo build` on miss with progress passed through; clear error when `cargo` is missing
- [ ] 2.6 Shared cache: directory and `{hash}`/`{target}` URL template, SHA-256 verification before execution, `bevy-ts engine publish`
- [ ] 2.7 `bevy-ts dev`: watch hashed Rust inputs, rebuild in the background, swap engines on success on the same port, load the TS generation as a restart, keep the old engine on failure
- [ ] 2.8 Example `examples/thrusters/`: a native crate adding `bevy_rapier3d` 0.37 and one Rust system; verify a TS-only edit starts no `cargo` and a Rust edit restarts

## 3. Rust components in TS

- [ ] 3.1 `bevy-ts-native-derive`: `#[derive(NativeLayout)]` for components without `Reflect`, mapping field types to bridge field types, `offset_of!` tables (public fields, no `repr(C)`); compile-fail tests for `String`, `Handle`, a private field
- [ ] 3.2 Tier every reflected component at startup by decision 5; TS systems accept layout tier as query data and reflect tier only in `With`/`Without`, with the ts-components error otherwise; accessor-only `bool`/enum/`Entity` fields, immutable components refused
- [ ] 3.3 Test that a TS write to a layout-tier Rust component is seen by a Rust system the same frame, with `Changed` exactly on written rows
- [ ] 3.4 `bevy-ts types`: build the app, stop after `Plugin::build` before `finish`, walk the registry, emit `native.d.ts` deterministically; verify no GPU device is created with `bevy_rapier3d` and `bevy_egui`, and report plugins that need one
- [ ] 3.5 Ship pre-generated `bevy-ts/bevy` types from the prebuilt engine; CI regenerate-and-diff; `bevy-ts dev` regenerates after each project-engine build
- [ ] 3.6 TSX accepts reflect-tier components as elements (patch through bridge's reflected access); test `<Weapon name="laser" />`

- [ ] 3.7 Prove the layout rule on `bevy_rapier3d` 0.37 in the engine: `Velocity`, `ExternalImpulse`, `RigidBody` (enum bytes) and the private `LockedAxes` are layout tier; `Collider` (no `Reflect`) is not; a TS write to `Velocity` moves a body

## 3b. Plugin adoption

- [ ] 3b.1 Read `native.ports` and `native.crates` from `bevy-ts.json`; generate the engine's dependencies and `add_plugins` lines; a project with plugins and no native crate builds and runs
- [ ] 3b.2 Port format: crate `bevy-ts-<name>` plus `bevy-ts/<name>` types; a port registers param kinds for the compile step, TSX constructors, flattened messages, sets and its plugin line
- [ ] 3b.3 Ship the `rapier3d` port: `Collider` constructors, `CollisionEvent` flattened to `{ kind, a, b, sensor }`, `ReadRapierContext` with `castRay`/`castRays`, `PhysicsSet`; example `examples/rapier/` with no Rust in the project
- [ ] 3b.4 Plan the `leafwing-input-manager` port with a dynamic `TsAction(u16)`; record avian and hanabi as waiting on Bevy 0.20

## 4. Ordering against Rust sets

- [ ] 4.1 `app.ts_set(name, schedule, set)`, name uniqueness at startup, bevy-ts's own anchors (`bevy:transform-propagate`)
- [ ] 4.2 Build before and after slots per exposed set at app build, with `BevyTsSlot::before/after` sets for Rust ordering and a non-empty run condition
- [ ] 4.3 Place TS systems by `before`/`after` into slots; validate one set per system, matching stage, and TS-to-TS ordering only within a slot; reload moves systems without touching schedules
- [ ] 4.4 Measure whether a skipped exclusive slot still costs the executor a barrier (Rust systems on both sides, slots empty vs absent); if it does, fall back to creating slots only for names the generation uses and record the result in a research note
- [ ] 4.5 Emit `NativeSet` as a string union in `native.d.ts`

## 5. Native functions

- [ ] 5.1 `app.ts_fn::<P>(name, f)` with `ReadOnlySystemParam` and `NativeLayout` input/output records; a per-function `SystemState<P>` and output arena
- [ ] 5.2 `NativeFn<name>` system param kind read by the compile step; typed from `native.d.ts`; load error on an unknown name
- [ ] 5.3 Load-time aliasing check between a function's component reads and the system's `write` set
- [ ] 5.4 Test 1,000 rays in one call record one JS-to-Rust crossing and return 1,000 results in the same run; output views detach after the run

## 6. Script materials

- [ ] 6.1 Spike: one `ScriptMaterial` asset type through `ErasedRenderAsset` with two kinds of different bind group layouts behind its single `TypeId` allocator, non-bindless; fallback to a fixed pool of Rust types if the allocator refuses
- [ ] 6.2 WGSL uniform layout in Rust for material params; generate the params struct and bindings and prepend them to the author's WGSL; unit-test sizes and offsets (`vec3f` at 16, arrays strided to 16)
- [ ] 6.3 `bevy-ts/material`'s `material()` declaration in the generation manifest; shader replacement in place on source change; param-schema changes re-encode instances by name; WGSL errors mapped to the material and source line, old shader kept
- [ ] 6.4 Decide whether instances are keyed assets or components on the entity, then add TSX `<material kind={...} />`
- [ ] 6.5 Demo: a `game::Glow` material on the cube example, edited live, in the prebuilt engine with no native crate

## 7. Docs

- [ ] 7.1 Write `docs/native.md`: the "what stays Rust" table, the two component tiers, slots, native functions, the shared cache for teams, and the TypeGPU verdict
- [ ] 7.2 README: one line on the native crate and project engines
