## ADDED Requirements

### Requirement: A scene root's script material takes each mesh's glTF channels
A `<material kind>` on a glTF scene root SHALL take `gltf`, a map from the kind's texture names and `color` param names to a channel of the swapped mesh's own `StandardMaterial`: `baseColor` and `emissive` for a `color` param; `baseColorTexture`, `emissiveTexture`, `normalMapTexture`, `metallicRoughnessTexture` and `occlusionTexture` for a texture. Each swapped mesh SHALL draw with an instance of the root's kind whose mapped params and textures hold that mesh's own values, and whose other params and textures hold the element's props. A name both mapped and set as a prop SHALL take the prop. A mapped texture the mesh's material lacks SHALL bind the white fallback. Meshes whose source `StandardMaterial` is the same asset SHALL share one instance. A changed prop or `gltf` SHALL re-derive every instance in the same frame, and removing the material SHALL restore each mesh's own, as for a root material without `gltf`. A name in `gltf` that is not a texture or `color` param of the kind, or a channel of the wrong kind for it, SHALL fail the load at the element. `material()` SHALL refuse a param or texture named `gltf`.

#### Scenario: A toon kind on a two-material scene
- **WHEN** a scene root carries `<material kind={Toon} gltf={{ albedo: "baseColorTexture", tint: "baseColor" }} shade="#5a6aa0" />` and its scene has two glTF materials with different textures
- **THEN** each mesh draws with its own material's texture and base colour under one `game::Toon` kind, and the meshes of each glTF material share one instance

#### Scenario: A prop wins over the mesh
- **WHEN** `game::MaskTint` is set with `gltf={{ base: "baseColorTexture" }}` and an explicit `mask="textures/hull-mask.png"`
- **THEN** every mesh takes its own `base` and the one `mask`

#### Scenario: A wrong channel
- **WHEN** `gltf={{ tint: "normalMapTexture" }}` names a `color` param
- **THEN** the load fails at the element, naming `tint` and the colour channels it may take
