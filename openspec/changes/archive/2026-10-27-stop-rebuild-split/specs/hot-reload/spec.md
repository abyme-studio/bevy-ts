## ADDED Requirements

### Requirement: A restart's engine is booted ahead
Under `bevy-ts dev`, after the first generation, a restart, or a Stop, bevy-ts SHALL boot the next JS engine instance in a later frame and hold it unused. A restart or a Stop SHALL take that instance instead of booting one, and SHALL drop the old instance in a frame after it answers. The held instance SHALL run no game code before it is taken, so a restart still applies its generation in a fresh instance. When no instance is held (one is still booting, or boot failed), a restart SHALL boot one as before. The debugger SHALL follow the instance taken, as it follows a restart today.

#### Scenario: Nothing carries over
- **WHEN** a generation sets `globalThis.count = 7`, and a client calls `bevy-ts/reload` with `restart: true`
- **THEN** the restarted generation reads `globalThis.count` as undefined

#### Scenario: Two Stops in a row
- **WHEN** a client sets `play`, `edit`, `play` and `edit` with no frame between them
- **THEN** both Stops rebuild in a fresh instance; the second boots its own because the spare was not ready
