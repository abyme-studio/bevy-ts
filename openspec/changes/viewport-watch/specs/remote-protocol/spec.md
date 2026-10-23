## ADDED Requirements

### Requirement: Each subscriber of a watch is served on its own
Every `+watch` method SHALL serve each open request on its own, even when several have the same params: each SHALL get the method's opening message when it opens, then every message owed to it after. A request that was not served for an update SHALL count as closed, so the same params opened again get the opening message again. When a request closes, another with the same params MAY be sent a message again, and SHALL NOT miss one; a request that opens in the update another with the same params closes MAY get no opening message. A Stop SHALL NOT restart open watches.

#### Scenario: A second studio window opens the same watch
- **WHEN** a client has `bevy-ts/viewport_tools+watch` open and a second request with the same params opens later
- **THEN** the second is sent the tools on open, and on the next change both are sent it

#### Scenario: A second window opens the asset watch as a file lands
- **WHEN** a client has `bevy-ts/assets+watch` open, a file is added, and a second request with the same params opens before the first is sent the change
- **THEN** the second's list has the file, and the first is still sent it as `added`

## MODIFIED Requirements

### Requirement: Entity and scene change events
`bevy-ts/entities` SHALL return a snapshot of every entity but bookkeeping (observers, resource entities, `ScriptHidden` entities, and the engine's windows and picking pointers): each entity's id, `Name` if any, parent, and the names of all its components (TS and Rust). `bevy-ts/entities+watch` SHALL first send that snapshot with a sequence number, then at most one message per frame with what changed that frame: spawned (with the snapshot fields), despawned, reparented, renamed, components added and components removed. Each message SHALL carry the next sequence number; a client that sees a gap SHALL resubscribe. Field values are not part of this stream.

#### Scenario: Spawning from TS shows in the scene tree
- **WHEN** a TS system spawns an entity named `bullet` as a child of the cube
- **THEN** the next message lists it under `spawned` with its parent and components, in the same frame's message

#### Scenario: A quiet frame
- **WHEN** nothing structural changes in a frame
- **THEN** no message is sent for that frame

#### Scenario: The engine's own entities after a Stop
- **WHEN** a viewport engine stops a played game and a client calls `bevy-ts/entities`
- **THEN** neither the virtual window's entity nor any `PointerId` entity is listed, nor in the `bevy-ts/entities+watch` snapshot
