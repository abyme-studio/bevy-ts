## REMOVED Requirements

### Requirement: Native spawn recipes
**Reason**: Scripts spawn the same entities from components: reflect-tier and hole values insert through commands, and `Transform.lookingAt` replaces `position`/`lookAt`. `spawnNative` and `plugin_api::add_ts_native_recipe` stay for recipes a plugin registers; bevy-ts registers none.
**Migration**: `spawnNative("camera3d", { position, lookAt, projection })` → `commands.spawn(Camera3d, Transform.lookingAt(position, lookAt), Projection.of(projection))`; `spawnNative("directionalLight", { position, lookAt, illuminance, shadows })` → `commands.spawn(DirectionalLight.of({ illuminance, shadows_enabled: shadows }), Transform.lookingAt(position, lookAt))`; `spawnNative("cuboid", { size, color, position })` → `commands.spawn(Transform.of({ translation: position }), <entity><mesh shape="cuboid" size={size} /><material color={color} /></entity>)`, or the same in a scene.

## ADDED Requirements

### Requirement: Commands insert reflected values
`commands.spawn` and `entity(e).insert` SHALL take reflect-tier values (`Projection.of({ Orthographic: {} })`, or the class for its default) and layout-tier values that set holes (`DirectionalLight.of({ illuminance: 8000, color: hex("#ffd7a8") })`). A reflect value SHALL cross as JSON in its own command op and be written by `bevy_ts::reflect_value::write` over the type's `ReflectDefault` or `ReflectFromWorld`; a layout value's holes SHALL be written the same way right after its bytes, in the same command. A value that fails to write SHALL fail that command with an error naming the type and the field, and the system's other commands SHALL still apply. A spawn naming a removed built-in recipe SHALL fail with an error naming its replacement.

#### Scenario: An orthographic camera from TS
- **WHEN** a `Startup` system calls `commands.spawn(Camera3d, Transform.lookingAt([10, 10, 10], [0, 0, 0]), Projection.of({ Orthographic: { scaling_mode: { FixedVertical: { viewport_height: 12 } } } }))`
- **THEN** the camera's `Projection` is `Orthographic` with `FixedVertical { viewport_height: 12 }` and every other field at its default, and its transform looks at the origin

#### Scenario: A warm sun from TS
- **WHEN** a system spawns `DirectionalLight.of({ illuminance: 8000, color: srgb(1, 0.85, 0.7) })`
- **THEN** the light has both values

#### Scenario: A removed recipe
- **WHEN** a system calls `commands.spawnNative("camera3d", {})`
- **THEN** the command fails with an error saying the recipe was removed and naming `commands.spawn(Camera3d, Transform.lookingAt(...))`

### Requirement: Transform.lookingAt
`Transform.lookingAt(eye, target, up = [0, 1, 0])` SHALL return a `Transform` value at `eye` whose rotation matches Bevy's `Transform::looking_at(target, up)`, computed in JS with no host call.

#### Scenario: Matches Bevy
- **WHEN** a script builds `Transform.lookingAt([-3, 3, 6], [0, 0, 0])`
- **THEN** its rotation equals Bevy's `Transform::from_xyz(-3., 3., 6.).looking_at(Vec3::ZERO, Vec3::Y)` within 1e-6 per lane

### Requirement: Reflected rows in TS systems
A reflect-tier component SHALL be accepted as query data (`C`, `Mut<C>`, `Option<C>`) and a reflect-tier resource as `Res<R>`/`ResMut<R>`, and a layout-tier row SHALL offer each hole as a getter and setter. A reflected row's value SHALL be cached across runs, keyed by the component's changed tick: a row whose tick has not moved SHALL return the cached value with no host call. A miss SHALL cost one host call, filled through a binary codec compiled once per type from its `TypeInfo`, whose decoded value equals `JSON.parse` of `bevy_ts::reflect_value::read`'s JSON. Rows the system does not touch SHALL cost no host call. A write through `Mut` SHALL be applied at once with `write`'s semantics, mark the component changed, and refresh the cache entry. Reflected writes SHALL NOT invalidate column views open in the same run. Writing a component Bevy computes (`GlobalTransform`, `ComputedNode`) SHALL throw naming it.

#### Scenario: Switching a camera to orthographic
- **WHEN** a system with `Query<[Mut<Projection>], With<Camera3d>>` sets `p.value = { Orthographic: {} }`
- **THEN** the camera renders orthographic that frame, and a later system with `Changed<Projection>` sees the row

#### Scenario: An unchanged row across runs
- **WHEN** a system reads `p.value` of a camera's `Projection` on two frames and nothing changes it in between
- **THEN** the second read makes no host call and returns an equal value

#### Scenario: Untouched rows
- **WHEN** a system queries 1,000 entities with `Query<[Transform, DirectionalLight]>` and reads only `translation`
- **THEN** no reflection host call is made in that run
