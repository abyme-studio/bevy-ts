## Context

After `reflect-enums-color` lands, `bevy_ts::reflect_value::write(registry, server, slot, base,
json)` writes a JSON value into any reflected slot for enums, `Option`, `Color`, `Handle<T>` and
struct merges, with `TypedReflectDeserializer` as the fallback. `bevy_ts::color::parse` owns
colours. TSX is its only caller besides the `camera3d` recipe.

What is still missing:

- No reader. `bevy-ts/get_components` delegates reflected components to stock `bevy_remote`
  serialization: `Duration` as `{ secs, nanos }`, `Handle<T>` as whatever its reflection gives,
  `u64` as a JSON number that loses precision past 2^53. TSX takes a different shape.
- TS systems see layout-tier columns only. A reflect-tier class is `rustComponent(name)`: usable
  in `With`/`Without` and TSX, nothing else (`index.d.ts`'s `NotReflect` rejects it as query
  data). A layout-tier hole is invisible to systems.
- `Commands.spawn`/`insert` encode components in their native layout (`encodeComponentsOp`).
  A reflect-tier value cannot cross.
- `spawnNative` has three built-in recipes in `crates/bevy-ts/src/recipes.rs`. Only `camera3d`
  is called outside its own tests: `examples/cube`, `examples/arena` and five CLI tests. Plugins
  register their own recipes through `plugin_api::add_ts_native_recipe`.
- `layout_reflect.rs` makes holes of: enums with data, non-packed arrays, opaque types, anything
  not plain data. `plain()` knows `f32`, `f64`, 8- to 32-bit integers, `bool`, `Entity`, `Vec2`,
  `Vec3`, `Vec3A`, `Vec4`, `Quat`.

## Goals / Non-Goals

**Goals:**
- One JSON shape per reflect kind, owned by `reflect_value`, used by TSX, scripts and the
  protocol.
- Every kind readable and writable from TSX and TS systems.
- A CI check that fails on any registered reflected type it cannot cover.
- No built-in native recipes.

**Non-Goals:**
- Holes as zero-copy columns, enums included. They stay reflection-backed; `q.tables` never
  shows them. Rust enum layout and niches are unspecified, so a tag written from JS could be
  undefined behaviour (research-script-perf.md, option E).
