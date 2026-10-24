## MODIFIED Requirements

### Requirement: Observers
`class E extends Event { ...fields }` and `class E extends EntityEvent { ...fields }` (with `static propagate = true` to bubble up `ChildOf`) SHALL declare events, with the same field rules as components. An observer SHALL be an exported function whose first param is `On<E>`, `On<Add<C>>`, `On<Insert<C>>` or `On<Remove<C>>` and whose other params are system params, registered with `app.addObserver(fn)`. `On<E>` SHALL give the event's fields directly (`explode.radius`, as Rust's `On` derefs to the event), the target as `entity` (an entity event's own field; for a lifecycle event the entity it happened to) and `propagate(on)`. An entity event SHALL carry its target as its `entity` field, and `commands.trigger(Explode.of({ entity: e, radius: 3 }))` SHALL trigger it on `e`, as Rust's `commands.trigger(Explode { entity: e, radius: 3.0 })`. An observer triggered from a TS system's commands SHALL run when those commands are applied, after that system. An observer triggered from Rust (a lifecycle event of a Rust insert, a picking event) SHALL run at the next bevy-ts stage boundary in the same frame, in trigger order, with its propagation path computed in Rust at trigger time. `entity`, `propagate` and `toJSON` are `On<E>`'s own, so a TS event field by one of those names SHALL fail the load at the field with "field X shadows On<E>.X; rename it" (an `EntityEvent`'s `entity` is its target and stays allowed).

#### Scenario: Triggered from TS
- **WHEN** system `a` calls `commands.trigger(Explode.of({ entity: e, radius: 3 }))` and an observer on `On<Explode>` despawns `e`
- **THEN** the next system in the stage no longer sees `e`

#### Scenario: A field named like On's own
- **WHEN** `class Hit extends Event` declares a field `propagate`
- **THEN** the load fails at that field with "field propagate shadows On<Hit>.propagate; rename it"
