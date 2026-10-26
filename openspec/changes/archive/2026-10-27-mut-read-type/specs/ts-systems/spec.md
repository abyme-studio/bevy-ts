## MODIFIED Requirements

### Requirement: Reflected rows in TS systems
A reflect-tier component SHALL be accepted as query data (`C`, `Mut<C>`, `Option<C>`) and a reflect-tier resource as `Res<R>`/`ResMut<R>`, and a layout-tier row SHALL offer each hole as a getter and setter. A reflected row's value SHALL be cached across runs, keyed by the component's changed tick: a row whose tick has not moved SHALL return the cached value with no host call. A miss SHALL cost one host call, filled through a binary codec compiled once per type from its `TypeInfo`, whose decoded value equals `JSON.parse` of `bevy_ts::reflect_value::read`'s JSON. Rows the system does not touch SHALL cost no host call. A write through `Mut` SHALL be applied at once with `write`'s semantics, mark the component changed, and drop the cache entry, so the next read fetches what Rust made of it. A field assigned on a `Mut` row SHALL keep the in-place changes made to that row's value earlier in the run. Reflected writes SHALL NOT invalidate column views open in the same run. Writing a component Bevy computes (`GlobalTransform`, `ComputedNode`) SHALL throw naming it.

A reflected member (a layout-tier hole; a reflect-tier row's `value` and struct fields) SHALL be typed for reading as `read` gives it, on every row kind (query data, `Mut`, `Ref`, `Option`, `Res`, `ResMut`, `ref.get`): struct fields present, a 64-bit integer a `number`, a `Color` in Bevy's shape (`ColorRead`), a `Handle<T>` its path, ref or `null`. An assignment on `Mut` or `ResMut` SHALL take every form `write` takes. TSX props and `.of()` SHALL keep the write type. `Res` and `ResMut` of a layout-tier resource SHALL have its holes.

#### Scenario: Switching a camera to orthographic
- **WHEN** a system with `Query<[Mut<Projection>], With<Camera3d>>` sets `p.value = { Orthographic: {} }`
- **THEN** the camera renders orthographic that frame, and a later system with `Changed<Projection>` sees the row

#### Scenario: An unchanged row across runs
- **WHEN** a system reads `p.value` of a camera's `Projection` on two frames and nothing changes it in between
- **THEN** the second read makes no host call and returns an equal value

#### Scenario: Untouched rows
- **WHEN** a system queries 1,000 entities with `Query<[Transform, DirectionalLight]>` and reads only `translation`
- **THEN** no reflection host call is made in that run

#### Scenario: An atlas index reads as a number
- **WHEN** a system with `Query<[Mut<Sprite>]>` writes `const i: number = sprite.texture_atlas!.index`
- **THEN** it type-checks, and `sprite.texture_atlas = { index: "3" }` type-checks too

#### Scenario: A hole reads in Bevy's shape and takes any form
- **WHEN** a system with `Query<[Mut<DirectionalLight>]>` reads `light.color` and assigns `light.color = "#ffd7a8"`
- **THEN** the read is typed `ColorRead` and the assignment type-checks; on `Query<[DirectionalLight]>` the assignment is a type error
