## Why

world-text stage 1 (archived as `2026-10-26-world-text`) draws `<worldText>` labels in the 3D world
from a shared distance-field atlas. Text along a curve, a road name or a ring, is its stage 2, split
out so stage 1 could archive. Until it lands, `path` and `normal` fail the load.

## What Changes

- `<worldText path={[[x, y, z], ...]} normal={[x, y, z]}>`: each glyph placed on the polyline by
  arc length, turned to its tangent, facing `normal`; one line only.
- `path` with `width`, or a line break in a path label, fails the load naming the prop.
- No new atlas or shader work: a path is a per-glyph transform in `mesh.rs` (the archived design's
  decision 9).

## Capabilities

### Modified Capabilities

- `world-text`: text along a path replaces stage 1's refusal of `path`.

## Impact

`crates/bevy-ts-material/src/world_text/{mod.rs,mesh.rs}`, `crates/bevy-ts-tsx/src/ui.rs`,
`crates/bevy-ts-material/tests/it/world_text.rs`, `packages/bevy-ts/jsx-runtime.d.ts`,
`packages/bevy-ts/test/tsx/elements.tsx`, `docs/world-text.md`, `examples/cube`. Starts after
world-text stage 1 is on main.
