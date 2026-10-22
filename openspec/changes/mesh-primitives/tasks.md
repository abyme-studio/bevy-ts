## 1. Rust shapes (`crates/bevy-ts-tsx/src/intrinsics.rs`)

- [ ] 1.1 Replace `MeshProps`'s `shape`/`size`/`radius` with `shape: MeshShape`: a `#[serde(tag = "shape", rename_all = "camelCase")]` enum, one `deny_unknown_fields` struct per shape in design 1's table, every field `Option`; deserialize `MeshProps` by taking `color` out first (design 2)
- [ ] 1.2 Per-shape `check()`: lengths of vectors, positive finite sizes and radii, and every value `bevy_shape` 0.20 / `bevy_mesh` 0.20 assert or error on for that shape (read each constructor and builder: sides ≥ 3, resolution ≥ 3, ico subdivisions ≤ 79, `ConvexPolygon::new` error, annulus inner < outer, `segments` only with `depth`); errors name the shape and the prop
- [ ] 1.3 Per-shape `mesh()` through the Bevy builder, setting only the props given; 2D shapes with `depth` build `Extrusion<T>` with `segments`
- [ ] 1.4 `cache_key()` from `MeshShape`'s `Debug`; keep `resolve.rs:325` and `reconcile.rs:585` reading `color`
- [ ] 1.5 Unit tests: the old `cuboid`/`sphere`/`plane` props parse and build the same meshes as before; an unknown prop, a wrong-length vector, and each error case in 1.2 fail with the shape and prop named; nothing panics

## 2. Headless mesh test (`crates/bevy-ts-tsx/tests/primitives.rs`)

- [ ] 2.1 Load one scene with an entity per shape at defaults, one per 2D shape with `depth={1}`, and `cylinder` with `caps={false}`, `sphere` with `kind="uv"`, `capsule3d` with `rings`; read each `Mesh3d`'s mesh
- [ ] 2.2 Check every mesh (cuboid included): vertex count > 0, one normal per vertex, `TriangleList` with an index count divisible by 3, every triangle's winding agrees with its vertex normals (design 4)
- [ ] 2.3 Check every closed solid has a positive signed volume (normals outward), and every flat 2D mesh's normals are `+Z`
- [ ] 2.4 Check `<mesh shape="cylinder" radius={0.5} height={2} resolution={8} />` equals Bevy's `Cylinder::new(0.5, 2.0).mesh().resolution(8).build()` (positions and indices)
- [ ] 2.5 Check a load with `<mesh shape="regularPolygon" sides={2} />` fails with an error naming the shape, the prop and the source location, and the app keeps updating

## 3. TS types (`packages/bevy-ts/jsx-runtime.d.ts`)

- [ ] 3.1 `MeshProps` becomes the union of one exported interface per shape (`CuboidMesh`, `CylinderMesh`, ...) and `{ color?: string }`, with `Extrude` on the 2D shapes; JSDoc each prop with Bevy's default
- [ ] 3.2 `packages/bevy-ts/test/tsx/elements.tsx`: replace the "unknown shape" case (`torus` is now valid) with `shape="dodecahedron"`; add `@ts-expect-error` cases for another shape's prop (`<mesh shape="sphere" sides={6} />`) and a wrong-length vector; add one valid element per shape
- [ ] 3.3 Mirror the types into `bench/dx/bevy-ts/jsx-runtime.ts` if it copies `MeshProps`; run the package's `check` script

## 4. Docs

- [ ] 4.1 New `docs/meshes.md`: every shape, its props and Bevy defaults, builder settings, `depth` extrusion, 2D shapes in the XY plane facing +Z, what fails the load; link it from `README.md`
- [ ] 4.2 `docs/materials.md` and `docs/viewport.md`: link `<mesh>` to `docs/meshes.md`

## 5. Gate

- [ ] 5.1 Confirm `protocol/bevy-ts.openrpc.json` is unchanged (no protocol bump)
- [ ] 5.2 `cargo build`, `cargo test` and `cargo clippy` on the workspace, and `pnpm -C packages/bevy-ts check`, once, last
- [ ] 5.3 `openspec validate mesh-primitives --strict`
