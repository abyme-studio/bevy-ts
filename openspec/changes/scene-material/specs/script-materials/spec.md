## ADDED Requirements

### Requirement: A script material on a glTF scene root draws the scene's meshes
When an entity carries both `WorldAssetRoot` and `ScriptMaterial3d`, every mesh entity the scene instance spawned that holds a `MeshMaterial3d<StandardMaterial>` SHALL, once the instance is ready, draw with the root's `ScriptMaterial3d` instead, and its `StandardMaterial` handle SHALL be kept. A `<material kind={K} gltfMaterial="Name" />` (in Rust, `GltfMaterialFilter("Name")` on the root) SHALL limit this to meshes whose glTF material name (`GltfMaterialName`) is exactly `Name`. Entities not spawned by the scene instance, such as TSX children of the root, SHALL be left alone. A changed `ScriptMaterial3d` SHALL reach every swapped mesh the frame it changes; a changed filter SHALL restore meshes that stop matching and swap those that start. Removing `ScriptMaterial3d` SHALL restore each swapped mesh's own `StandardMaterial`. A scene spawned again (ready again) SHALL be swapped again. `material()` SHALL refuse a param or texture named `kind` or `gltfMaterial`, which `<material>` takes itself. `gltfMaterial` bound to a function SHALL fail as any bound `<material>` prop does.

#### Scenario: Tinting the hull of a ship
- **WHEN** an entity has `WorldAssetRoot("models/ship.gltf#Scene0")` and `<material kind={MaskTint} gltfMaterial="hull" colors={[...]} />`, and the ship's meshes use materials `hull` and `trim`
- **THEN** once the scene is ready, the `hull` meshes hold the `MaskTint` instance and the `trim` meshes keep their own `StandardMaterial`

#### Scenario: A new colour
- **WHEN** the same element's `colors` change
- **THEN** the `hull` meshes hold the new instance, with no scene respawn

#### Scenario: Removing the material
- **WHEN** the `<material>` element is removed
- **THEN** every `hull` mesh draws with its own `StandardMaterial` again

#### Scenario: A reserved name
- **WHEN** `material("game::M", { params: { gltfMaterial: f32 }, fragment })` is declared
- **THEN** the load fails at the `material()` call, naming `gltfMaterial`
