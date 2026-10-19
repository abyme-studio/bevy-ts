# plugin-api Specification

## Purpose
TBD - created by archiving change bridge. Update Purpose after archive.

## Requirements

### Requirement: Feature crates extend bevy-ts through a public Rust API
`bevy-ts` SHALL expose a public plugin API through which other Rust crates (feature crates such as a future TSX crate) extend the bridge without changing it: manifest sections, host-invoked entries, command kinds, component lookup, reflected component access and restart notification. Every extension point SHALL be engine-neutral and SHALL work against the fake backend.

#### Scenario: A feature crate against the fake backend
- **WHEN** a test plugin registers a manifest section, an entry and a command kind, and runs against the fake backend
- **THEN** all three work with no V8 in the build

### Requirement: Manifest sections
A plugin SHALL be able to register a named section of the generation manifest. JS code SHALL be able to write that section during module evaluation, through the `bevy-ts` module, under the registered name. The plugin SHALL receive the section's bytes when the generation is applied, in the same frame-boundary step as component and system declarations. The plugin SHALL be able to reject the section with errors carrying TS locations; a rejection SHALL fail the whole generation under the failed-reload rule. A section name nobody registered SHALL fail the load. A plugin MAY also register an apply step for its section, which SHALL run once the generation is committed (components live and migrated, systems replaced) and before the generation's startups and systems, and which cannot fail the generation; it SHALL also run, told the section is absent, for an applied generation that does not write the section; while the generation is checked, the plugin SHALL be able to read the pending components' declarations and encode values for them.

#### Scenario: A plugin rejects its section
- **WHEN** a plugin's section handler returns an error located at `game.tsx:12:5`
- **THEN** the generation is not applied, the previous one keeps running, and the error is reported at `game.tsx:12:5`

#### Scenario: Applying after the commit
- **WHEN** a plugin registers an apply step for its section and a generation carries the section and a new component
- **THEN** the apply step runs after the component is live with its defaults encodable, and before the generation's first startup

#### Scenario: Applied with declarations
- **WHEN** a generation carries both new components and a plugin section
- **THEN** the plugin receives its section in the same frame the components are registered, and can already look those components up

### Requirement: Host-invoked JS entries
A plugin SHALL be able to call a named JS entry registered by the current generation, passing a byte payload, from an exclusive system on the main thread. The call SHALL return the bytes the entry returned plus a command buffer applied like a system's. Entry calls SHALL follow the system error rules: the stack is mapped to TS, repeats are deduped, and on a throw the commands are discarded. No call SHALL be made into JS when the plugin makes none. This is a fifth seam operation next to load, run system, run startup and pump.

#### Scenario: A plugin with nothing to do
- **WHEN** a plugin's entry exists but the plugin does not call it in a frame
- **THEN** the backend records no call into JS for that entry that frame

#### Scenario: An entry throws
- **WHEN** an entry throws after queuing a spawn
- **THEN** the plugin gets an error with the TS location, and no entity is spawned

### Requirement: Plugin command kinds
A plugin SHALL be able to register a command opcode with a decoder. JS SHALL be able to append that plugin's ops to a system's `Commands` param in a system run or an entry call through the `bevy-ts` module. Plugin ops SHALL be applied in order with the built-in commands of the same buffer, with `&mut World`. An unregistered opcode SHALL fail that run's commands with an error naming it.

#### Scenario: Mixed ops keep order
- **WHEN** a system queues `spawn`, then a plugin op that targets the spawned entity, then `despawn` of another entity
- **THEN** Rust applies them in that order, and the plugin's decoder sees the spawned entity already existing

### Requirement: Component ids and layouts by name
A plugin SHALL be able to look up a live TS component's `ComponentId` and layout (size, align, fields with offsets and types) by its declared name, and SHALL be able to encode a map of field names to values into the component's native bytes, with defaults filled in and type errors naming the field.

#### Scenario: Encoding a partial value
- **WHEN** a plugin encodes `{ hp: 50 }` for `class Health extends Component { static path = "game::Health"; declare hp: f32; max: f32 = 100 }`
- **THEN** it gets 8 bytes holding 50.0 and 100.0

### Requirement: Reflected Rust components for plugins
A plugin SHALL be able to read the change ticks of any component by `ComponentId`, and to read and write a reflected Rust component (one with `ReflectComponent`, such as `Text` or `Node`) on an entity as JSON through Bevy reflection, by type path. This is the slow path for components without a native layout; TS systems still reject them as query data.

#### Scenario: Watching Text without a native layout
- **WHEN** a plugin checks the change tick of an entity's `Text` each frame and reads it as JSON only when the tick moved
- **THEN** it gets `Text`'s current value on the frames where it changed, with no call into JS

### Requirement: Restart notification
A restart (`bevy-ts/reload` with `restart: true`) SHALL notify every plugin that registered for it, after TS-spawned entities are despawned and before the generation is applied as if first.

#### Scenario: A plugin clears its state
- **WHEN** a restart happens
- **THEN** each registered plugin is notified once, between the despawn and the first-time apply
