## MODIFIED Requirements

### Requirement: Generated field accessors
For each component, the `bevy-ts` module SHALL give TS code a typed view object whose properties read and write the fields of one row in place (`mover.speed`, `t.translation.y`), plus the raw offsets and size for code that indexes the typed arrays itself. Every setter SHALL also store the run's change tick in the row's changed tick (see ts-systems). TS types for declared components SHALL be the classes themselves, with no codegen step. A component read by a query SHALL be typed read-only: its fields `readonly`, its vector fields `ReadonlyVec3`/`ReadonlyQuat` without mutating methods. A written component (`Mut<C>`) SHALL type its scalar fields as assignable and its vector fields as views that are not reassignable and are mutated in place with glam-style methods that write `this` and allocate nothing (`set`, `copy`, `add`, `sub`, `scale`, `addScaled`, `normalize`, `lerp`, `subVectors`, `rotate`, ...). `vec3(x, y, z)` and `quat()` SHALL make scratch values with the same methods. A vector view SHALL read its row from its parent accessor, not keep a copy updated by a setter on every row move (operator ruling 2026-10-20: nested views measured 3.5× raw arrays at 1M rows when re-positioned per row, `bench/dx/results/iter-forms.md`). A vector method that writes several components (`addScaled`, `normalize`, `lerp`, ...) SHALL store the row's change tick once per call, not once per component; a plain field setter keeps its one store, the form measured cheapest on V8 (`docs/research/change-detection.md`). An accessor SHALL keep its own state (its row, the run's tick, the row's tick column, its typed views and its nested views) under keys no field can name, so a field may take any name, and SHALL NOT refuse or rename a field for its name.

#### Scenario: A field named like the accessor's state
- **WHEN** a component, a resource, a `Local`, a message or an event declares fields such as `t: Vec3`, `r: f32`, `k: u32`, `f32: f32` or `present: f32`, and a system writes them
- **THEN** each field reads back the value written, and a written component's row is marked changed

#### Scenario: Typed access in the editor
- **WHEN** a system's `run` writes `mover.sped = 2`
- **THEN** `tsc` reports that `sped` does not exist on the component's type

#### Scenario: Writing a read component
- **WHEN** a system with `q: Query<[Mover]>` writes `mover.speed = 2`
- **THEN** `tsc` reports that `speed` is read-only

#### Scenario: A vector method marks once
- **WHEN** a system calls `t.translation.addScaled(dir, s)` on a row
- **THEN** all three components are written and the row's changed tick is stored once
