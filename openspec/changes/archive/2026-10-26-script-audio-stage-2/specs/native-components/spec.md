## ADDED Requirements

### Requirement: Reflect-tier Rust resources are reflected handles
`bevy-ts/bevy` SHALL ship the prebuilt engine's reflect-tier resources (such as `GlobalVolume`, `DefaultSpatialScale` and `ClearColor`) as reflected handles, named by the components' rule and typed with their fields, usable as `Res<R>`, `ResMut<R>`, `Option<Res<R>>` and `Option<ResMut<R>>`. A resource in neither tier SHALL have no handle; nor SHALL one a built-in module's param stands for (`GizmoConfigStore`), nor one on `types.rs`'s `NO_HANDLE` list, each entry with its reason: types that are never a resource (`ButtonInput<GamepadButton>`) and bevy-ts's and Bevy's internals (`TsSavedResources`, `UiStack`). `docs/native.md` SHALL list every handle that ships.

#### Scenario: GlobalVolume from bevy-ts/bevy
- **WHEN** a script imports `GlobalVolume` from `bevy-ts/bevy`
- **THEN** it is a reflected handle whose `volume` is typed `{ Linear: number } | { Decibels: number }`, and `ResMut<GlobalVolume>` writes it

#### Scenario: Not a resource a game takes
- **WHEN** `bevy-ts/bevy` is generated
- **THEN** it has no `ButtonInputGamepadButton` and no `TsSavedResources`

### Requirement: TSX builds a FromWorld default from the live world
When TSX or a level inserts a reflected component whose only default is a `FromWorld` that panics on an empty world, it SHALL build the default from the live world, as a script's `C.of(...)` does. One that panics on the live world too SHALL not be inserted, and the script log SHALL name the type.

#### Scenario: A default that reads a resource
- **WHEN** a component's `FromWorld` reads a resource the app has, and TSX renders it with one prop
- **THEN** the entity has the component with the resource's value and the prop written over it

#### Scenario: A default that cannot be built
- **WHEN** that resource is absent
- **THEN** the load applies, the entity has no such component, and the script log has `<type>'s FromWorld panicked`
