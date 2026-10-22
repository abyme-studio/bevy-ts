## MODIFIED Requirements

### Requirement: Native spawn recipes
Rust SHALL register named recipes callable from `spawnNative` for things TS cannot yet build: in this change `camera3d`, `directionalLight` and `cuboid` (a mesh plus a `StandardMaterial` with a colour). Their params cross as JSON; this path is for rare spawns, not per-frame work. `camera3d` SHALL take an optional `projection` in the shape `<Projection value>` takes, written by the same reflected-value writer. A recipe's colour SHALL take every form the colour parser takes.

#### Scenario: A cube from TS
- **WHEN** a `Startup` system calls `commands.spawnNative("cuboid", { size: [1, 1, 1], color: "#f80" })` and then inserts `Mover` on the returned entity
- **THEN** an orange cube is visible and carries `game::Mover`

#### Scenario: An orthographic camera from TS
- **WHEN** a `Startup` system calls `commands.spawnNative("camera3d", { position: [10, 10, 10], lookAt: [0, 0, 0], projection: { Orthographic: { scaling_mode: { FixedVertical: { viewport_height: 12 } } } } })`
- **THEN** the camera's `Projection` is `Orthographic` with `FixedVertical { viewport_height: 12 }` and every other field at its default

#### Scenario: A bad projection
- **WHEN** `projection` is `{ Isometric: {} }`
- **THEN** the spawn fails with an error naming `Projection` and its variants `Perspective`, `Orthographic` and `Custom`
