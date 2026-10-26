## MODIFIED Requirements

### Requirement: Resources
`class R extends Resource { ...fields }` SHALL declare a resource with the same field rules, naming and layout as a component (ts-components). A resource SHALL be a component on its own resource entity, as in Bevy 0.20 (`trait Resource: Component`), so `Res<R>` and `ResMut<R>` are one-row column views through the same zero-copy path as a query. `app.insertResource(R.of({...}))` SHALL insert it at the generation's apply if absent; a reload SHALL keep its value, and a `restart` reload or a Stop (hot-reload, edit-mode) SHALL insert it again with its `insertResource` value.

A layout-tier Rust resource (native-components) SHALL be reachable the same way: `Res<R>` reads its own row in place, and `ResMut<R>` writes it in place and marks its change tick, so a Rust system's `Res<R>::is_changed()` sees the write that frame. A resource name SHALL resolve to a TS resource first, then to a layout-tier Rust resource by type path or by short name when only one type has it. `Time` is not on this path (see "Time as a resource view"). The load SHALL fail, naming the system, the param and the resource, when the name is:
- a reflect-tier Rust resource named through a layout class (`nativeResource`), with the tier's reason; through its reflected handle (`bevy-ts/bevy`, `reflectClass`) it loads, as "Reflected rows in TS systems" says;
- a Rust component that is not a resource;
- a short name more than one type shares, with their type paths;
- unknown;
- or, for `ResMut`, an immutable type.

A Rust resource of either tier named as query data (`Query<[ClearColor]>`) SHALL fail the load naming the system and saying the type is a resource, not a component, to be read with `Res<R>`. It SHALL NOT report that the type is stored in a sparse set, though Bevy 0.20 stores a resource as a sparse-set component.

When the resource is absent at run time, `Res<R>` and `ResMut<R>` SHALL fail the run with an error naming the resource, as Bevy's `Res<T>` fails validation. `Option<Res<R>>` and `Option<ResMut<R>>` SHALL be params for TS and Rust resources alike: `undefined` when the resource is absent, and the system runs. `runIf(resourceExists(R))` SHALL skip the system instead.

#### Scenario: A score
- **WHEN** a system with `score: ResMut<Score>` adds 1 to `score.value`
- **THEN** a Rust system reading `Score`'s component id sees the new value that frame

#### Scenario: A score across a save and a Stop
- **WHEN** `Score` is 5, `game.ts` is saved and reloads, then a client sets `edit` after play
- **THEN** `Score` is still 5 after the save, and back at its `insertResource` value after the Stop

#### Scenario: A Bevy resource written from TS
- **WHEN** a TS system with `ambient: ResMut<GlobalAmbientLight>` sets `ambient.brightness = 400`, and a Rust system after it reads `Res<GlobalAmbientLight>`
- **THEN** the Rust system reads 400 and `is_changed()` is true that frame, and false the next frame if TS does not write again

#### Scenario: A Bevy resource read without a write
- **WHEN** a TS system takes `scale: Res<UiScale>` and the app's `UiScale` is 2
- **THEN** it reads `scale.value === 2` (a newtype's one field is `value`) and Bevy's change tick for `UiScale` does not move

#### Scenario: A reflect-tier resource
- **WHEN** a TS system takes `Res<ClearColor>` with `ClearColor` from `bevy-ts/bevy`, and the app's `ClearColor` is red
- **THEN** it loads and reads the red colour as `value`

#### Scenario: A reflect-tier resource through a layout class
- **WHEN** a TS system takes `Res<ClearColor>` with `class ClearColor extends nativeResource("ClearColor", { v: f32 })`
- **THEN** the load fails naming the system, `ClearColor` and its reflect-tier reason

#### Scenario: A resource named as query data
- **WHEN** a TS system takes `Query<[ClearColor]>` with `ClearColor` from `bevy-ts/bevy`
- **THEN** the load fails naming the system and saying `ClearColor is a resource, not a component; take Res<ClearColor>`, with no mention of a sparse set

#### Scenario: A component named as a resource
- **WHEN** a TS system takes `Res<Transform>`
- **THEN** the load fails saying `Transform` is a component, not a resource

#### Scenario: An absent Rust resource
- **WHEN** a TS system takes `Res<UiScale>` in an app with no `UiScale`
- **THEN** the run fails with an error naming `UiScale`, and the same system with `Option<Res<UiScale>>` runs and reads `undefined`

#### Scenario: An optional TS resource
- **WHEN** a system takes `score: Option<ResMut<Score>>` and `Score` is inserted only after its first run
- **THEN** the first run reads `undefined`, and later runs write `Score` in place


## ADDED Requirements

### Requirement: GlobalTransform reads as Bevy's GlobalTransform
`GlobalTransform` SHALL be a class `bevy-ts` ships typed, as `Transform` is, and `bevy-ts/bevy` SHALL re-export it rather than generate a handle for it. Its rows (query data, `Ref`, `Option`, and `ref.get` in TSX) SHALL offer Bevy 0.20's `GlobalTransform` read methods by Bevy's names in camelCase, computed from its affine with Bevy's math:

- `translation()`, `rotation()`, `scale()`, `toScaleRotationTranslation()` and `computeTransform()` (a `Transform`);
- `affine()` (the 12 floats, column-major, as `Affine3A::to_cols_array`) and `toMatrix()` (16 floats, as `Mat4::to_cols_array`);
- `right()`, `left()`, `up()`, `down()`, `back()` and `forward()`, each a unit `Vec3`;
- `transformPoint(p)`, `transformVector(v)`, `reparentedTo(parent)` (a `Transform`) and `mulTransform(t)` (a `GlobalTransform`).

Each SHALL equal Bevy's method on the same value within 1e-5 per lane. A vector or quaternion a method returns SHALL be a fresh value the caller may change. The row's `value` SHALL stay the 12 affine floats, so a host function taking a transform still takes the row. The read SHALL keep the reflected row's cost: cached by changed tick, one host call on a miss, none for a row not read. A write SHALL still throw naming `GlobalTransform`; `GlobalTransform.of(...)` and the `from*` constructors are not offered.

#### Scenario: Reading a child's world position
- **WHEN** a child at local `[1, 0, 0]` has a parent at `[0, 2, 0]`, rotated a quarter turn about Y and scaled 2, and a TS system in `PostUpdate` after `bevy:transform-propagate` with `Query<[GlobalTransform]>` reads `g.translation()`, `g.rotation()`, `g.scale()`, `g.forward()` and `g.transformPoint([0, 0, 1])`
- **THEN** each equals what a Rust system reads from the same `GlobalTransform` that frame, within 1e-5 per lane

#### Scenario: After propagation
- **WHEN** an `Update` system moves an entity's `Transform` 1 along X each frame and a `PostUpdate` system after `bevy:transform-propagate` logs `g.translation().x`
- **THEN** it logs 1 on the first frame and 2 on the second

#### Scenario: A row passed to a host function
- **WHEN** a system passes a `GlobalTransform` row to a host function that takes a transform
- **THEN** the function receives that `GlobalTransform` as given, as before this change

#### Scenario: A write
- **WHEN** a system takes `Mut<GlobalTransform>`
- **THEN** tsc rejects it, and a write that reaches the runtime throws naming `GlobalTransform`
