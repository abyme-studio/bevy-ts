## MODIFIED Requirements

### Requirement: Material instances are shared assets keyed by kind and props
A TSX `<material kind={...} />` SHALL resolve to a `ScriptMaterial` asset keyed by its kind and its props, shared by every element with equal props, as `<material>`'s `StandardMaterial` cache shares by props. The entity SHALL hold `ScriptMaterial3d(handle)`. A keyed instance SHALL NOT be written in place: new props SHALL take another key. Each `PostUpdate`, after the TSX reconcile, a keyed instance that no entity holds SHALL be dropped from the cache, so its asset and binding are freed; a key removed and added again within one frame SHALL keep its asset. A param or texture bound to a function (`prop={() => ...}`) SHALL fail the load at the element with `<material> props cannot be bound yet; bind a component field or a UI prop`, as on `<mesh>` and `<material>`. Props SHALL be checked against the materials section of the element's own generation, whatever the order of the manifest's sections.

#### Scenario: Two equal instances
- **WHEN** two meshes carry `<material kind={Glow} color="#f80" pulse={2} />`
- **THEN** both hold the same `ScriptMaterial3d` handle, and a third with `pulse={3}` holds another

#### Scenario: A bound param
- **WHEN** an element writes `<material kind={Glow} pulse={() => t.value} />` and `pulse` is a param
- **THEN** the load fails at that element with `<material> props cannot be bound yet; bind a component field or a UI prop`

#### Scenario: Props that change every frame
- **WHEN** an element's `pulse` follows state that changes every frame for 120 frames
- **THEN** `Assets<ScriptMaterial>` holds at most two of the kind's instances throughout

#### Scenario: One of two sharers removed
- **WHEN** two elements share an instance and one is removed
- **THEN** the other keeps drawing with the same handle

## ADDED Requirements

### Requirement: Owned instances are written in place
The kind `material()` returns SHALL have `add(props)`, which makes a `ScriptMaterial` asset of that kind outside the keyed cache and returns reflect-assets' `AssetRef`. `bevy-ts-material` SHALL register `ScriptMaterial` with reflect-assets' `app.ts_hand_map::<ScriptMaterial>(read, write)`: `assets.get(ref)` SHALL return `{ kind, ...params, ...textures }`, and `assets.set(ref, patch)` SHALL write the patch over that at the top level and keep the asset's `AssetId`, so every entity holding it draws the new values. A patch that changes nothing SHALL mark nothing changed. A patch naming a param or texture the kind lacks, or a different `kind`, SHALL fail with the kind's names. `<material asset={ref} />` SHALL put the ref's asset on the entity, as `ScriptMaterial3d`, and take `gltfMaterial` as `<material kind>` does; any other prop beside `asset`, a ref of another asset type, or a stale ref SHALL fail at the element. A changed params schema SHALL re-encode owned instances by param name, as it does keyed ones. `material()` SHALL refuse a param, texture or instance param named `asset`.

#### Scenario: Writing an owned glow
- **WHEN** a script makes `const g = Glow.add({ color: "#f00" })`, a mesh carries `<material asset={g} />`, and a later run calls `assets.set(g, { color: "#00f" })`
- **THEN** the mesh holds the same `AssetId` and the next rendered frame draws it blue

#### Scenario: Writing every frame
- **WHEN** a system calls `assets.set(g, { pulse: t })` with a new `t` for 60 frames
- **THEN** the number of render pipelines is the same after as before, and `Assets<ScriptMaterial>` holds no new asset

#### Scenario: Reading one back
- **WHEN** `assets.get(g)` runs after `assets.set(g, { pulse: 3 })`
- **THEN** it returns `{ kind: "game::Glow", color: ..., pulse: 3 }`

#### Scenario: A ref of another type
- **WHEN** an element writes `<material asset={StandardMaterialRef} />`
- **THEN** the load fails at that element, naming `ScriptMaterial`

### Requirement: Instance params live on the entity
`material(name, { instance, ... })` SHALL take an `instance` schema of the same types as `params`, laid out by the same rules, at most 256 bytes; a larger one, or an instance name equal to a param, texture, sampler, `instance`, `instances` or the generated `<Name>Instance` struct, SHALL fail at the `material()` call. Instance props on `<material kind>` SHALL go to a `ScriptInstance` component on the entity, never to the asset or its key, and SHALL be packed by Bevy's `GpuComponentArrayBuffer` into one storage buffer indexed by the entity's `MeshTag`. The kind's WGSL SHALL receive an appended `<Name>Instance` struct, a read-only storage binding after its textures and `fn instance(i: u32) -> <Name>Instance`, read as `instance(in.instance_index)`. Unset instance values SHALL be zero, colours white. Only entities of a kind that declares `instance` SHALL get a `ScriptInstance` or a `MeshTag`. An entity that already has a `MeshTag` bevy-ts did not set SHALL keep it, get no `ScriptInstance`, and report an error naming the entity and the element. Instance props, and only they, SHALL accept a bound function, which writes the entity's `ScriptInstance` the frame it changes. On a glTF scene root, each swapped mesh SHALL take the root's `ScriptInstance` and follow its changes, and lose it when restored.

#### Scenario: Two spheres, one hovered
- **WHEN** two spheres carry `<material kind={Glow} color="#f80" hover={0} />` and `hover={1}`, and `hover` is an instance param
- **THEN** both hold one `ScriptMaterial3d` handle and draw different colours

#### Scenario: A bound hover
- **WHEN** an element writes `hover={() => h()}` and `h` changes every frame
- **THEN** its pixels change frame by frame and `Assets<ScriptMaterial>` gains no asset

#### Scenario: A user tag
- **WHEN** an entity with `MeshTag(7)` from Rust carries a kind with instance params
- **THEN** it keeps `MeshTag(7)`, has no `ScriptInstance`, and the error names the entity and the element

#### Scenario: A kind with no instance params
- **WHEN** an entity carries `<material kind={Glow} />` and `Glow` declares no `instance`
- **THEN** it has neither `ScriptInstance` nor `MeshTag`
