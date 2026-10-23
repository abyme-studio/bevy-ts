## Why

`<mesh>` builds only `cuboid`, `sphere` and `plane`. A game that wants a cylinder, a capsule or a torus must write Rust or ship a glTF file. Bevy 0.20 ships 22 shapes that build a solid mesh, each with builder settings (resolution, segments, rings). TSX should offer all of them, under Bevy's names, typed.

## What Changes

- **`<mesh>` takes every Bevy 0.20 primitive that builds a surface mesh.** 3D: `cuboid`, `sphere`, `plane`, `cylinder`, `capsule3d`, `cone`, `conicalFrustum`, `torus`, `tetrahedron`, `triangle3d`. 2D: `circle`, `circularSector`, `circularSegment`, `ellipse`, `annulus`, `rectangle`, `rhombus`, `regularPolygon`, `triangle2d`, `capsule2d`, `convexPolygon`.
- **Props are Bevy's.** Each shape's props are its Bevy constructor's parameters in camelCase (`radius`, `height`, `innerRadius`, `sides`), plus its mesh builder's settings (`resolution`, `segments`, `caps`, `anchor`, `rings`, `longitudes`, `latitudes`, `uvProfile`, `subdivisions`, `minorResolution`, `majorResolution`). An unset prop takes Bevy's default.
- **A 2D shape with `depth` is an extrusion** (Bevy's `Extrusion<T>`), with `segments` along its depth.
- **Typed per shape.** `MeshProps` in `bevy-ts/jsx-runtime` becomes a discriminated union on `shape`, so a prop another shape takes is a type error.
- **Checked per shape at load.** An unknown prop, a wrong length, a non-positive size, or a setting Bevy would panic on (a polygon with fewer than 3 sides, an icosphere over 79 subdivisions, a concave `convexPolygon`) fails the load at the element, never panics.
- **Not breaking.** `cuboid`'s `size`, `sphere`'s `radius`, `plane`'s `size` and `color` keep their meaning and defaults. Props that were type errors before stay errors.
- **No protocol change.** `<mesh>` props do not cross the remote protocol.
- **Out of scope:** line shapes (`Segment2d`, `Segment3d`, `Polyline2d`, `Polyline3d`), generic `Ring<P>` other than `annulus`, `Mesh2d` and 2D cameras, and binding mesh props to functions.

## Capabilities

### New Capabilities

None.

### Modified Capabilities

- `jsx-runtime`: a new requirement, "`<mesh>` builds Bevy's mesh primitives", says which shapes, how props map to Bevy, what is checked, and that equal props share one mesh.

## Impact

- Nothing here needs the operator's go: no new dependency (the shapes come from `bevy_mesh`/`bevy_shape`, already built), no performance change (meshes stay cached by props and are built once), no new pattern.
- Code: `crates/bevy-ts-tsx/src/intrinsics.rs` (`MeshProps` becomes a shape enum with per-shape checks and builders), `resolve.rs` and `reconcile.rs` where they match `MeshProps { color, .. }`.
- Types: `packages/bevy-ts/jsx-runtime.d.ts` (`MeshProps` union); `packages/bevy-ts/test/tsx/elements.tsx` (its "unknown shape" case uses `torus`, which becomes valid).
- Docs: new `docs/meshes.md` (every shape, its props and defaults), linked from `README.md`.
- Tests: a headless test in `crates/bevy-ts-tsx/tests/` spawns every shape and checks the mesh: vertices, normals, and normals that agree with winding and point outward.
