## ADDED Requirements

### Requirement: `<mesh>` builds a mesh from vertex data
`<mesh>` with `positions` and no `shape` SHALL build a `Mesh3d` from vertex data, as `Mesh.of` does. Its props SHALL map onto Bevy's: `positions` (`Vec3[]`, `Mesh::ATTRIBUTE_POSITION`, required), `normals` (`Vec3[]`), `uvs` (`Vec2[]`, `ATTRIBUTE_UV_0`), `uvs1` (`Vec2[]`, `ATTRIBUTE_UV_1`), `tangents` (`Vec4[]`), `colors` (`Vec4[]`, linear RGBA, `ATTRIBUTE_COLOR`), `jointIndices` (`[u16, u16, u16, u16][]`), `jointWeights` (`Vec4[]`), `indices` (`number[]` as `Indices::U32`, or `{ U16: number[] }` / `{ U32: number[] }`), `topology` (a `PrimitiveTopology` variant name, default `"TriangleList"`), `assetUsage` (default both worlds). `normals="computed"`, `"flat"`, `"smooth"` or `"areaWeighted"` SHALL call Bevy's `compute_normals`, `compute_flat_normals`, `compute_smooth_normals` or `compute_area_weighted_normals`; `tangents="generated"` SHALL call `generate_tangents` after the normals. `color` SHALL stay the `<material color>` shorthand.

`MeshProps` SHALL gain this member in its union, so `tsc` rejects `shape` with `positions`, and a vertex prop on a shape.

At load, an attribute whose length differs from `positions`', an index out of range, a `topology` Bevy does not have, or a computation Bevy refuses SHALL fail with an error naming the prop and the element's source location, and SHALL NOT panic. Elements with equal props SHALL share one mesh asset. Vertex data in TSX is for meshes that change with props; a mesh that changes each frame SHALL go through `ResMut<Assets<Mesh>>`.

#### Scenario: A quad from props
- **WHEN** a scene has `<mesh positions={[[0,0,0],[1,0,0],[1,1,0],[0,1,0]]} uvs={[[0,0],[1,0],[1,1],[0,1]]} indices={[0,1,2,0,2,3]} normals="computed" />`
- **THEN** its mesh equals Bevy's `Mesh::new(TriangleList, default)` with the same inserts, `Indices::U32` and `compute_normals()`

#### Scenario: Mismatched lengths fail at load
- **WHEN** a scene gives four `positions` and three `uvs`
- **THEN** loading fails naming `uvs`, both lengths and the element's file and line, and the engine keeps running

#### Scenario: Equal vertex props share one mesh
- **WHEN** two elements have the same vertex props
- **THEN** both entities hold the same `Handle<Mesh>`
