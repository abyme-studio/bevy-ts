## Why

Stage 1 (`reflect-enums-color`) makes enums, `Option`s and `Color`s writable from TSX. Most of
what Bevy reflection can express is still out of reach: TS systems cannot read or write a
reflect-tier component or a layout-tier hole at all, collections and maps have no stated shape,
`Duration` and big integers are opaque, and the protocol serves reflected values in stock
`bevy_remote`'s shape, which differs from what TSX takes. Three native recipes (`camera3d`,
`directionalLight`, `cuboid`) exist only because scripts cannot spawn those components. The
operator asked for all of it: every shape reflection can express is readable and writable from
TSX and from scripts, and no gap can come back.

## What Changes

Three stages, each landable on its own, in this order. Stage 3 waits on the operator's go.

1. **One shape for every reflect kind (TSX and protocol).** `bevy_ts::reflect_value` (stage 1's
   writer) gains `read`, the inverse of `write`: `write(read(v))` gives back `v`. Together they
   own the JSON shape of every kind: structs, tuple structs and newtypes, tuples, arrays, `Vec`,
   `VecDeque`, `HashMap` and `HashSet` (and every other list, map and set), enums of every variant
   kind, `Option`, `Color` in every space, math types (`Vec*`, `IVec*`, `UVec*`, `DVec*`,
   `BVec*`, `Quat`, `Mat2`/`Mat3`/`Mat4`, `Affine2`/`Affine3A`, `Isometry2d`/`Isometry3d`, `Rot2`,
   `Dir*`), `Duration` (seconds), 64- and 128-bit integers, `Handle<T>` (asset path), `Entity`,
   opaque values with reflected serde, and dynamic values. TSX takes every one of them. Generated
   `.d.ts` types each kind. The protocol's reflected reads and writes switch to the same reader
   and writer, so an inspector sees exactly what TSX takes. **BREAKING (wire):** `Duration`
   fields read as seconds, handles as their path, big integers as strings past 2^53. Bump the
   protocol to the next free minor.
2. **Scripts write every reflect kind; native recipes go.** Reflect-tier classes gain `.of(json)`,
   so `commands.spawn(Camera3d.of(), Projection.of({ Orthographic: {} }))` and `insert` work, and
   a layout-tier `.of` takes its writable holes (`DirectionalLight.of({ illuminance: 8000, color:
   hex("#ffd7a8") })`). `Transform.lookingAt(eye, target, up?)` is the one sugar. The built-in
   recipes `camera3d`, `directionalLight` and `cuboid` are removed; `spawnNative` stays for
   recipes a plugin registers. **BREAKING:** scripts calling the three recipes migrate (list in
   design).
3. **Scripts read every reflect kind (needs the operator's go).** A reflect-tier component is
   query data and `Res` data: `Query<[Projection]>`, `Query<[Mut<Camera3d>]>`. A layout-tier
   hole is a field accessor on the row (`light.color`), outside `q.tables`. A row's reflected
   value is cached across runs by its changed tick, so an unchanged row costs no host call; a
   changed row is read through one host call and a binary codec compiled once per type from its
   `TypeInfo`. A write is applied at once and marks the component changed. `NodeRef.get` reads
   the same way inside TSX bindings.
4. **Coverage matrix and a hole check.** `docs/reflection.md` lists every reflect kind: read,
   write, TS type, and the test that proves it. `crates/bevy-ts-cli/tests/it/reflect_coverage.rs`
   walks every registered reflected component and resource in the prebuilt engine (plus the
   Rapier port) and fails on any field it cannot read, write back, round-trip and type, unless
   the type is on the allowlist in `crates/bevy-ts/src/reflect_value.rs` with a reason. It lands
   in stage 1 of this change and grows with each stage.

## Capabilities

### New Capabilities

None.

### Modified Capabilities

- `native-components`: one JSON shape for every reflect kind; holes and reflect-tier types
  readable and writable from TS systems (stage 3) and insertable from commands (stage 2); the
  generated types cover every kind; a CI check proves no hole.
- `ts-systems`: reflect-tier query data and resources; commands insert reflect values;
  `Transform.lookingAt`; the built-in native recipes are removed.
- `remote-protocol`: reflected components are read and written through bevy-ts's reader and
  writer, in the shape TSX takes.

## Impact

- Code: `crates/bevy-ts/src/reflect_value.rs` (read, every kind, allowlist), `layout_reflect.rs`
  (hole accessors), `commands.rs` (reflect insert op), `recipes.rs` (deleted), `systems.rs` and
  `components.rs` (reflect-tier query data, `Res`), `host.rs` (read and write host calls);
  `crates/bevy-ts-v8/js/core.js` and `packages/bevy-ts/runtime/core.js` (`.of` for reflect tier,
  lazy reflected rows, `lookingAt`); `crates/bevy-ts-tsx/src/reflected.rs`, `host.rs`
  (`NodeRef.get`); `crates/bevy-ts-remote/src/protocol.rs`; `crates/bevy-ts-cli/src/types.rs`;
  `packages/bevy-ts/index.d.ts`, `bevy.d.ts` regenerated.
- Tests: new `crates/bevy-ts-cli/tests/it/reflect_coverage.rs` (the hole check) and
  `reflect_scripts.rs`; existing tests that call `spawnNative("camera3d")` migrate.
- Examples: `examples/cube/src/game.ts`, `examples/arena/src/game.tsx` drop `spawnNative`.
- Docs: new `docs/reflection.md` (the matrix and the shape of each kind); `docs/native.md`
  (holes and reflect tier from TS systems); `docs/materials.md` (its camera example).
- Protocol: stage 1 of this change changes reflected values on the wire; bump to the next free
  minor at landing (stage 1 of `reflect-enums-color` may already have taken `1.25.0`).
- Shared seams, each with one owner (design, "Seams"): `reflected.rs::set()` and the `value`
  name of newtypes (bevy-ui-coverage), `NodeRef.get` (this change for reflected reads,
  bevy-ui-coverage for `has`), `<mesh>` props (mesh-primitives), `reflect_value` and `color`
  (this change after `reflect-enums-color` lands), `core.js` `encodeWithLayout`
  (ambient-light-script; stage 2 sequences after it).
- Operator's go needed: stage 3 only. It adds a per-frame path: a TS system that touches a
  changed reflected row pays a host call and a binary decode (about 0.1 µs); an unchanged row
  hits a tick cache (about 10 ns). No new dependency,
  service or infrastructure. Removing the recipes is under the operator's delegation for bevy
  work.
