# Reflected values

Every reflected value bevy-ts takes or gives has one JSON shape per kind, owned by
`bevy_ts::reflect_value`: `read` gives it, `write` takes it, and writing what `read` gave over a
default gives back an equal value. TSX props, the protocol (`bevy-ts/get_components`,
`+watch`, `mutate_components`) and script commands all go through these two functions, so an
inspector can hand a value it read straight back to TSX or to the protocol.

A written value is merged over a base: the slot's value in a fresh default of its component.
Keys left out take the base's value. Lists, sets and maps are written whole, never merged.

## The matrix

| Kind | Read | Write also takes | TS type | Test |
|---|---|---|---|---|
| Struct | object of its fields | a partial object, merged | interface, fields optional | `every_kind_round_trips` |
| Newtype (one-field tuple struct) | its inner value; a whole newtype component is `{ value }` | `{ value }` for a component | the inner type | `newtype_components_are_value` |
| Tuple struct, tuple | array | | `readonly [A, B]` | `every_kind_round_trips` |
| Array `[T; N]` | array of N | | a tuple type for N ≤ 4, else `readonly T[]` | `every_kind_round_trips` |
| List (`Vec`, `VecDeque`) | array | written whole | `readonly T[]` | `a_list_is_replaced_not_merged` |
| Set (`HashSet`; `BTreeSet` is opaque in Bevy 0.20, so it takes its serde) | array, sorted by each element's JSON text | written whole | `readonly T[]` | `every_kind_round_trips` |
| Map, string keys | object | written whole | `Record<string, V>` | `every_kind_round_trips` |
| Map, other keys | `[key, value]` pairs, sorted like sets | written whole | `readonly (readonly [K, V])[]` | `a_map_with_entity_keys` |
| Enum | `"Unit"`, `{ Newtype: inner }`, `{ Tuple: [a, b] }`, `{ Struct: {...} }` | a new variant starts from its default | a union | `shapes_of_each_kind` |
| `Option<T>` | `null` or `T` | | `T \| null` | `options_take_null_and_a_value` |
| `Color` | Bevy's shape, `{ Srgba: { red, green, blue, alpha } }` and every other space | `"#rrggbb[aa]"`, `[r, g, b, a?]`, any space with `alpha` left out | `ColorValue` | `colours_in_any_form` |
| glam vectors and `Quat` (`Vec*`, `IVec*`, `UVec*`, `DVec*`, `BVec*`, the 8-, 16- and 64-bit ones) | array of lanes | an object of lanes, merged | a lane tuple | `every_kind_round_trips` |
| glam matrices and affines (`Mat2`, `Mat3`, `Mat3A`, `Mat4`, `Affine2`, `Affine3A`) | flat array, column-major | | a number tuple | `every_kind_round_trips` |
| `Dir2`, `Dir3`, `Rot2`, `Isometry2d`, `Isometry3d` | inner vector, or their reflected struct | | tuple or interface | `every_kind_round_trips` |
| `Duration` | seconds | `{ secs, nanos }` | `number` | `durations_take_seconds_or_secs_and_nanos` |
| `f32`, `f64` | number; `"Infinity"`, `"-Infinity"`, `"NaN"` when not finite | | `number` | `shapes_of_each_kind` |
| `i64`, `u64`, `i128`, `u128`, `isize`, `usize` | number within ±(2^53 − 1), else a decimal string | either | `number \| string` | `big_integers_from_strings_and_numbers` |
| `Handle<T>` | its asset path, or `null` for a handle with none | `null` keeps the default handle | `string \| null` | `reflect_values.rs` |
| `Range<T>`, `RangeInclusive<T>` of a number type | `{ start, end }` | | `{ start: number; end: number }` | `colour_grading_round_trips_with_its_midtones_range` |
| `Exposure` (opaque in Bevy 0.20) | `{ ev100 }` | | `{ ev100?: number }` | `exposure_is_ev100` |
| `Entity` | `Entity::to_bits` as a number | | `Entity` | `a_map_with_entity_keys` |
| `Name` | its string | | `string` | `reflect_coverage.rs` |
| Opaque with reflected serde (`String`, `char`, paths, `NonZero*`) | its serde JSON | | `string`, or `unknown` with no shape | `every_kind_round_trips` |
| Dynamic values | by the kind they represent | only when they represent a registered type | as represented | `reflect_coverage.rs` |

The tests named are in `crates/bevy-ts/src/reflect_value.rs` unless a file is given;
`reflect_values.rs` is `crates/bevy-ts-cli/tests/it/reflect_values.rs`.

A `HandleSlots` hook, when one is installed (reflect-assets), goes first for `Handle<T>`: it reads a
handle it knows as its own JSON, and is given any handle JSON that is neither a string nor `null`.

