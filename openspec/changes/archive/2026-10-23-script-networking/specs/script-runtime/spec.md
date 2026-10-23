## MODIFIED Requirements

### Requirement: Engine-neutral seam
The core crate (`bevy-ts`) SHALL depend on no JS engine. It SHALL drive a backend only through one trait whose operations are: load a set of compiled modules as a new generation, with the environment `process.env` shows, and return its declarations and manifest sections, run one declared system over a frame of column buffers and get back a command buffer, run a startup function, call a named entry with a byte payload and get back bytes and a command buffer (see plugin-api), deliver one frame's socket events and the current real time and get back the network requests scripts made and the next timer deadline (see script-networking), and pump pending engine work (microtasks, inspector messages). Network requests and events SHALL cross the seam as plain data tagged with a connection id and a generation; the backend SHALL hold no socket. The V8 backend (`bevy-ts-v8`, `deno_core` 0.412) SHALL be the only backend in this change.

#### Scenario: The core builds without V8
- **WHEN** `bevy-ts` is built with no backend crate
- **THEN** it compiles, and its tests run against an in-process fake backend that implements the trait

#### Scenario: A backend sees buffers, not the world
- **WHEN** a backend runs a system
- **THEN** it receives only pointers, lengths, layouts and a command arena, and has no access to `World`

#### Scenario: A backend sees network data, not sockets
- **WHEN** a script sends on a `WebSocket`
- **THEN** the backend hands core a send request with the connection id, generation and bytes, and core's transport writes it
