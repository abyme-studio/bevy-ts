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

### Requirement: SceneAlpha fades a glTF scene's meshes
An entity with `WorldAssetRoot` and `SceneAlpha { alpha }` SHALL, once its scene instance is ready, draw every mesh the instance spawned that holds a `MeshMaterial3d<StandardMaterial>` with a copy of that material owned by the root: one copy per distinct original material per root. A copy SHALL have the original's `base_color` alpha times `alpha` (clamped to 0..1). Below 1, a copy of an `Opaque`, `Mask` or `AlphaToCoverage` original SHALL be `Blend`; other modes SHALL be kept; at 1 the copy SHALL equal the original. Originals SHALL never be edited, so another root sharing the model's materials is not faded. A changed `alpha` SHALL reach every copy the frame it changes by editing the copies in place, creating no asset. A field binding (`<SceneAlpha alpha={() => k()} />`) SHALL drive it each frame with no remount. Removing `SceneAlpha` SHALL give each mesh its original material back and drop the copies. A scene spawned again (ready again) SHALL be faded again, and an original that reloads SHALL be copied again. Entities the instance did not spawn, such as TSX children of the root, and meshes drawing a script material, SHALL be left alone.

#### Scenario: A cluster fades out
- **WHEN** ten entities each have `WorldAssetRoot("models/house.glb#Scene0")` and `<SceneAlpha alpha={() => k()} />`, and `k` goes from 1 to 0 over 2 s
- **THEN** each frame every house mesh draws with its root's copy at alpha `k`, in `Blend`, and no `StandardMaterial` asset is added after the first frame

#### Scenario: Another instance of the same model
- **WHEN** a second root spawns the same model with no `SceneAlpha`, while the first fades
- **THEN** the second root's meshes draw with the model's own materials, unchanged

#### Scenario: Removing it
- **WHEN** `SceneAlpha` is removed from a faded root
- **THEN** every mesh of its scene draws with its own material again, and the root's copies are dropped

#### Scenario: A hull tinted by a script material
- **WHEN** a faded root also has a script material that swapped its `hull` meshes (scene-material)
- **THEN** the `hull` meshes keep the script material, unfaded, and the other meshes fade; removing either component leaves each mesh with what the other still asks for
