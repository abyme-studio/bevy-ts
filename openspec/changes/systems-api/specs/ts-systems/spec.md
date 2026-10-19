## ADDED Requirements

### Requirement: Time as a resource view
`Res<Time>` SHALL give a read-only view with `delta` (seconds since the last frame, `f32`), `elapsed` (seconds since start, `f64`) and `frame` (`u32`), taken from Bevy's `Time` for the schedule the system runs in (`Time<Fixed>` in `FixedUpdate`). `ResMut<Time>` SHALL fail the load with its location.

#### Scenario: Fixed step
- **WHEN** a `FixedUpdate` system reads `time.delta`
- **THEN** it reads the fixed timestep, not the frame's delta

### Requirement: Changed and Added rows reach JS as row lists
For a query with `Changed<C>` or `Added<C>`, Rust SHALL compare each matched row's tick for `C` with the system's last-run tick, kept per system identity, and SHALL hand JS the matching row indices per table. `each`, iteration, `get` and `single` SHALL visit only those rows; `table.rows` SHALL expose them, and `table.read`, `write` and `split` SHALL still cover every row.

#### Scenario: Only changed rows
- **WHEN** a Rust system changes `Health` on 2 of 1,000 entities and a TS system with `Query<[Health], Changed<Health>>` runs next
- **THEN** its `each` visits exactly those 2 rows

### Requirement: Where observers run
An observer triggered by a TS system's `commands.trigger` SHALL run when that system's commands are applied, before the next system. An observer of a lifecycle event fired while a TS system's commands are applied SHALL run at the same point. An observer triggered from Rust SHALL run at the start of the next bevy-ts stage in the same frame. Each triggered event SHALL be one call into JS.

#### Scenario: A lifecycle observer on a Rust insert
- **WHEN** a Rust `Update` system inserts `Health` on an entity and a TS observer takes `On<Add<Health>>`
- **THEN** the observer runs at the start of the `PostUpdate` bevy-ts stage that frame
