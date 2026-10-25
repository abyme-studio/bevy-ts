## ADDED Requirements

### Requirement: Forwarding a Rust message into a TS message
`plugin_api::write_message(world, name, values)` SHALL push a message into the TS message type
the live generation declares under `name` (its `static path`), field values as JSON, and SHALL
return `false` when no such TS message is declared, so a port forwards only what TS reads. It is
the seam for a Rust message without `Reflect`, mapped by the port onto a TS-declared message
(Rapier's `CollisionEvent`). A reflected Rust message SHALL use `ts_message` instead.

#### Scenario: Collisions read only when TS asks
- **WHEN** the Rapier port forwards `CollisionEvent`s and no TS system declares `bevy_rapier3d::pipeline::events::CollisionEvent`
- **THEN** `write_message` returns `false` and the port stops forwarding that frame
