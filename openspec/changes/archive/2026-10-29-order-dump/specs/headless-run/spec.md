## ADDED Requirements

### Requirement: An order dump names what a shot drew

With `BEVY_TS_ORDER_DUMP=1`, each shot SHALL also write `<shot>.order.txt` beside it, from the
frame it captured. The file SHALL list each active camera's transform as f32 bits, and every
entity with an `Aabb`, with its `StableKey` and a hash of each material's reflected fields. A
handle in a material SHALL hash as its asset's path. Then, per camera view, it SHALL list
Opaque3d, AlphaMask3d, Opaque3dPrepass and AlphaMask3dPrepass: every bin in draw order with its
entities' keys, the unbatchable bins' entities in the order they draw, and the count of non-mesh
items. Last come Transparent3d's items with their keys. An item with no key SHALL show `-`, and a
key two items share SHALL be marked `*`. Entity ids SHALL appear only after a tab on their line.
Without the variable, no file SHALL be written and no dump system SHALL run.

#### Scenario: Two runs dump alike
- **WHEN** the glTF race scene is shot twice under `FixedStep` with `OrderDump`
- **THEN** both dumps exist and are equal once each line's text after a tab is dropped

#### Scenario: No dump unasked
- **WHEN** the same scene is shot without `OrderDump`
- **THEN** no `.order.txt` is written
