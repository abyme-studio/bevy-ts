# jsx-runtime Specification

## Purpose
TSX describes entities. The `bevy-ts/jsx-runtime` module, with no React, turns entity, component, prefab and UI intrinsic elements into a plain-data entity description, typed per field as `initial*` or controlled, with explicit or type-scoped keys and, in dev, the source location of every element.

## Requirements

### Requirement: A custom JSX runtime with no React
The `bevy-ts` package SHALL provide `bevy-ts/jsx-runtime` (`jsx`, `jsxs`, `Fragment`) and `bevy-ts/jsx-dev-runtime` (`jsxDEV`, `Fragment`) for TypeScript's automatic runtime with `jsxImportSource: "bevy-ts"`. It SHALL NOT depend on React or any npm package, and SHALL ship the `JSX` namespace types so `tsc` checks elements and props.

#### Scenario: A project with no React installed
- **WHEN** a project with `"jsx": "react-jsx"` and `"jsxImportSource": "bevy-ts"` in its tsconfig and no `react` package compiles `game.tsx`
- **THEN** `tsc` reports no errors and `bevy-ts dev` loads it

### Requirement: Elements describe, they do not execute
Calling the JSX runtime SHALL return a plain-data description node (`type`, `key`, `props`, `children`, `source`) and SHALL NOT spawn, insert or call into Rust. Function-valued props and children SHALL be kept as bindings or handlers to be replaced by ids when the description is sent.

#### Scenario: Building an element in a unit test
- **WHEN** a test calls `<Enemy at={[1, 0, 0]} />` with no engine running
- **THEN** it gets a description tree whose entity has a `Transform` element with `translation` `[1, 0, 0]`

### Requirement: Entity, component, prefab and intrinsic elements
`<entity>` SHALL describe one entity; its children SHALL be component elements and child entities in any order. A component element's tag SHALL be a component class or a Rust component typed by bevy-ts (`Transform`, `Name`), and its props SHALL be that component's fields. A Rust component that is a tuple struct of one field SHALL take that field as the prop `value`, as `Name` does (`<WorldAssetRoot value="models/Fox.glb#Scene0" />`); a `Handle<T>` field, there or in a named field, SHALL take an asset path. A prefab SHALL be either a plain TS function from props to one element, called when its element is created (usable in TSX only, like a Rust scene function), or a scene component, a class extending `SceneComponent` with `static scene(props)`, whose element takes its fields and its scene's props (`<Enemy team={1} hp={250} />`), adds the component to the scene's root, and is placeable from a level (levels). A scene component whose field and prop share a name SHALL fail the load. `<mesh>`, `<material>`, `<worldText>`, `<node>`, `<text>`, `<image>`, `<button>` and `<input>` SHALL be intrinsics; `<mesh>`, `<material>` and `<worldText>` SHALL be components of their enclosing entity. A fragment SHALL describe several siblings. A component element outside an `<entity>` or an intrinsic, or a component given twice on one entity, SHALL be an error naming the element's source location.

#### Scenario: Two Transforms on one entity
- **WHEN** an `<entity>` has two `<Transform>` children
- **THEN** the scene fails with an error naming the second element's file and line

#### Scenario: A glTF scene from TSX
- **WHEN** a scene describes `<entity><WorldAssetRoot value="models/Fox.glb#Scene0" /></entity>`
- **THEN** the entity carries `WorldAssetRoot` holding the handle loaded from `models/Fox.glb#Scene0`, and the fox's meshes spawn under it

#### Scenario: A one-field tuple struct given a field name
- **WHEN** a scene describes `<WorldAssetRoot path="models/Fox.glb#Scene0" />`
- **THEN** the scene fails with an error naming the element's file and line and saying `WorldAssetRoot` takes `value`

#### Scenario: An input is typed
- **WHEN** a project writes `<input initialValue="" onKeyboardInput={(e) => e.value.length} />`
- **THEN** `tsc` reports no errors, and `<input value="x" />` or `onKeyboardInput={(e) => e.foo}` is a type error

#### Scenario: worldText is a component of its entity
- **WHEN** a scene describes `<entity><Transform translation={[0, 2, 0]} /><worldText size={0.5}>Paris</worldText></entity>`
- **THEN** it gets one entity whose components are `Transform` and a `worldText` intrinsic with `text` `"Paris"` and `size` 0.5, and `<worldText>` outside an `<entity>` fails naming its file and line

### Requirement: initial and controlled props are typed per field
For each field `f` of a component, the element's props SHALL accept `f` (controlled) and `initialF` (initial, `f` with its first letter capitalised), typed from the `component()` declaration with no codegen. Giving both for one field SHALL be a type error and, if it reaches the runtime, an error naming the field.

#### Scenario: Both spellings for one field
- **WHEN** a prefab writes `<Health hp={10} initialHp={10} />`
- **THEN** `tsc` reports a conflict on `hp`, and loading the scene fails with an error naming `game::Health.hp`

### Requirement: Source location on every element in dev
Under `bevy-ts dev`, every description node SHALL carry its file (project-relative), line and column in TS source coordinates, taken from the development JSX transform. Each entity SHALL also carry the call-site locations of every prefab that produced it, innermost first. Release builds MAY omit source.

#### Scenario: An entity made inside a prefab
- **WHEN** `game.tsx` line 12 writes `<Enemy key="b" />` and `enemy.tsx` line 4 writes the `<entity>` it returns
- **THEN** that entity's source chain is `enemy.tsx:4`, then `game.tsx:12`

### Requirement: Keys are explicit or type-scoped, never positional in source
An element's key SHALL be its `key` prop if given; otherwise `type#n`, where `type` is the intrinsic name, component or prefab name and `n` counts the preceding siblings with that type. Source line and column SHALL NOT take part in a key. Sibling elements produced from an array without keys SHALL produce a dev warning naming the location. Two siblings with the same explicit key SHALL be an error.

#### Scenario: Adding a blank line
- **WHEN** a blank line is inserted above `<Enemy key="b" />` and the file reloads
- **THEN** enemy `b` keeps its entity id

#### Scenario: Duplicate keys
- **WHEN** two sibling `<Enemy key="a" />` elements are mounted
- **THEN** the scene fails with an error naming both locations

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
