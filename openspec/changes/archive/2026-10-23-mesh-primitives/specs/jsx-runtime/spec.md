## ADDED Requirements

### Requirement: `<mesh>` builds Bevy's mesh primitives
`<mesh shape>` SHALL build a `Mesh3d` from every Bevy 0.20 primitive that meshes to a surface: `cuboid`, `sphere`, `plane`, `cylinder`, `capsule3d`, `cone`, `conicalFrustum`, `torus`, `tetrahedron`, `triangle3d`, `circle`, `circularSector`, `circularSegment`, `ellipse`, `annulus`, `rectangle`, `rhombus`, `regularPolygon`, `triangle2d`, `capsule2d` and `convexPolygon`. A shape's props SHALL be its Bevy constructor's parameters in camelCase, plus its Bevy mesh builder's settings, and an unset prop SHALL take Bevy's default. Lengths SHALL be full lengths where the constructor takes them (`cuboid`, `rectangle` and `plane` take `size`) and half lengths where it takes those (`ellipse` takes `halfSize`). A 2D shape SHALL lie in the XY plane facing +Z; given `depth`, it SHALL be Bevy's extrusion of that shape, `depth` deep along Z, with `segments` along the depth. `color` SHALL stay a shorthand for `<material color>` on every shape. `cuboid`'s `size`, `sphere`'s `radius` and `plane`'s `size` SHALL keep their meaning and defaults.

`MeshProps` in `bevy-ts/jsx-runtime` SHALL be a union discriminated by `shape`, so a prop the shape does not take, or a value of the wrong type or length, is a `tsc` error.

At load, a prop the shape does not take, a vector of the wrong length, a size or radius that is not positive and finite, or a setting Bevy rejects or panics on (fewer than 3 sides or vertices, a resolution below 3, an icosphere over 79 subdivisions, a `convexPolygon` that is not convex, an `annulus` whose inner radius is not below its outer one) SHALL fail the load with an error naming the shape, the prop and the element's source location. It SHALL NOT panic.

Elements with equal shape and props SHALL share one mesh asset, as today.

#### Scenario: Every shape builds a valid mesh
- **WHEN** a headless app with no GPU spawns one `<mesh>` of each shape with its default props, and one extruded (`depth={1}`) of each 2D shape
- **THEN** each entity's mesh has a vertex count above zero, a normal per vertex, a triangle list whose winding agrees with its normals, and on every closed solid normals that point outward

#### Scenario: Builder settings reach Bevy
- **WHEN** a scene has `<mesh shape="cylinder" radius={0.5} height={2} resolution={8} />`
- **THEN** its mesh equals Bevy's `Cylinder::new(0.5, 2.0).mesh().resolution(8).build()`

#### Scenario: Existing shapes are unchanged
- **WHEN** a scene written before this change has `<mesh shape="cuboid" size={[1, 2, 3]} color="#c33" />`, `<mesh shape="sphere" radius={2} />` and `<mesh shape="plane" size={[4, 4]} />`
- **THEN** it type-checks and builds the same meshes and material as before

#### Scenario: A prop the shape does not take
- **WHEN** a scene writes `<mesh shape="sphere" sides={6} />`
- **THEN** `tsc` reports `sides` on the sphere, and loading fails with an error naming `sphere`, `sides` and the element's file and line

#### Scenario: A value Bevy would panic on
- **WHEN** a scene writes `<mesh shape="regularPolygon" sides={2} />`
- **THEN** loading fails with an error naming `regularPolygon` and `sides`, and the engine keeps running
