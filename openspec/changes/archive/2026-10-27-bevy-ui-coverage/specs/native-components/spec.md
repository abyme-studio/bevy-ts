## ADDED Requirements

### Requirement: A one-field tuple struct's field is value
A layout-tier or reflect-tier Rust component that is a tuple struct with exactly one field (`ZIndex(i32)`, `TabIndex(i32)`, `Hovered(bool)`, `ScrollPosition(Vec2)`) SHALL expose that field to TS as `value`, everywhere TS names fields: TSX component elements, `NodeRef.get`, TS system columns, `bevy-ts types` and `bevy.d.ts`. A tuple struct with two or more fields SHALL keep `"0"`, `"1"`, .... Level files and the remote protocol SHALL keep Bevy's reflection paths (`.0`).

#### Scenario: Z order from TSX
- **WHEN** a project writes `<node><ZIndex value={2} /></node>`
- **THEN** `tsc` reports no errors and the entity has `ZIndex(2)`

#### Scenario: A TS system reads a newtype
- **WHEN** a TS system queries `Query<[TabIndex]>` and reads `t.value`
- **THEN** it reads the `i32` inside `TabIndex`, and `t["0"]` is a type error
