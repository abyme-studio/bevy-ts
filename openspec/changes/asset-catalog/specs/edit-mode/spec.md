## MODIFIED Requirements

### Requirement: Picking selects what the source placed
In edit mode game pointer input SHALL be off (`PointerInputSettings` mouse disabled) and no picking pointer SHALL exist, so game `Pointer<…>` observers and TSX handlers do not fire. A click with no drag SHALL pick by a ray cast from the editor camera against meshes and against light and camera icons, and select, from the topmost hit, the nearest ancestor that is a prefab instance root (an entity whose TSX source chain has a call site, or a `.bsn` level instance); ⌥-click SHALL select the hit entity itself; ⇧-click SHALL add to or remove from the selection; a click on nothing SHALL clear it. `bevy-ts/viewport_pick` (`x`, `y`) SHALL answer every hit at that point, nearest first, with entity, depth, its world `point` `[x, y, z]` and the entity the click would select, and a `ground` `[x, y, z]`, where the ray meets the plane y = 0 in front of the camera, or null when it does not, without changing the selection.

#### Scenario: Clicking an enemy's eye
- **WHEN** the operator clicks enemy `b`'s `eye` child mesh
- **THEN** enemy `b` (the `<Enemy key="b">` instance root) is selected; with ⌥ held, the `eye` entity is

#### Scenario: A button in the HUD during edit
- **WHEN** the operator clicks the HUD's Spawn button in edit mode with `ui: true`
- **THEN** the button's `<button>` entity is selected and no enemy is spawned

#### Scenario: Where a dropped model lands
- **WHEN** the studio calls `viewport_pick` at a point over empty floor below the horizon, then at a point on enemy `b`
- **THEN** the first answers no hits and a `ground` with y = 0; the second's first hit has a `point` on `b`'s surface

#### Scenario: Pointing at the sky
- **WHEN** the studio calls `viewport_pick` at a point above the horizon with nothing there
- **THEN** it answers no hits and `ground: null`
