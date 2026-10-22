# Meshes

`<mesh shape>` builds a `Mesh3d` from one of Bevy 0.20's mesh primitives. The shape is the Bevy
type's name in camelCase; its props are the Bevy constructor's parameters and its mesh builder's
settings, in camelCase. An unset prop is Bevy's default. Angles are radians.

```tsx
<entity>
  <mesh shape="cylinder" radius={0.5} height={2} resolution={8} color="#c33" />
</entity>
```

`color` works on every shape: a shorthand for `<material color>`. Equal props share one mesh
asset, so a thousand identical enemies make one mesh. Props cannot be bindings. The code is
`crates/bevy-ts-tsx/src/shapes.rs`; the types are `MeshProps` and one interface per shape
(`CylinderMesh`, ...) in `bevy-ts/jsx-runtime`, so a prop the shape does not take is a `tsc` error.

## 3D shapes

| shape | props (default) | builder settings (default) |
|---|---|---|
| `cuboid` | `size: [x, y, z]` full lengths (`[1, 1, 1]`) | |
| `sphere` | `radius` (0.5) | `kind: "ico" \| "uv"` (`"ico"`); ico: `subdivisions` (5, at most 79); uv: `sectors` (32), `stacks` (18) |
| `plane` | `size: [x, z]` (`[1, 1]`), `normal` (`[0, 1, 0]`) | `subdivisions`, `subdivisionsX`, `subdivisionsZ` (0) |
| `cylinder` | `radius` (0.5), `height` (1) | `resolution` (32), `segments` (1), `caps` (true), `anchor: "midPoint" \| "top" \| "bottom"` |
| `capsule3d` | `radius` (0.5), `length` (1, between the hemispheres' centres) | `rings` (0), `longitudes` (32), `latitudes` (16, even), `uvProfile: "aspect" \| "uniform" \| "fixed"` |
| `cone` | `radius` (0.5), `height` (1) | `resolution` (32), `anchor: "midPoint" \| "tip" \| "base"` |
| `conicalFrustum` | `radiusTop` (0.25), `radiusBottom` (0.5), `height` (0.5) | `resolution` (32), `segments` (1) |
| `torus` | `innerRadius` (0.5), `outerRadius` (1) | `minorResolution` (24), `majorResolution` (32), `angleRange: [start, end]` (`[0, 2π]`) |
| `tetrahedron` | `vertices: [a, b, c, d]` (Bevy's regular tetrahedron in the unit cube) | |
| `triangle3d` | `vertices: [a, b, c]` (`[[0, 0.5, 0], [-0.5, -0.5, 0], [0.5, -0.5, 0]]`) | |

Bevy has no default for a uv sphere's resolution; 32 by 18 is the one its docs suggest.

## 2D shapes

A 2D shape lies in the XY plane facing +Z. Given `depth`, it is Bevy's `Extrusion` of that shape:
`depth` deep along Z, centred on the origin, with `segments` (1) along the depth.

| shape | props (default) | builder settings (default) |
|---|---|---|
| `circle` | `radius` (0.5) | `resolution` (32) |
| `circularSector` | `radius` (0.5), `angle` (4π/3, the whole angle, up to 2π) | `resolution` (32), `uvAngle` (0) |
| `circularSegment` | `radius` (0.5), `halfAngle` (2π/3, up to π) | `resolution` (32), `uvAngle` (0) |
| `ellipse` | `halfSize: [x, y]` half lengths (`[1, 0.5]`) | `resolution` (32) |
| `annulus` | `innerRadius` (0.5), `outerRadius` (1) | `resolution` (32) |
| `rectangle` | `size: [x, y]` full lengths (`[1, 1]`) | |
| `rhombus` | `horizontalDiagonal` (1), `verticalDiagonal` (1) | |
| `regularPolygon` | `circumradius` (0.5), `sides` (6) | |
| `triangle2d` | `vertices: [a, b, c]`, either winding (`[[0, 0.5], [-0.5, -0.5], [0.5, -0.5]]`) | |
| `capsule2d` | `radius` (0.5), `length` (1) | `resolution` (16, per half circle) |
| `convexPolygon` | `vertices` (required), either winding | |

`uvAngle` rotates the UV mask (Bevy's `CircularMeshUvMode::Mask`).

## What fails the load

These fail the load at the element, with an error naming the shape, the prop and the file and
line; the engine keeps running the last good generation:

- a prop the shape does not take, a value of the wrong type, or a vector of the wrong length;
- a size, radius, length, height or depth that is not positive and finite;
- a value Bevy would panic or error on: fewer than 3 sides or vertices, a `resolution` (any of
  them) below 3, `segments` below 1, a uv sphere with fewer than 2 stacks, an ico sphere over 79
  subdivisions, a capsule's `latitudes` odd or below 4, a `convexPolygon` that is not convex or has
  no area, an `annulus` or `torus` whose inner radius is not below its outer one, a sector's
  `angle` over 2π or a segment's `halfAngle` over π, a `plane` normal of zero;
- a setting for the other kind (`subdivisions` on a uv sphere, `sectors` or `stacks` on an ico
  one), and `segments` on a 2D shape without `depth`.

Not here yet: line shapes (`Segment2d`, `Polyline3d`, ...), rings of shapes other than the circle,
and 2D meshes for a 2D camera.
