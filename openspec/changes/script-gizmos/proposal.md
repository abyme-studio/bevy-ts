## Why

TS cannot draw a gizmo. Bevy's `Gizmos` is the first debugging tool a Bevy user reaches for, and
TS has only the `Show*Gizmo` markers (`docs/research/bevy-coverage.md:154-155`). Since
writable-native-params landed, an exported param can defer drawing to after the run, so `Gizmos`
can reach TS as Bevy's own param, with Bevy's names and shapes.

## What Changes

Three stages, each landable alone. `docs/research/script-gizmos.md` lists every Bevy 0.20 gizmo
item with its TS form, status and test.

1. **`Gizmos` as a system param.** `export function debug(gizmos: Gizmos) { gizmos.line(a, b,
   "#f00"); gizmos.circle(at, 1, red).resolution(64); }`. Every immediate drawing method of Bevy's
   `GizmoBuffer` except primitives and curves: lines, rays, strips and loops (2D and 3D, with
   gradients), rects, cubes, AABBs, crosses, circles, ellipses, spheres, arcs, arrows, axes, grids,
   rounded boxes and stroke text. Builder settings chain as in Rust (`.resolution(n)`,
   `.withTipLength(l)`, `.outerEdges()`). `Gizmos<LightGizmoConfigGroup>` draws into Bevy's light
   group, and so for each registered group; `gizmos.config()` reads the group's config. A Rust
   crate exposes its own group with `app.ts_gizmo_group::<G>()`. Exported-method arguments gain
   Bevy's colour, isometry, transform, `UVec2`/`UVec3` and point-list forms, for any export. A
   `bench/gizmos` measures 1,000 TS lines a frame against Rust; `examples/gizmos` ports Bevy's
   `3d_gizmos` example; `docs/gizmos.md` holds the table.
2. **Config, primitives and curves.** `ResMut<GizmoConfigStore>`: `store.config(Group)` and
   `store.insert(Group, config, ext)`, every field written through `reflect_value` (line width,
   style, joints, depth bias, render layers, `enabled`, each group's own fields).
   `gizmos.primitive3d({ Cuboid: { half_size } }, iso, color)` for every `GizmoPrimitive2d`/`3d`
   impl, and `gizmos.curve3d(curve, times, color)` for Bevy's reflected curves, both written
   through `reflect_value`.
3. **Retained gizmos.** `new GizmoAsset()` takes the same drawing calls; `assets.add(GizmoAsset, g)`
   adds it on reflect-assets' path and `<Gizmo handle={ref} line_config={...} />` draws it.

## Capabilities

### New Capabilities

- `script-gizmos`: TS draws Bevy gizmos through `Gizmos` and its config groups, configures them
  through `GizmoConfigStore`, and builds retained `GizmoAsset`s.

### Modified Capabilities

- `native-functions`: exported methods take colours, isometries, transforms, `UVec2`/`UVec3`,
  point lists, lists of tuples and reflected JSON values; a method may return its builder for
  chaining; `stands_for` also claims `Gizmos` spellings.

## Impact

- Code: new `crates/bevy-ts-native/src/gizmos/` (the param, its method table, config, primitives, retained replay, behind a default `gizmos` feature); new `crates/bevy-ts-cli/src/gizmos.rs` (the plugin and `bevy-ts/gizmos`); `crates/bevy-ts/src/host.rs`
  (argument kinds, builder chaining, the glue); `crates/bevy-ts-native/src/lib.rs`
  (`ts_gizmo_group`); `crates/bevy-ts-native-derive/src/lib.rs` (`stands_for` spellings, the new
  kinds); `crates/bevy-ts-cli/src/reader.rs` (`Gizmos` claims); `crates/bevy-ts-cli/src/types.rs`,
  `cli.rs` (the plugin, declarations); stage 3 registers a hand-map entry with reflect-assets.
- Depends on: writable-native-params (landed); reflect-enums-color's `reflect_value` writer
  (landed); stage 3 on reflect-assets stage 1 and on reflect-assets widening `HandMap` to an asset
  type with no `Reflect` (`GizmoAsset` is `TypePath` only, `bevy_gizmos/src/lib.rs:325`), to ask of
  lead@reflect-assets (task 5.1).
- Protocol: the handshake's `ExportedParam` lists the new exports; `standsFor` carries `Gizmos<..>`
  spellings. No schema change expected; if one shows, the next free minor.
- Performance: a new path only. Each drawing call is one host call (about 113 ns today,
  `docs/research/writable-native-params.md` section 3) plus its argument encoding; the bench
  states the TS cost of 1,000 lines a frame against Rust in `docs/gizmos.md`. Nothing existing gets
  slower.
- No new dependency; `bevy_gizmos` and `bevy_gizmos_render` are already in the engine's `3d`
  feature.
- Tests: new `crates/bevy-ts-cli/tests/it/gizmos.rs`; unit tests in `host.rs`; cases in
  `packages/bevy-ts/test/gizmos.ts`. Frames: `~/dev/bevy-ts-frames/script-gizmos/`.
- Docs: new `docs/gizmos.md`; `docs/native.md` (argument kinds, chaining, `ts_gizmo_group`);
  `docs/research/bevy-coverage.md` rows; `README.md` link.