Components Bevy computes (`GlobalTransform`, `ComputedNode`, `TextLayoutInfo`) read like any
other and refuse writes, naming themselves.

## From scripts

`commands.spawn` and `entity(e).insert` take reflected values too. A reflect-tier class
(`Projection`, `Camera3d`) is spawned bare for its default, or as `C.of(value)` with its JSON
written over the default. A layout-tier `.of` may also set the type's holes: the fields its
layout has go as bytes, the rest are written through reflection right after, in the same
command.

```ts
commands.spawn(Camera3d, Transform.lookingAt([10, 10, 10], [0, 0, 0]), Projection.of({ Orthographic: { scaling_mode: { FixedVertical: { viewport_height: 12 } } } }));
commands.spawn(DirectionalLight.of({ illuminance: 8000, color: hex("#ffd7a8") }), Transform.lookingAt([3, 5, 2], [0, 0, 0]));
```

A layout class whose holes may own heap data (`Skybox`, for its image handle) is never inserted
from bytes: `Skybox.of({ image: "sky.ktx2", brightness: 1000 })` is written whole through
reflection, as its TSX element is, and a string at any `Handle<T>` loads that path.

A value that fails to write fails its whole command, logged with the type and field: a spawn
with a bad value spawns nothing, an insert inserts nothing. A type whose only default is
`FromWorld` is checked the same way, over a value built from the world, so its `FromWorld` runs
even for a command that is then skipped. The system's other commands still
apply. `Transform.lookingAt(eye, target, up = [0, 1, 0])` is
Bevy's `Transform::looking_at`, computed in JS.

bevy-ts has no built-in spawn recipes. `spawnNative` stays for recipes a plugin registers
(`plugin_api::add_ts_native_recipe`); the three it once had are components now:

| Was | Now |
|---|---|
| `spawnNative("camera3d", { position, lookAt, projection })` | `commands.spawn(Camera3d, Transform.lookingAt(position, lookAt), Projection.of(projection))` |
| `spawnNative("directionalLight", { position, lookAt, illuminance, shadows })` | `commands.spawn(DirectionalLight.of({ illuminance, shadow_maps_enabled: shadows }), Transform.lookingAt(position, lookAt))` |
| `spawnNative("cuboid", { size, color, position })` | `commands.spawn(Transform.of({ translation: position }), <entity><mesh shape="cuboid" size={size} /><material color={color} /></entity>)` |

Calling a removed recipe logs its replacement.

## No gap goes unchecked

`crates/bevy-ts-cli/tests/it/reflect_coverage.rs` walks every component, resource and asset in the
prebuilt engine's registry, and in a project engine with the Rapier port (`bevy-ts types
--coverage <report.json>`, an ignored test since it builds Rapier), through every field type.
For each type it takes a value (its `Default`, its `FromWorld`, else one shaped from its type
info), checks that `read` succeeds, that writing the JSON back over a fresh value gives an equal
one, and that its TS type is not `unknown`. A failure fails the test with the type path, the
component and field it was met in, and what failed, unless `reflect_value::UNCOVERED` lists the
type with a reason. An entry the walk never meets fails the test too, so the list cannot rot.

## Uncovered

`reflect_value::UNCOVERED`, each with its reason. A field of an uncovered type reads as absent and
fails to write with the reason; a hole of one fails the load.

| Type | Reason |
|---|---|
| `core::any::TypeId` | a type id is not data, opaque with no reflected serde |
| `wgpu_types::BlendState` | opaque with no reflected serde |
| `bevy_camera::camera::CameraMainTextureUsages` | wgpu texture usages, opaque with no reflected serde |
| `bevy_render::camera::CameraRenderGraph` | an interned render graph label, opaque with no reflected serde |
| `bevy_a11y::AccessibilityRequested` | shared state behind an Arc<AtomicBool>, opaque with no reflected serde |
| `bevy_picking::events::PointerState` | press and click times are Instants, opaque with no reflected serde |
| `bevy_time::time::Time<bevy_time::real::Real>` | its clock readings are Instants, opaque with no reflected serde |
| `bevy_scene::scene_component::SceneComponentInfo` | computed by bevy_scene; holds a &'static str |
| `bevy_animation::graph::AnimationGraph` | its graph is a petgraph DiGraph, opaque with no default or reflected serde (building graphs is animation-control's) |
| `bevy_image::image::Image` | reflect-assets: its wgpu texture and view descriptors are opaque with no reflected serde |
| `bevy_render::storage::ShaderBuffer` | opaque with no reflected serde |
| `bevy_mesh::mesh::Mesh` | script-assets: no default; attributes and indices are reflect(ignore); engine-core builds meshes |

`reflect-assets` marks an asset type that change covers; its landing removes the entry.
