## ADDED Requirements

### Requirement: A standard material takes alpha and an alpha mode
`<material>` without `kind` SHALL build a `StandardMaterial` from `color`, `metallic`, `roughness`, `emissive`, `unlit`, `alpha` and `alphaMode`. `alpha` SHALL be a number from 0 to 1 that replaces the alpha of `base_color`; a colour written with alpha (`"#ff000080"`) SHALL keep its own alpha when `alpha` is unset. `alphaMode` SHALL take the values a script material's `alphaMode` takes: `"opaque"`, `"blend"`, `"premultiplied"`, `"add"`, `"multiply"`, `"alphaToCoverage"` or `{ mask: cutoff }`. Unset, it SHALL be `Blend` when the final alpha is below 1 and `Opaque` otherwise, as Bevy's `StandardMaterial::from(Color)`. An `alpha` outside 0 to 1, or an unknown `alphaMode`, SHALL fail the load at the element. Equal props SHALL share one material, as every `<material>` does.

#### Scenario: A half see-through cube
- **WHEN** an entity has `<mesh shape="cuboid" />` and `<material color="#ff0000" alpha={0.5} />`
- **THEN** its `StandardMaterial` has `base_color` alpha 0.5 and `AlphaMode::Blend`, and the cube is drawn blended over what is behind it

#### Scenario: An explicit mode wins
- **WHEN** a material has `alpha={0.5}` and `alphaMode={{ mask: 0.3 }}`
- **THEN** its mode is `AlphaMode::Mask(0.3)`

#### Scenario: A bad alpha
- **WHEN** a material has `alpha={2}`
- **THEN** the load fails at that element, saying alpha takes 0 to 1

### Requirement: MaterialAlpha fades an entity's materials
An entity with `MaterialAlpha { alpha }` SHALL fade its targets: the entity itself when it holds a `MeshMaterial3d<StandardMaterial>`, and, when it has `WorldAssetRoot`, every mesh its scene instance spawned that holds one, once the instance is ready. Each target SHALL draw with a copy of its material owned by the entity: one copy per distinct original material per entity. A copy SHALL have the original's `base_color` alpha times `alpha` (clamped to 0..1). Below 1, a copy of an `Opaque`, `Mask` or `AlphaToCoverage` original SHALL be `Blend`; other modes SHALL be kept; at 1 the copy SHALL equal the original. Originals SHALL never be edited, so another entity sharing the material is not faded. A changed `alpha` SHALL reach every copy the frame it changes by editing the copies in place, creating no asset. A field binding (`<MaterialAlpha alpha={() => k()} />`) SHALL drive it each frame with no remount. A target given another `StandardMaterial` while faded SHALL be faded again from that material. Removing `MaterialAlpha` SHALL give each target its original material back and drop the copies. A scene spawned again (ready again) SHALL be faded again, and an original that reloads SHALL be copied again. TSX children of the entity, entities its scene did not spawn, and meshes drawing a script material SHALL be left alone.

#### Scenario: A shape fades out
- **WHEN** an entity has `<mesh shape="cuboid" />`, `<material color="#ff0000" />` and `<MaterialAlpha alpha={() => k()} />`, and `k` goes from 1 to 0 over 2 s
- **THEN** each frame the cube draws with the entity's copy at alpha `k`, in `Blend`, and no `StandardMaterial` asset is added after the first frame

#### Scenario: Another shape with the same material
- **WHEN** a second entity has the same `<mesh>` and `<material color="#ff0000" />` and no `MaterialAlpha`, while the first fades
- **THEN** the second cube draws with the shared material, opaque red

#### Scenario: A shape's material changes while faded
- **WHEN** a TS save changes the faded cube's `<material color>` to `"#0000ff"`
- **THEN** the cube draws blue at alpha `k`, and removing `MaterialAlpha` leaves it opaque blue

#### Scenario: A cluster fades out
- **WHEN** ten entities each have `WorldAssetRoot("models/house.glb#Scene0")` and `<MaterialAlpha alpha={() => k()} />`, and `k` goes from 1 to 0 over 2 s
- **THEN** each frame every house mesh draws with its root's copy at alpha `k`, in `Blend`, and no `StandardMaterial` asset is added after the first frame

#### Scenario: Another instance of the same model
- **WHEN** a second root spawns the same model with no `MaterialAlpha`, while the first fades
- **THEN** the second root's meshes draw with the model's own materials, unchanged

#### Scenario: Removing it
- **WHEN** `MaterialAlpha` is removed from a faded shape or root
- **THEN** every target draws with its own material again, and the entity's copies are dropped

#### Scenario: A hull tinted by a script material
- **WHEN** a faded root also has a script material that swapped its `hull` meshes (scene-material)
- **THEN** the `hull` meshes keep the script material, unfaded, and the other meshes fade; removing either component leaves each mesh with what the other still asks for
