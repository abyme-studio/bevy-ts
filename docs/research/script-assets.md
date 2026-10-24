# script-assets: what a chunk remesh costs from TS

Change script-assets, design decision 5. A voxel chunk is remeshed from TS through
`meshes: ResMut<Assets<Mesh>>`: `getMut`, four `insertAttribute` and one `insertIndices`. Each
call passes one typed array, viewed in place, and Rust copies it once into the mesh's own `Vec`.
The question was whether that path is close enough to doing the same copies in Rust for the docs
to recommend remeshing per frame.

## Method

`crates/bevy-ts-cli/tests/it/mesh_update_bench.rs`, ignored by default, run in release through
the `mesh-update-bench` process. A 24,000-vertex chunk surface: positions, normals, UV 0, colours
(all `f32`) and 36,000 `U32` indices, about 1.2 MB a remesh. Each arm runs 3 times, 200 frames
after 20 to warm up, each run in a fresh headless app (`MinimalPlugins`, `AssetPlugin`,
`MeshPlugin`).

Each TS arm is timed by the system's own run time (`TsSystems`' `last_run_us`: the JS, its host
calls and the copies), with the median run of the same system doing nothing (6.0 µs) subtracted.
The Rust arm is timed inside its system. A first version timed whole frames; the frame's other
work and its noise (250-375 µs) swamped the remesh and gave negative numbers, so it was dropped.

## Results

Apple M5 Max, 2026-10-24.

| arm | what | median µs | p95 µs | ns/vertex |
|---|---|---|---|---|
| a | Rust `get_mut` + the same copies (floor) | 29.3 | 37.2 | 1.22 |
| b | TS `getMut` + 4 `insertAttribute` + `insertIndices`, same lengths | 45.0 | 84.0 | 1.88 |
| c | TS, the vertex count changing every frame (every buffer allocated) | 50.0 | 88.0 | 2.08 |
| d | TS `Mesh.new` + `meshes.add` of a whole chunk, the old one dropped | 52.0 | 91.0 | 2.17 |

Target: (b) at most 1.5 × (a) + 5 µs = 48.9 µs. Met: 45.0 µs. (a) and (b) are well outside each
other's noise, so 3 runs per arm stand.

## Reading

- The copies dominate: (b) is (a) plus about 16 µs, six host calls and the JS around them.
- A changed vertex count costs about 5 µs more than reusing the buffers: the allocations.
- Adding a new mesh per edit (d) costs little more than updating one (c), but each add is a new
  asset Bevy prepares and uploads as new; `getMut` on one mesh per chunk is the path to use, and
  `assets.drop` the ones replaced.
- Not measured: Bevy's re-upload of the whole mesh on `Modified`, which is the same for Rust and
  TS (partial upload is not in Bevy 0.20).
- Arm (e) of the design, `assets.set` of a mesh as JSON, waits for stage 3's `Mesh` entry.
