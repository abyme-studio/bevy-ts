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

## No gap goes unchecked

`crates/bevy-ts-cli/tests/it/reflect_coverage.rs` walks every component, resource and asset in the
prebuilt engine's registry, and in a project engine with the Rapier port (`bevy-ts types
--coverage <report.json>`, an ignored test since it builds Rapier), through every field type.
For each type it takes a value (its `Default`, its `FromWorld`, else one shaped from its type
info), checks that `read` succeeds, that writing the JSON back over a fresh value gives an equal
one, and that its TS type is not `unknown`. A failure fails the test with the type path, the
component and field it was met in, and what failed, unless `reflect_value::UNCOVERED` lists the
type with a reason. An entry the walk never meets fails the test too, so the list cannot rot.

A component (not a resource) with neither `ReflectDefault` nor `ReflectFromWorld` is a `no
default` gap: TSX, levels and the editor build a component from its default, so they cannot insert
it. It fails the test unless `bevy_ts::type_data` gives it a default or `reflect_value::NO_DEFAULT`
lists it with a reason. A `type_data` entry Bevy already reflects fails the test too.

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

## Type data Bevy omits

`bevy_ts::type_data`, registered by `BevyTsPlugin`, adds what Bevy 0.20 leaves out of the types
bevy-ts exposes. TSX, levels, stock BRP and scenes read it through Bevy's registry, with no special
case of their own.

| Type data | For | Why |
|---|---|---|
| `ReflectDefault` | `VisibilityRange`, `RenderTarget`, `LineHeight` | they implement `Default` but do not reflect it |
| `ReflectDefault` | glTF names, `UiDebugOptions` and every `bevy_ui_widgets` component (feature `engine_type_data`, on in the prebuilt engine) | the same |
| `ReflectSerialize`, `ReflectDeserialize` | `Range<N>` and `RangeInclusive<N>` of every numeric `N` | bevy_reflect makes them opaque with `Clone` only; serde's `{ start, end }` is the JSON shape `reflect_value` uses |

To add an entry: when the hole check names a `no default` component that implements `Default`, add
`with_default::<T>(r, redundant)` to `type_data::defaults`, with the upstream file and line above
it. A type from a crate `bevy-ts` does not depend on goes under `engine_type_data`, with the Bevy
feature added to it. When Bevy reflects it itself, the test fails; delete the entry.

An HLOD in TSX: the detail mesh fades out over 8..12 m as its impostor fades in. A margin left
out is `0..0` (the derived default), so the impostor sets an `end_margin` past any view distance.
The first frames of a scene with `VisibilityRange` may be blank: Bevy compiles a mesh pipeline for
it in the background. A pixel test sets `synchronous_pipeline_compilation` (`common::sync_pipelines`).

```tsx
<entity name="tower">
  <mesh shape="cuboid" size={[1, 4, 1]} /><material color="#c0a080" />
  <VisibilityRange end_margin={{ start: 8, end: 12 }} />
</entity>
<entity name="tower-impostor">
  <mesh shape="cuboid" size={[1, 4, 1]} /><material color="#c0a080" unlit />
  <VisibilityRange start_margin={{ start: 8, end: 12 }} end_margin={{ start: 1000, end: 1001 }} />
</entity>
```

## No default

`reflect_value::NO_DEFAULT`: components with no Rust `Default`, or whose default is bookkeeping
Bevy owns, each with its reason. They read and
write like any other; TSX and levels cannot insert them.

| Component | Reason |
|---|---|
| `bevy_ecs::observer::distributed_storage::ObservedBy` | the observers watching an entity: bookkeeping Bevy owns; an empty one inserted from TSX would replace it |
| `bevy_ui_widgets::dialog::DialogDragState` | a drag in progress: bookkeeping Bevy owns; an empty one inserted from TSX would replace it |
| `bevy_ui_widgets::menu::MenuFocusState` | a menu's open state: bookkeeping Bevy owns; an empty one inserted from TSX would replace it |
| `bevy_ui_widgets::slider::SliderDragState` | a drag in progress: bookkeeping Bevy owns; an empty one inserted from TSX would replace it |
| `bevy_animation::AnimatedBy` | the Entity of its AnimationPlayer |
| `bevy_animation::AnimationTargetId` | a Uuid hashed from the target's name path |
| `bevy_camera::camera::MainPassResolutionOverride` | a resolution an upscaler sets |
| `bevy_core_pipeline::schedule::RootNonCameraView` | an interned schedule label bevy_core_pipeline makes |
| `bevy_dev_tools::picking_debug::PointerDebugOverlay` | a relationship to an Entity |
| `bevy_dev_tools::picking_debug::PointerDebugOverlayOf` | a relationship to an Entity |
| `bevy_ecs::resource::IsResource` | marks a resource's entity; made by the World |
| `bevy_extract::sync_world::MainEntity` | the render world's link to a main-world Entity |
| `bevy_input_focus::FocusedInput<` | an input event with its target Entity |
| `bevy_light::directional_light::DirectionalLightTexture` | needs an image Handle |
| `bevy_light::point_light::PointLightTexture` | needs an image Handle |
| `bevy_light::spot_light::SpotLightTexture` | needs an image Handle |
| `bevy_mesh::morph::MeshMorphWeights` | an enum with no default; set from the mesh's morph targets |
| `bevy_picking::mesh_picking::ray_cast::SimplifiedMesh` | needs a mesh Handle |
| `bevy_rapier3d::dynamics::soft_body::SoftBodyCluster` | needs its soft body Entity (Rapier port) |
| `bevy_render::view::window::screenshot::Screenshot` | a one-shot request for a RenderTarget, spawned by code |
| `bevy_time::delayed_commands::DelayedCommandQueue` | holds a CommandQueue; made by delayed commands |
| `bevy_ui::ui_node::UiTargetCamera` | needs a camera Entity |
| `bevy_ui::widget::viewport::ViewportNode` | needs a camera Entity |
| `bevy_ui_render::image::ImageNodeTextureAtlasLayout` | private to bevy_ui_render |
| `bevy_ui_widgets::scrollbar::Scrollbar` | needs a target Entity |
| `bevy_window::monitor::Monitor` | made by the window backend for each display |
| `bevy_window::monitor::PrimaryMonitor` | made by the window backend for the primary display |
