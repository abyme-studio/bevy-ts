## MODIFIED Requirements

### Requirement: MaterialAlpha fades an entity's materials
An entity with `MaterialAlpha { alpha }` SHALL fade its targets: the entity itself when it holds a `MeshMaterial3d<StandardMaterial>`, and, when it has `WorldAssetRoot`, every mesh its scene instance spawned that holds one, once the instance is ready. Each target SHALL draw with a `FadeMaterial` copy (`ExtendedMaterial<StandardMaterial, Fade>`) owned by the entity: one copy per distinct original material per entity, its base a clone of the original. A copy of an `Opaque`, `Mask` or `AlphaToCoverage` original SHALL keep the original's mode and `base_color`, and SHALL discard each pixel whose threshold on a 4×4 ordered (Bayer) screen-space map is above `alpha` (clamped to 0..1), so about `alpha` of the pixels draw, in steps of 1/16, with depth written. A copy of a `Blend`, `Premultiplied`, `Add` or `Multiply` original SHALL keep its mode, SHALL have the original's `base_color` alpha times `alpha`, and SHALL NOT discard in the main pass. At 1 a copy SHALL draw as the original; at 0 it SHALL draw nothing. Originals SHALL never be edited, so another entity sharing the material is not faded. A changed `alpha` SHALL reach every copy the frame it changes by editing the copies in place, creating no asset; the sync SHALL run after Bevy's `AnimationSystems`, so an `alpha` an animation writes reaches the copies that frame. A field binding (`<MaterialAlpha alpha={() => k()} />`) SHALL drive it each frame with no remount. A target given another `StandardMaterial` while faded SHALL be faded again from that material. Removing `MaterialAlpha` SHALL give each target its original `MeshMaterial3d<StandardMaterial>` back, remove its `MeshMaterial3d<FadeMaterial>`, and drop the copies. A scene spawned again (ready again) SHALL be faded again, and an original that reloads SHALL be copied again. TSX children of the entity, entities its scene did not spawn, and meshes drawing a script material SHALL be left alone. `alpha.rs` SHALL be the one place that knows a faded mesh's original; other code (scene-material) SHALL ask it.

#### Scenario: A shape fades out
- **WHEN** an entity has `<mesh shape="cuboid" />`, `<material color="#ff0000" />` and `<MaterialAlpha alpha={() => k()} />`, and `k` goes from 1 to 0 over 2 s
- **THEN** each frame the cube draws with the entity's `FadeMaterial` copy at alpha `k`, its mode `Opaque`, about `k` of its pixels opaque red and the rest showing what is behind, and no material asset is added after the first frame

#### Scenario: A model fades as one solid
- **WHEN** a red cube holds a smaller black cube inside it, both under one faded root at `alpha` 0.5, drawn unlit over white with `Msaa::Off`
- **THEN** no pixel in the red cube's area is near black, and the fraction of red pixels there is 0.5 ± 1/16

#### Scenario: An already transparent material fades
- **WHEN** a faded entity's original is `<material color="#ff0000" alpha={0.5} />` (`Blend`) and `alpha` is 0.5
- **THEN** its copy is `Blend` with `base_color` alpha 0.25, and its main pass discards nothing

#### Scenario: Another shape with the same material
- **WHEN** a second entity has the same `<mesh>` and `<material color="#ff0000" />` and no `MaterialAlpha`, while the first fades
- **THEN** the second cube draws with the shared material, opaque red

#### Scenario: A shape's material changes while faded
- **WHEN** a TS save changes the faded cube's `<material color>` to `"#0000ff"`
- **THEN** the cube draws blue at alpha `k`, and removing `MaterialAlpha` leaves it opaque blue

#### Scenario: A cluster fades out
- **WHEN** ten entities each have `WorldAssetRoot("models/house.glb#Scene0")` and `<MaterialAlpha alpha={() => k()} />`, and `k` goes from 1 to 0 over 2 s
- **THEN** each frame every house mesh draws with its root's `FadeMaterial` copy at alpha `k`, and no material asset is added after the first frame

#### Scenario: Another instance of the same model
- **WHEN** a second root spawns the same model with no `MaterialAlpha`, while the first fades
- **THEN** the second root's meshes draw with the model's own materials, unchanged

#### Scenario: Removing it
- **WHEN** `MaterialAlpha` is removed from a faded shape or root
- **THEN** every target holds its own `MeshMaterial3d<StandardMaterial>` again and no `MeshMaterial3d<FadeMaterial>`, and the entity's copies are dropped

#### Scenario: A hull tinted by a script material
- **WHEN** a faded root also has a script material that swapped its `hull` meshes (scene-material)
- **THEN** the `hull` meshes keep the script material, unfaded, and the other meshes fade; removing either component leaves each mesh with what the other still asks for

## ADDED Requirements

### Requirement: A faded target's shadow follows its alpha
A target drawing a `FadeMaterial` copy SHALL cast a shadow dithered by the same 4×4 map at the same `alpha`, whatever the original's alpha mode: at 1 the original's full shadow, at 0 none, between them a shadow that lightens as `alpha` falls. The dither SHALL run in the copy's prepass fragment shader, which the shadow pass uses. `MaterialAlpha` SHALL NOT add, remove or read `NotShadowCaster`; one already on a target SHALL keep it from casting at any `alpha`.

#### Scenario: Three cubes over a lit ground
- **WHEN** three cubes stand above a white ground plane under a directional light with shadows, at `MaterialAlpha` 1, 0.5 and 0
- **THEN** the ground under the 0.5 cube is lighter than under the 1 cube and darker than under the 0 cube, and the ground under the 0 cube equals unshadowed ground

#### Scenario: A faded scene's shadow
- **WHEN** a `ship.gltf` root at `MaterialAlpha` 0 stands over the lit ground
- **THEN** the ground under it equals unshadowed ground

#### Scenario: An authored NotShadowCaster stays
- **WHEN** a cube with `NotShadowCaster` gets `MaterialAlpha` 1, and the component is then removed
- **THEN** it casts no shadow throughout, and still holds `NotShadowCaster` after
