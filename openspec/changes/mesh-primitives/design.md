## Context

`crates/bevy-ts-tsx/src/intrinsics.rs` has one `MeshProps` struct: `shape: String`, `size: Option<Vec<f32>>`, `radius: Option<f32>`, `color: Option<String>`, with `deny_unknown_fields`. `check()` matches on the shape string; `mesh()` builds `Cuboid`, `Plane3d` or `Sphere`; `cache_key()` formats the props so equal props share one `Handle<Mesh>` in `IntrinsicAssets`. `resolve.rs` parses it with `parse_props` and reports errors at the element's source location; `resolve.rs:325` and `reconcile.rs:585` read its `color`. The TS side is `MeshProps` in `packages/bevy-ts/jsx-runtime.d.ts`, one interface for all three shapes.

Bevy 0.20 moved the shape types to `bevy_shape`; their `Meshable` impls and builders are in `bevy_mesh::primitives`. Both are already compiled into `bevy-ts-tsx`.

## Goals / Non-Goals

**Goals:** every Bevy 0.20 primitive with a surface mesh, under Bevy's names, with its builder settings; typed per shape in TSX; checked at load with no panic; no break for existing scenes.

**Non-Goals:**
- Line shapes (`Segment2d`, `Segment3d`, `Polyline2d`, `Polyline3d`): they mesh to `LineList`/`LineStrip` with no normals and draw as hairlines under `StandardMaterial`. A later change can add them with a line material.
- Generic `Ring<P>`: `annulus` is the circle ring. Other rings need a nested shape prop; later if asked.
- `Mesh2d` with `ColorMaterial` and a 2D camera. 2D shapes are `Mesh3d` in the XY plane, which is what Bevy's own builders produce.
- Binding mesh props to functions (still refused, as on `<material>`).

## Decisions

