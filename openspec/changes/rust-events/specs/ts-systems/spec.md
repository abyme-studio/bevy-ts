## MODIFIED Requirements

### Requirement: Observers
`class E extends Event { ...fields }` and `class E extends EntityEvent { ...fields }` (with `static propagate = true` to bubble up `ChildOf`) SHALL declare events, with the same field rules as components. An observer SHALL be an exported function whose first param is `On<E>`, `On<Add<C>>`, `On<Insert<C>>` or `On<Remove<C>>` and whose other params are system params, registered with `app.addObserver(fn)`. `On<E>` SHALL give the event's fields directly (`explode.radius`, as Rust's `On` derefs to the event), the current target as `entity` (an entity event's own field; for a lifecycle event the entity it happened to), the first target of a bubbling event as `original` (Bevy's `original_event_target`; for a lifecycle event the entity it happened to; undefined for a global event) and `propagate(on)`. An entity event SHALL carry its target as its `entity` field, and `commands.trigger(Explode.of({ entity: e, radius: 3 }))` SHALL trigger it on `e`, as Rust's `commands.trigger(Explode { entity: e, radius: 3.0 })`. An observer triggered from a TS system's commands SHALL run when those commands are applied, after that system. An observer triggered from Rust (a lifecycle event of a Rust insert, a picking event) SHALL run at the next bevy-ts stage boundary in the same frame, in trigger order, with its propagation path computed in Rust at trigger time. `entity`, `original`, `propagate` and `toJSON` are `On<E>`'s own, so a TS event field by one of those names SHALL fail the load at the field with "field X shadows On<E>.X; rename it" (an `EntityEvent`'s `entity` is its target and stays allowed).

`commands.entity(e).observe(fn)`, as Bevy's `EntityCommands::observe`, SHALL attach an observer to `e` alone, for a TS or Rust entity event. `fn` SHALL be an exported function whose first param is `On<E>`; it need not be passed to `addObserver`. The observer SHALL be built on first use, keyed by the function's declaration (`file#name`), and SHALL follow that declaration across a reload. It SHALL run only for triggers whose current target is `e`, bubbling hops included, and SHALL be removed when `e` is despawned. A function whose first param is not an entity event SHALL fail that run's commands with an error naming it.

#### Scenario: Triggered from TS
- **WHEN** system `a` calls `commands.trigger(Explode.of({ entity: e, radius: 3 }))` and an observer on `On<Explode>` despawns `e`
- **THEN** the next system in the stage no longer sees `e`

#### Scenario: A field named like On's own
- **WHEN** `class Hit extends Event` declares a field `propagate`
- **THEN** the load fails at that field with "field propagate shadows On<Hit>.propagate; rename it"

#### Scenario: An observer on one entity
- **WHEN** a system calls `commands.entity(a).observe(onHit)` where `onHit` takes `On<Hit>`, and later `Hit` is triggered on `a` and on `b`
- **THEN** `onHit` runs once, for `a`; after `a` is despawned a `Hit` on a new entity does not run it

#### Scenario: An entity observer on a Rust event
- **WHEN** a system calls `commands.entity(button).observe(clicked)` where `clicked` takes `On<PointerClick>`, and the user clicks `button`
- **THEN** `clicked` runs at the next bevy-ts stage start with `entity` the button

#### Scenario: Reload keeps entity observers
- **WHEN** `clicked`'s body changes and the module reloads while `button` lives
- **THEN** the next click on `button` runs the new body, once
