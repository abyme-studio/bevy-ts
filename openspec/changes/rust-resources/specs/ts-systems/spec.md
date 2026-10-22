## MODIFIED Requirements

### Requirement: Resources
`class R extends Resource { ...fields }` SHALL declare a resource with the same field rules, naming and layout as a component (ts-components). A resource SHALL be a component on its own resource entity, as in Bevy 0.20 (`trait Resource: Component`), so `Res<R>` and `ResMut<R>` are one-row column views through the same zero-copy path as a query. `app.insertResource(R.of({...}))` SHALL insert it at the generation's apply if absent; a reload SHALL keep its value, and a `restart` reload or a Stop (hot-reload, edit-mode) SHALL insert it again with its `insertResource` value.

A layout-tier Rust resource (native-components) SHALL be reachable the same way: `Res<R>` reads its own row in place, and `ResMut<R>` writes it in place and marks its change tick, so a Rust system's `Res<R>::is_changed()` sees the write that frame. A resource name SHALL resolve to a TS resource first, then to a layout-tier Rust resource by type path or by short name when only one type has it. `Time` is not on this path (see "Time as a resource view"). The load SHALL fail, naming the system, the param and the resource, when the name is:
- a reflect-tier Rust resource, with the tier's reason;
- a Rust component that is not a resource;
- a short name more than one type shares, with their type paths;
- unknown;
- or, for `ResMut`, an immutable type.

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
- **THEN** it reads `scale[0] === 2` (the tuple field's reflected name, `0`) and Bevy's change tick for `UiScale` does not move

#### Scenario: A reflect-tier resource
- **WHEN** a TS system takes `Res<ClearColor>`
- **THEN** the load fails naming the system, `ClearColor` and its reflect-tier reason

#### Scenario: A component named as a resource
- **WHEN** a TS system takes `Res<Transform>`
- **THEN** the load fails saying `Transform` is a component, not a resource

#### Scenario: An absent Rust resource
- **WHEN** a TS system takes `Res<UiScale>` in an app with no `UiScale`
- **THEN** the run fails with an error naming `UiScale`, and the same system with `Option<Res<UiScale>>` runs and reads `undefined`

#### Scenario: An optional TS resource
- **WHEN** a system takes `score: Option<ResMut<Score>>` and `Score` is inserted only after its first run
- **THEN** the first run reads `undefined`, and later runs write `Score` in place