### 1. Shape and prop names follow Bevy, in camelCase
`shape` is the Bevy type name in lower camelCase: `capsule3d`, `conicalFrustum`, `regularPolygon`. `plane` stays `plane` (Bevy's `Plane3d`): renaming it would break scenes for no gain, and there is no 2D plane mesh to confuse it with. Props are the constructor's parameters (`Cylinder::new(radius, height)` → `radius`, `height`; `Torus::new(inner_radius, outer_radius)` → `innerRadius`, `outerRadius`; `CircularSector::new(radius, angle)` → `angle`; `CircularSegment::new(radius, half_angle)` → `halfAngle`), so Bevy's docs read across. Constructors that take per-axis lengths (`Cuboid`, `Rectangle`, `PlaneMeshBuilder::size`) take one `size` vector, as `cuboid` and `plane` do today. Angles are radians, as in Bevy.

| shape | shape props (default: Bevy's) | builder settings |
|---|---|---|
| `cuboid` | `size: Vec3` | |
| `sphere` | `radius` | `kind: "ico" \| "uv"`, `subdivisions` (ico), `sectors`, `stacks` (uv) |
| `plane` | `size: [x, z]`, `normal: Vec3` | `subdivisions`, `subdivisionsX`, `subdivisionsZ` |
| `cylinder` | `radius`, `height` | `resolution`, `segments`, `caps`, `anchor: "midPoint" \| "top" \| "bottom"` |
| `capsule3d` | `radius`, `length` | `rings`, `longitudes`, `latitudes`, `uvProfile: "aspect" \| "uniform" \| "fixed"` |
| `cone` | `radius`, `height` | `resolution`, `anchor: "midPoint" \| "tip" \| "base"` |
| `conicalFrustum` | `radiusTop`, `radiusBottom`, `height` | `resolution`, `segments` |
| `torus` | `innerRadius`, `outerRadius` | `minorResolution`, `majorResolution`, `angleRange: [start, end]` |
| `tetrahedron` | `vertices: [Vec3 ×4]` | |
| `triangle3d` | `vertices: [Vec3 ×3]` | |
| `circle` | `radius` | `resolution` |
| `circularSector` | `radius`, `angle` | `resolution`, `uvAngle` |
| `circularSegment` | `radius`, `halfAngle` | `resolution`, `uvAngle` |
| `ellipse` | `halfSize: Vec2` | `resolution` |
| `annulus` | `innerRadius`, `outerRadius` | `resolution` |
| `rectangle` | `size: Vec2` | |
| `rhombus` | `horizontalDiagonal`, `verticalDiagonal` | |
| `regularPolygon` | `circumradius`, `sides` | |
| `triangle2d` | `vertices: [Vec2 ×3]` | |
| `capsule2d` | `radius`, `length` | `resolution` |
| `convexPolygon` | `vertices: Vec2[]` (required) | |

Every 2D shape also takes `depth` and, with it, `segments` (the `Extrusion<T>` builder). `uvAngle` is `CircularMeshUvMode::Mask { angle }`, Bevy's only UV mode.

Alternative: Rust field names (`half_size`, `minor_radius`). Rejected: Bevy users write constructors, not struct literals, and the existing `cuboid size` is already a constructor-style full length.

### 2. A discriminated union in TS, a tagged enum in Rust
TS: `type MeshProps = (CuboidMesh | SphereMesh | ...) & { color?: string }`, each member `{ shape: "cuboid"; size?: Vec3 }` and so on, exported per shape so a prefab can take `CylinderMesh` props. `tsc` narrows on `shape` and rejects another shape's props. An extrudable 2D member is `{ shape: "circle"; ... } & Extrude`, `Extrude = { depth?: number; segments?: number }`; `segments` without `depth` is a load error rather than a type error, to keep the types flat.

Rust: `MeshProps { shape: MeshShape, color: Option<String> }`, with `MeshShape` a `#[serde(tag = "shape", rename_all = "camelCase")]` enum of one struct per shape, each `rename_all = "camelCase"`, `deny_unknown_fields`, every field `Option`. `MeshProps` deserializes by hand: take `color` out of the object, then deserialize the rest as `MeshShape`, because `#[serde(flatten)]` and `deny_unknown_fields` do not combine. Serde's message for an unknown field names it; the parse wraps it with the shape name. `resolve.rs` and `reconcile.rs` keep reading `p.color`.

`check()` moves to a method per shape struct and returns `Err` for every value Bevy would panic or error on (spec list); `mesh()` builds through the Bevy builder, setting only the props given, so unset props stay Bevy's default. `cache_key()` is `format!("{:?}", self.shape)`: the enum is the full identity, and the derive replaces the hand-written key. `MeshShape` derives `PartialEq` as `MeshProps` does now.

Alternative: keep one flat struct with every prop optional. Rejected: 40 optional fields, and "this shape does not take that prop" becomes a hand-kept table instead of serde's `deny_unknown_fields`.

### 3. Existing scenes keep working
`cuboid` with no `size` is still `1×1×1`; `sphere` with no `radius` is still radius 0.5 and Bevy's default ico sphere (subdivisions 5); `plane` with no `size` is still `1×1` facing +Y. Scenes that used a prop on the wrong shape were already load errors and stay errors. The TS type narrows `size` per shape; a scene that passed a `Vec3` to `plane` typechecked before and failed at load, and now fails at `tsc` too. That is not counted as a break.

### 4. The headless test checks geometry, not pixels
`crates/bevy-ts-tsx/tests/primitives.rs` builds the existing test app (`MinimalPlugins`, `AssetPlugin`, mesh and material assets, no GPU), loads one scene with an `<entity>` per shape (defaults), one per 2D shape extruded, and a few with settings, then reads each `Mesh` from `Assets<Mesh>`:

- `count_vertices() > 0`, `ATTRIBUTE_NORMAL` present with one normal per vertex, `TriangleList` with an index count that is a multiple of 3.
- Winding: for every triangle, the face normal `(b - a) × (c - a)` has a positive dot with the sum of its three vertex normals. This catches a flipped face on any shape, flat or curved.
- Outward: on every closed solid (the 3D shapes except `plane` and `triangle3d`, and every extrusion), the signed volume `Σ a · (b × c) / 6` is positive. This holds for non-convex closed meshes too (`torus`, extruded `annulus`), unlike a centroid test.
- Flat 2D shapes: every normal is `+Z`.

Cylinder with `caps: false` is open, so it gets the winding check only.

## Risks / Trade-offs

- [A Bevy upgrade renames a shape or builder setting] → Names are copied from Bevy 0.20; the test spawns every shape, so a build or test failure shows the drift at the upgrade.
- [Bevy panics on a value we did not foresee] → Each check mirrors the `assert!`s and `Result`s in `bevy_shape` and `bevy_mesh` for that shape; tasks list reading them. The test includes each error case from the spec.
- [Serde's error text for an unknown field lists every expected field] → Acceptable; it is accurate. Wrapped to lead with the shape and prop.
- [Large `resolution` or `subdivisions` values build big meshes] → Same as Bevy; the mesh is built once per distinct props and cached. No cap beyond Bevy's own (ico 79).
