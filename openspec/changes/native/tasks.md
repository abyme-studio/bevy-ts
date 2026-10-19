## 1. Bridge prerequisites (N1 to N5)

- [x] 1.1 Fill bridge's native layout registry at startup from reflection (decision 5: offsets from a value's field references, bounds and leaf-size checks, enum variant bytes read from the build, unreadable leaves as holes; test `Res<Time>` reads `delta_secs`), make it public, and route every Rust-component check in TS systems through it (N1); `Transform` takes the same path
- [x] 1.2 Turn `bevy-ts-cli`'s `main` into `bevy_ts_cli::run(extra: impl Plugin)` and make the prebuilt binary a one-line caller (N4)
- [x] 1.3 Make the exclusive stage system constructible for any schedule and ordering (N3); bridge's four stages use it unchanged
- [x] 1.4 Add the call-host-function seam operation to the backend trait, the fake backend and `bevy-ts-v8` (deno_core op over views of JS memory) (N2)
- [x] 1.5 Add engine identity, exposed sets and native functions to `bevy-ts/hello` and `bevy-ts/schema` in `protocol/bevy-ts.openrpc.json`, minor bump, regenerate both sides (N5)

## 2. Native crate and project engine

- [x] 2.1 Create `bevy-ts-native` with the `App` extension trait (`ts_param`, `ts_set`, `ts_module`) and a `bevy` re-export; publish plan at the npm version
- [x] 2.2 Read `native.crate`/`native.plugin` from `bevy-ts.json`; generate `.bevy-ts/engine/` (Cargo.toml depending on the native crate by path, `main` calling `bevy_ts_cli::run` with the named plugin); print the build plan (hash, cache miss, what compiles) before building
- [x] 2.3 Check `Cargo.lock` for exact `bevy-ts-native` and `bevy` versions before building; error naming crate, expected and found
- [x] 2.4 Compute the engine hash (bevy-ts version, target, profile, features, `Cargo.lock`, workspace manifests, packaged native files); unit-test that TS and asset edits leave it unchanged
- [x] 2.5 Local cache `.bevy-ts/engines/<hash>/`; `cargo build` on miss with progress passed through; clear error when `cargo` is missing
- [x] 2.6 Shared cache: a directory (`<hash>/<target>/`), SHA-256 verification before execution; docs say it proves integrity, not authenticity. HTTP and `engine publish` are later
- [x] 2.7 `bevy-ts dev`: watch hashed Rust inputs, rebuild in the background, swap engines on success on the same port, load the TS generation as a restart, keep the old engine on failure
- [x] 2.8 Example `examples/thrusters/`: a native crate adding `bevy_rapier3d` 0.37 and one Rust system; verify a TS-only edit starts no `cargo` and a Rust edit restarts

## 3. Rust components in TS

- [x] 3.1 `bevy-ts-native-derive`: `#[derive(NativeLayout)]` for components without `Reflect`, mapping field types to bridge field types, `offset_of!` tables (public fields, no `repr(C)`); compile-fail tests for `String`, `Handle`, a private field
- [ ] 3.2 Tier every reflected component at startup by decision 5; TS systems accept layout tier as query data and reflect tier only in `With`/`Without`, with the ts-components error otherwise; accessor-only `bool`/enum/`Entity` fields, immutable components refused
- [ ] 3.3 Test that a TS write to a layout-tier Rust component is seen by a Rust system the same frame, with `Changed` exactly on written rows
- [x] 3.4 `bevy-ts types`: build the app, stop after `Plugin::build` before `finish`, walk the registry, emit `native.d.ts` deterministically; verify no GPU device is created with `bevy_rapier3d` and `bevy_egui`, and report plugins that need one
- [ ] 3.5 Ship pre-generated `bevy-ts/bevy` types from the prebuilt engine; CI regenerate-and-diff; `bevy-ts dev` regenerates after each project-engine build
- [ ] 3.6 TSX accepts reflect-tier components as elements (patch through bridge's reflected access); test `<Weapon name="laser" />`

- [x] 3.7 Prove the layout rule on `bevy_rapier3d` 0.37 in the engine: `Velocity`, `ExternalImpulse`, `RigidBody` (enum bytes) and the private `LockedAxes` are layout tier; `Collider` (no `Reflect`) is not; a TS write to `Velocity` moves a body

## 3b. Plugin adoption

- [x] 3b.1 Read `native.ports` and `native.crates` from `bevy-ts.json`; generate the engine's dependencies and `add_plugins` lines; a project with plugins and no native crate builds and runs
- [x] 3b.2 Port format: crate `bevy-ts-<name>` whose Bevy plugin adds the third-party plugin, exports params, exposes sets and registers the `bevy-ts/<name>` module (types, JS glue, TSX constructors, flattened messages)
- [ ] 3b.3 Ship the `rapier3d` port: `RapierPhysicsPlugin::<NoUserData>::default().in_fixed_schedule()`, `Collider` constructors, `CollisionEvent` flattened to `{ kind, a, b, sensor }`, `ReadRapierContext` exported with `castRay`/`castRays`, sets `rapier:sync-backend`/`rapier:step`/`rapier:writeback`; example `examples/rapier/` with no Rust in the project
- [x] 3b.4 Plan the `leafwing-input-manager` port with a dynamic `TsAction(u16)`; record avian and hanabi as waiting on Bevy 0.20

## 4. Ordering against Rust sets

- [x] 4.1 `app.ts_set(name, schedule, set)`, name uniqueness at startup, bevy-ts's own anchors (`bevy:transform-propagate`)
- [x] 4.2 Make before and after slots for a set the first time a generation names it (one schedule edit at that generation's apply), with `BevyTsSlot::before/after` sets for Rust ordering; no slots for unnamed sets
- [x] 4.3 Place TS systems by `.before/.after(systemSet(name))` into slots; validate one set per system, matching schedule, and TS-to-TS ordering only within a slot; reload moves systems between existing slots without touching schedules
- [ ] 4.4 Measure the cost of one slot's barrier (Rust systems on both sides, slot present vs absent); the barrier itself is established from `multi_threaded.rs:533`; record the numbers in a research note
- [x] 4.5 Emit `NativeSet` as a string union in `native.d.ts`

## 5. Exported system params

- [x] 5.1 `#[bevy_ts::export(ts, module)]` on an `impl` of a `ReadOnlySystemParam`: per method a host function (V8 fast op), argument and result codecs, the TS declaration and JS glue; compile errors for unsupported argument types and non-read-only params
- [x] 5.2 `app.ts_param::<P>()`: a `SystemState<P>` per param, fetched read-only during the JS stage; validation errors and panics (`catch_unwind`) thrown as JS exceptions naming param and method; no re-entry into JS
- [x] 5.3 The compile step reads exported params as system param kinds (from the module that declares them); load error on an unknown name
- [x] 5.4 Test 1,000 rays in one `castRays` call record one JS-to-Rust crossing and return 1,000 results in the same run; a single `castRay` is one fast op; output views detach after the run

## 7. Docs

- [ ] 7.1 Write `docs/native.md`: the "what stays Rust" table, the two component tiers and holes, plugin adoption and ports, exported params, slots, and the shared cache for teams
- [x] 7.2 README: one line on the native crate and project engines
