## MODIFIED Requirements

### Requirement: A fixed-step shot draws ties in a stable order

Under `FixedStep`, every entity with an `Aabb` SHALL get a 64-bit key: a `FixedHasher` hash of its
whole `GlobalTransform` affine and its local `Aabb`, as f32 bits, the asset path of its mesh or
sprite image when it has one, and a hash of the reflected fields of each `MeshMaterial3d` or
`MeshMaterial2d` asset it draws with. A key SHALL be computed again when any of those components
change. The render world SHALL hold the keys as `StableBinOrder(MainEntityHashMap<u64>)`.

While `StableBinOrder` is in the render world:

- `sort_binned_render_phase` SHALL sort each bin's entities by key, then `MainEntity`, in
  batchable, unbatchable and non-mesh bins alike. It SHALL order `batchable_meshes`,
  `unbatchable_meshes` and `non_mesh_items` by each bin's first key, then by the bin's own key. An
  entity with no key SHALL sort after every keyed one. An unbatchable bin's buffer indices SHALL
  follow its sorted order.
- `sort_phase_system` SHALL order a sorted phase's items by key before `SortedPhaseItem::sort`, so
  items with one sort key keep the key order.

So which of two items at one depth is drawn last SHALL NOT depend on pipeline ids, mesh or material
`AssetId`s, entity ids, or the order assets finished loading. Without `StableBinOrder`, phases SHALL
sort as in Bevy 0.20.

#### Scenario: Mesh asset order
- **WHEN** two overlapping quads in one plane, one red and one blue, are shot once with the red mesh added to `Assets<Mesh>` first and once with the blue first
- **THEN** the two PNGs have identical pixels

#### Scenario: glTF load order
- **WHEN** the two quads are two glTFs, shot once with the red file's read released first and once with the blue first
- **THEN** the two PNGs have identical pixels

#### Scenario: A glTF race
- **WHEN** the two glTF quads are shot 10 times with both reads released at once
- **THEN** the 10 PNGs have identical pixels

#### Scenario: A town-like scene
- **WHEN** a scene of three tree glTFs sharing one alpha-mask leaf material, their foliage overlapping at one depth, plus transparent sprites at one z, is shot 10 times with the reads released at once and 30 ms of extra work each frame on half of them
- **THEN** the 10 PNGs have identical pixels

#### Scenario: A bin's order without the fixed step
- **WHEN** `sort_binned_render_phase` runs with no `StableBinOrder`
- **THEN** bins and their batchable entities keep Bevy 0.20's order, and unbatchable and non-mesh entities draw in the order they were queued

#### Scenario: Unbatchable bins
- **WHEN** `sort_binned_render_phase` runs with `StableBinOrder` on two unbatchable bins whose entities were queued out of key order
- **THEN** the bins come in order of their first key and each bin's entities in key order

#### Scenario: A half turn
- **WHEN** one slab mesh, red in front and blue behind, is shot twice at one place, the second turned exactly half a turn about y, once spawned in each order
- **THEN** the two PNGs have identical pixels

#### Scenario: Two materials on one mesh
- **WHEN** one quad mesh is shot twice at one place, in a red material and a blue one, once spawned in each order
- **THEN** the two PNGs have identical pixels
