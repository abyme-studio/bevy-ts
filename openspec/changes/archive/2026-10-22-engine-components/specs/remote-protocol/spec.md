## ADDED Requirements

### Requirement: Schemas say whose a component is and how an editor treats it
Every entry `bevy-ts/schema` returns, TS and Rust alike, SHALL carry an `x-bevy-ts-editor` object with four fields, all required:

- `origin`: `"engine"` when the component's type comes from one of Bevy's own crates at the pinned Bevy version or from a bevy-ts crate; `"game"` for every TS component and for a type from the project's native crate; `"plugin"` for a type from any other crate. Origin SHALL be decided from the type's crate, not from a name prefix: `bevy_rapier3d` is `plugin`.
- `internal`: `true` for a component the engine maintains and nobody authors (GlobalTransform, InheritedVisibility, ViewVisibility, VisibilityClass, Aabb, TransformTreeChanged, TransformGizmoFocus, Children and the like). bevy-ts SHALL own this list. Only an `engine` component SHALL ever be internal.
- `addable`: `true` when an editor may offer the component in Add component: a reflected Component with `ReflectDefault` or `ReflectFromWorld`, mutable, not a resource (Bevy 0.20 stores resources as components; they carry `ReflectResource`) and not internal; a TS component while its live generation declares it. Otherwise `false`.
- `category`: one of `transform`, `render`, `light`, `camera`, `physics`, `audio`, `ui`, `animation`, `game`, `other`. Every `game` component SHALL be `game`. An `engine` component SHALL take its category from bevy-ts's table, `other` when unlisted. A `plugin` component SHALL be `other` unless bevy-ts's table names its crate.

`bevy-ts/schema` without `names` SHALL still list every registered reflected Component and every TS component, live and undeclared. `x-bevy-ts` SHALL stay as it is, present on TS components only. A client SHALL treat an unknown `category` as `other`, and an entry without `x-bevy-ts-editor` as from an engine before protocol 1.11.0.

#### Scenario: The inspector hides what the engine maintains
- **WHEN** the inspector asks `bevy-ts/schema` for `bevy_transform::components::global_transform::GlobalTransform` and `bevy_transform::components::transform::Transform`
- **THEN** both have `origin: "engine"` and `category: "transform"`, GlobalTransform has `internal: true` and `addable: false`, and Transform has `internal: false` and `addable: true`

#### Scenario: A resource is never offered for an entity
- **WHEN** the inspector asks `bevy-ts/schema` for `bevy_camera::clear_color::ClearColor`
- **THEN** its entry has `addable: false`

#### Scenario: A TS component is the game's
- **WHEN** the live generation declares `game::Mover`
- **THEN** its entry has `x-bevy-ts-editor` `{ origin: "game", internal: false, addable: true, category: "game" }` beside its `x-bevy-ts`

#### Scenario: An undeclared TS component cannot be added
- **WHEN** a reload drops `game::Mover` but entities still hold its data
- **THEN** its entry has `addable: false` and `x-bevy-ts.declared: false`

#### Scenario: A third-party crate is a plugin
- **WHEN** the rapier example's engine answers `bevy-ts/schema` for `bevy_rapier3d::dynamics::rigid_body::RigidBody`
- **THEN** its origin is `plugin`, not `engine`, and its category is `physics` because bevy-ts's table names `bevy_rapier3d`

#### Scenario: The project's native crate is the game's
- **WHEN** a project's native crate `native` registers a reflected component `native::Fuel`
- **THEN** its entry has `origin: "game"` and `category: "game"`
