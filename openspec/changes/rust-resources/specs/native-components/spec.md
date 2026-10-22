## ADDED Requirements

### Requirement: Layout-tier Rust resources are Resource classes
A registered Rust resource SHALL be tiered as a component is (a resource is a component in Bevy 0.20), and the registry SHALL record that it is a resource: from `ReflectResource` for a reflected type, and from `app.ts_resource_layout::<T>()` (`T: NativeLayout + Resource`) for a derive-only one, the resource sibling of `ts_layout::<T>()`. A layout-tier resource SHALL reach TS as a `Resource` class, usable only as `Res<R>`, `ResMut<R>`, `Option<Res<R>>` and `Option<ResMut<R>>` (ts-systems), never as query data. `bevy-ts/bevy` SHALL ship the prebuilt engine's layout-tier resources (such as `GlobalAmbientLight` and `UiScale`) and `native.d.ts` a project's, named by the components' rule: the type's short name, prefixed by its crate when two types share it. `bevy-ts` SHALL export `nativeResource(name, fields)`, the resource sibling of `nativeComponent`, for those modules and for native crates shipping their own resources. Holes and read-only fields SHALL follow the component rules. `Time` SHALL stay the class `bevy-ts` itself exports.

#### Scenario: A Bevy resource from bevy-ts/bevy
- **WHEN** a script imports `GlobalAmbientLight` from `bevy-ts/bevy`
- **THEN** it is a `Resource` class with `brightness: f32` and the `bool` field writable, a doc note naming `color` as a hole, and `Query<[GlobalAmbientLight]>` fails the load as not a component

#### Scenario: A derive-only resource from a native crate
- **WHEN** a native crate registers `#[derive(Resource, NativeLayout)] struct Wind { speed: f32 }` with `app.ts_resource_layout::<Wind>()` and ships `class Wind extends nativeResource("game_native::Wind", { speed: f32 }) {}`
- **THEN** a TS system with `ResMut<Wind>` writes `speed` in place and a Rust system's `Res<Wind>` reads it that frame

#### Scenario: Determinism with resources
- **WHEN** `scripts/gen-bevy-types.sh --check` runs after the generated files are committed
- **THEN** it passes: the `bevy-ts/bevy` outputs, resource classes included, are byte-identical
