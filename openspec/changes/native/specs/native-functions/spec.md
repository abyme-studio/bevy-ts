## ADDED Requirements

### Requirement: Rust registers batched native functions
The native crate SHALL be able to register a native function with `app.ts_fn::<P>("physics:raycast", f)`, where `P` is a read-only Bevy system parameter (`ReadOnlySystemParam`, for example `SpatialQuery` or `Res<NavMesh>`) and `f(P, input: &[u8], output: &mut NativeOut)` reads a request batch and appends results. Its input and output shapes SHALL be declared as bridge field-type records (`ts_fn` takes them as `NativeLayout` types), so TS sees typed arrays of records, not bytes. Each function SHALL appear in `bevy-ts/schema` and in `native.d.ts` as a system param kind, `NativeFn<"physics:raycast">`; a port MAY group several functions under one param kind of its own (`ReadRapierContext`).

#### Scenario: A typed raycast batch
- **WHEN** the native crate registers `physics:raycast` with input `Ray { origin: Vec3, dir: Vec3, max: f32 }` and output `Hit { entity: Entity, toi: f32, normal: Vec3 }`
- **THEN** in TS, a system taking `raycast: NativeFn<"physics:raycast">` can call `raycast(rays)`, which type-checks with `rays` a batch of `Ray` and returns a batch of `Hit`

### Requirement: One crossing per batch, synchronous inside a run
A TS system SHALL call a native function synchronously during its run and get results in the same run. Each call SHALL be one call from JS into Rust whatever the batch length; the input batch SHALL be read in place from JS memory and the output SHALL be written into a Rust-owned buffer exposed to JS without a copy, valid until the run ends. This is a new seam operation (call host function) that every backend SHALL implement.

#### Scenario: 1,000 rays
- **WHEN** a system casts 1,000 rays in one `raycast(rays)` call
- **THEN** the backend records one call from JS into Rust for it, and the system reads 1,000 results before returning

### Requirement: Native functions cannot alias the running system's columns
A native function's `P` SHALL be validated at registration to be read-only. At load, bevy-ts SHALL reject a TS system that takes a native function whose `P` reads a component the system writes (`Mut<C>`), naming both. A native function SHALL NOT be able to make structural changes; it MAY return entity ids, and the TS system uses its own command buffer for changes.

#### Scenario: Aliasing rejected
- **WHEN** a native function's parameter reads `Transform` and a TS system that takes `NativeFn<"physics:raycast">` also queries `Mut<Transform>`
- **THEN** the generation fails with an error naming the system, the function and `Transform`

### Requirement: Systems declare the native functions they call
A TS system SHALL reach a native function only through a param of kind `NativeFn<name>` (or a port's param kind), read from its signature by the compile step (script-runtime). A name that is not registered SHALL be a type error and a load error naming it.

#### Scenario: Undeclared use
- **WHEN** a system takes `NativeFn<"physics:raycst">`
- **THEN** `tsc` reports the name missing from the `NativeFn` union, and loading fails naming `physics:raycst`
