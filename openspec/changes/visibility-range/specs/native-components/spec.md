## ADDED Requirements

### Requirement: Bevy type data that Bevy omits is registered once
bevy-ts SHALL register, with `App::register_type_data` in one module, the type data Bevy 0.20 leaves out of reflected types bevy-ts exposes: `ReflectDefault` for each Bevy component that implements `Default` but does not reflect it, and `ReflectSerialize` and `ReflectDeserialize` for `Range<N>` and `RangeInclusive<N>` of every numeric `N`. Each entry SHALL name the upstream type it covers. An entry Bevy already reflects SHALL fail a test, so it is deleted. TSX, levels, stock BRP and scenes SHALL use this type data through Bevy's registry with no bevy-ts special case.

#### Scenario: VisibilityRange from TSX
- **WHEN** a scene has `<VisibilityRange start_margin={{ start: 0, end: 10 }} end_margin={{ start: 40, end: 50 }} />`
- **THEN** the entity has `VisibilityRange` with those margins and `use_aabb` false

#### Scenario: VisibilityRange over BRP
- **WHEN** a client calls `world.insert_components` with `bevy_camera::visibility::range::VisibilityRange` and margins as `{ start, end }`
- **THEN** the call succeeds and `world.get_components` returns the same margins

### Requirement: Every reflected component can be inserted or says why not
The hole check SHALL report a `no default` gap for any registered component with neither `ReflectDefault` nor `ReflectFromWorld`, unless it is on `reflect_value::UNCOVERED` with a reason. CI SHALL fail on such a gap.

#### Scenario: A Bevy upgrade adds a component without a reflected default
- **WHEN** the prebuilt engine registers a component with no `ReflectDefault` and no `ReflectFromWorld` that is not listed
- **THEN** the hole check fails naming the component and `no default`

### Requirement: HLOD cross-fade from TSX
A detail mesh and its impostor given `VisibilityRange` from TSX SHALL swap by camera distance, with both drawn inside the shared margin, in an app that renders.

#### Scenario: Near, far and in the margin
- **WHEN** a red cube has `end_margin` 8..12 and a blue cube at the same spot has `start_margin` 8..12, and the camera is 4 m, 20 m, then 10 m away
- **THEN** at 4 m only red is drawn, at 20 m only blue, and at 10 m the frame holds both red and blue pixels