- New TS-declared component field kinds (TS components keep their plain-data rule).
- Reactive bindings on reflected props (stage 1's rule stands).

## Decisions

1. **`reflect_value::read(registry, value) -> Result<Value, String>` is `write`'s inverse.** The
   law: for any value `v` of a covered type, `write` over a default with `read(v)` gives a value
   `reflect_partial_eq` to `v`. Shapes, by reflect kind:

   | Kind | JSON | TS type |
   |---|---|---|
   | Struct | object of fields; `write` merges over the default (stage 1) | interface |
   | Tuple struct, 1 field (newtype) | the inner value; as a component, `{ value }` (bevy-ui-coverage) | inner type |
   | Tuple struct, n fields; tuple | array | `[A, B, ...]` |
   | Array `[T; N]` | array of N | `T[]` (a tuple type for N ≤ 4) |
   | List (`Vec`, `VecDeque`, `SmallVec`) | array; `write` replaces the whole list | `T[]` |
   | Set (`HashSet`, `BTreeSet`) | array; read sorted by each element's JSON text, so it is deterministic | `T[]` |
   | Map with string keys | object; `write` replaces the whole map | `Record<string, V>` |
   | Map with other keys | array of `[k, v]`, sorted like sets | `[K, V][]` |
   | Enum | stage 1: `"Unit"`, `{ Variant: data }` | union |
   | `Option<T>` | `null` or `T` | `T \| null` |
   | `Color` | read: Bevy's shape (`{ Srgba: {...} }`); write: every stage 1 form | `ColorValue` |
   | `Vec*`, `IVec*`, `UVec*`, `DVec*`, `BVec*`, `Quat` | array of lanes | tuple types |
   | `Mat2`/`Mat3`/`Mat3A`/`Mat4`, `Affine2`/`Affine3A` | flat array, column-major (glam's serde) | `number[]` tuple types |
   | `Isometry*`, `Rot2`, `Dir2`/`Dir3` | their reflected structs / inner vectors | interface / tuple |
   | `Duration` | seconds as a number; `write` also takes `{ secs, nanos }` | `number` |
   | `Range<T>` (Bevy 0.20 registers it opaque, `Clone` only, no serde) | `{ start, end }`, each in `T`'s shape; written whole (`ColorGrading.global.midtones_range`) | `{ start: T; end: T }` |
   | `i64`, `u64`, `i128`, `u128`, `isize`, `usize` | number when within ±2^53, else a decimal string; `write` takes both | `number \| string` |
   | `Handle<T>` | its asset path, or `null` for a handle with none (a generated mesh); writing `null` keeps the default handle. A `HandleSlots` hook, when installed, goes first (below) | `string \| AssetRef<T> \| AssetValue<T> \| null` |
   | `Entity` | `Entity::to_bits` as a number, as TS systems use | `Entity` |
   | Opaque with `ReflectSerialize`/`ReflectDeserialize` (`String`, `char`, paths) | its serde JSON | from `registry.schema` |
   | Dynamic (`DynamicStruct` and kin, `Box<dyn PartialReflect>` fields) | read by its own reflect kind; written only when it represents a registered type | as represented |

   The `Handle` arm has one hook for both directions: `reflect_value::Cx` carries an optional
   `&dyn HandleSlots` with `read(untyped) -> Option<Value>`, tried before the path, and
   `write(handle_type, json) -> Result<UntypedHandle>`, called for any JSON that is not a string
   or `null`. reflect-assets installs it and owns what it accepts (`AssetRef<T>`, `AssetValue<T>`,
   `$load`). With no hook, a handle reads as its path or `null`, and a write takes a path or
   `null`; any other JSON fails naming the slot's handle type.

   *Alternative:* keep stock `bevy_remote` serialization for reads. Rejected: two shapes for one
   value, and a protocol client could not hand a read value back to TSX.

2. **The allowlist is the only home of a gap.** A type the reader or writer cannot cover (an
   opaque type without serde, `TypeId`, a `dyn` field with no represented type, a type without
   `ReflectDefault` or `ReflectFromWorld` that cannot be inserted) is listed in
   `reflect_value::UNCOVERED` with a reason, and its slot reads as absent and fails to write with
   that reason. Computed components (`ComputedNode`, `TextLayoutInfo`, `GlobalTransform`) read
   like any other and refuse writes from TSX and scripts with "computed by Bevy".

3. **The hole check.** `crates/bevy-ts-cli/tests/reflect_coverage.rs` builds the prebuilt engine
   headless (as `bevy-ts types` does) with the Rapier port added, and for every registration with
   `ReflectComponent` or `ReflectResource` walks every field type recursively. For each type it
   takes a value (`ReflectDefault`, `ReflectFromWorld`, else a type-shaped sample built from type
   info), then checks `read` succeeds, `write` of that JSON over a default round-trips by
   `reflect_partial_eq`, and the generated TS type is not `unknown`. Any failure not in
   `UNCOVERED` fails the test, naming the type path, the field path and the kind. A second check
   fails when `UNCOVERED` names a type the walk never meets, so the list cannot rot.

4. **Protocol uses the reader and writer.** `bevy-ts/get_components`, `+watch` and
   `mutate_components` serve reflected components through `read` and `write` instead of stock
   serialization. `mutate_components`' `path` stays a reflection path; the value at it is written
   by `write` over the slot's default. `bevy-ts/schema` keeps stock JSON Schema shapes and adds
   the TS type string to `x-bevy-ts-editor` per field. Clients get the protocol minor bump.

5. **Scripts write reflect values through commands (stage 2).** The generated class of a
   reflect-tier type stops being `rustComponent(name)` and becomes `reflectClass(name)`: still
   good in filters, plus `.of(json)` and use as a bare class (its default). A new command op,
   `OP_INSERT_REFLECT { entity, type index, json }`, carries each reflect value; `spawn` and
   `insert` emit it beside `OP_SPAWN`/`OP_INSERT` for layout values. Rust applies it with
   `write` over `ReflectDefault` (or `ReflectFromWorld`) and inserts through `ReflectComponent`.
   A layout-tier `.of({...})` that names writable holes encodes the layout part as bytes and the
   holes as a reflect-insert patch applied right after, in the same command. The bytes come from
   `encodeWithLayout`, which ambient-light-script changes to start from the layout's default
   bytes; stage 2 builds on that and starts after it lands.
   *Alternative:* a new `commands.reflect(e, C, json)` call. Rejected: Bevy's own `insert` is
   the call authors know.

6. **Recipes removed (stage 2).** `recipes.rs` and its three registrations go. `spawnNative`
   and `plugin_api::add_ts_native_recipe` stay for plugins. Migration:

   | Was | Now |
   |---|---|
   | `spawnNative("camera3d", { position, lookAt, projection })` | `commands.spawn(Camera3d, Transform.lookingAt(position, lookAt), Projection.of(projection))` |
   | `spawnNative("directionalLight", { position, lookAt, illuminance, shadows })` | `commands.spawn(DirectionalLight.of({ illuminance, shadows_enabled: shadows }), Transform.lookingAt(position, lookAt))` |
   | `spawnNative("cuboid", { size, color, position })` | an element: `commands.spawn(Transform.of({ translation: position }), <entity><mesh shape="cuboid" size={size} /><material color={color} /></entity>)` or a scene |

   Until ecs-queries lands, `spawn(..., <el/>)` mounts the element as a child of the spawned
   entity; after it, the element's components go on the returned root.
   Callers that migrate: `examples/cube/src/game.ts`, `examples/arena/src/game.tsx`,
   `crates/bevy-ts-cli/tests/material_errors.rs`, `play_stop.rs`, `tsx.rs`, `viewport_edit.rs`,
   `viewport_walk.rs`, `viewport_level.rs`, `crates/bevy-ts-v8/js/test/jsx-runtime.test.js` where
   they call it, `docs/materials.md`. A project outside the repo calling a removed recipe fails
   at the spawn with an error naming the replacement.
   `Transform.lookingAt(eye, target, up = [0, 1, 0])` is plain JS math returning a `Transform`
   value, matching Bevy's `Transform::looking_at`. It is the only sugar; it has no Rust side.

7. **Scripts read reflect values (stage 3, operator's go).** `NotReflect` goes: a reflect-tier
   class is query data (`Query<[Projection]>`, `Mut<Projection>`) and `Res`/`ResMut` data.
   Its table has no value columns; it has the component's changed-tick column, a zero-copy view
   as layout-tier writes already use. Reads go through two layers (research-script-perf.md):
   - **Tick cache.** A per-type cache, entity → (changed tick, plain object), outlives the run.
     A row whose tick matches its entry returns the cached object with no host call (about
     10 ns). The cache is dropped on reload and an entry on despawn or removal.
   - **Binary codec on a miss.** At load, `reflect_value` compiles each reflected type's
     `TypeInfo` once into a list of ops (kind, size; flecs' `ecs_meta_op_t` pattern), and the JS
     runtime generates a decoder and encoder from the same ops. A miss is one host call that
     walks the component by reflection along the ops into a byte buffer, decoded in JS (about
     0.1 µs, against about 1 µs for JSON). Lists, maps, sets and strings are length-prefixed,
     `Handle` is its path string, and an opaque serde type is an embedded JSON op. The decoder's
     output is exactly `JSON.parse` of `read`'s JSON; the hole check (decision 3) asserts this
     for every type it covers. A type the ops cannot express falls back to `read`'s JSON.
   A write through `Mut` (`cam.value = { Orthographic: {} }`, or a field assignment on the cached
   object followed by the row's end) encodes with the same ops, calls a host function that writes
   at once with `write`'s semantics and sets the change tick, then refreshes the cache entry with
   the new tick. A layout-tier hole is a getter and setter on the row accessor doing the same for
   one field; `q.tables` and column arrays never include holes. In-place reflection writes do not
   move archetypes, so open column views stay valid. `NodeRef.get` reads reflect-tier components
   and holes through the same cache and codec and subscribes like any other component.
   Stages 1 and 2 keep JSON: TSX, the protocol and `OP_INSERT_REFLECT` are not per-row per-frame.
   *Alternatives:* a host call and JSON per touched row per run (about 1 µs a row, every run);
   packing every matched row before the run (pays for rows the system never touches); zero-copy
   enum columns (see Non-Goals); batching `Mut` writes to the end of the run (saves about 35 ns a
   write, hides writes from Rust mid-run; only if the bench shows write-heavy systems).

## Seams

| Seam | Owner | Other party |
|---|---|---|
| `bevy_ts::reflect_value`, `bevy_ts::color` | this change (after `reflect-enums-color` lands; build@reflect-enums-color owns them until then) | everyone calls them |
| `reflected.rs::set()` path mapping; `value` as a newtype's TS field (`layout_reflect.rs`) | lead@bevy-ui-coverage | this change follows `value` in `read` and the generated types |
| `NodeRef.get` | this change for reflected reads (stage 3) | lead@bevy-ui-coverage owns `NodeRef.has` |
| `ui.rs` colour call sites | `reflect-enums-color` (parser) | lead@bevy-ui-coverage adds call sites |
| `<mesh>` props, `MeshProps` in `intrinsics.rs`, `resolve.rs`/`reconcile.rs` matches on it | lead@mesh-primitives | this change's `cuboid` migration uses `<mesh shape="cuboid">` and touches no `MeshProps` code |
| `reflect_value::HandleSlots` hook | this change (the hook and its no-hook behaviour) | lead@reflect-assets implements and installs it |
| `types.rs` | this change for reflect kinds, the `Handle<T>` arm included | lead@bevy-ui-coverage for UI prop types |
| `core.js` `encodeWithLayout` (layout values start from each layout's default bytes sent in the resolution) | lead@ambient-light-script | stage 2's layout-tier `.of` with holes and `OP_INSERT_REFLECT` build on it and start after it lands |

Whichever lands second rebases and keeps both sides.

## Risks / Trade-offs

- [Stage 3 is a per-frame path: a system touching a changed reflected row pays a host call and
  a binary decode] → Unchanged rows hit the tick cache with no call; a miss costs about 0.1 µs.
  A bench (task 3.7) states the real numbers in `docs/reflection.md`. Ask-first: stage 3 does not
  start without the operator's go.
- [The codec and `read` drift apart] → The hole check compares the decoder's output with
  `JSON.parse` of `read` for every covered type.
- [Protocol shape change breaks a client parsing `{ secs, nanos }` or stock handle shapes] →
  Minor bump; the handshake carries the version; `docs/reflection.md` lists the changed kinds.
- [Big integers as strings surprise a script] → Typed `number | string`; only past 2^53.
- [The hole check fails on an upstream Bevy bump] → That is the point; a new gap is fixed or
  allowlisted with a reason in the bump's PR.
- [Removing recipes breaks out-of-repo scripts] → The spawn error names the replacement; the
  changelog lists the migration.

## Migration Plan

Stage 1: protocol minor bump; no script change. Stage 2: the recipe callers above migrate in the
same commit as the removal. Stage 3: additive. Rollback is a revert per stage.

## Open Questions

None. Stage 3 waits on the operator's go, which decides whether it starts, not how.
