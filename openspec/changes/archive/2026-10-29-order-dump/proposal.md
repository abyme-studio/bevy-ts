## Why

On alpha.43, agent town's civ-hazards shot still differs 1 run in 10: 79 pixels, all on edges the
land shader paints, not leaves (research doc `still-flips.md`). The shots alone cannot show the
cause. It is the draw order, the camera's fit, or the material values the game writes. A dump of
what the shot's frame drew, diffed between a matching run and a flipped one, tells which.

## What Changes

- `BEVY_TS_ORDER_DUMP=1` writes `<shot>.order.txt` beside each shot. It lists:
  - each active camera's transform as f32 bits;
  - every entity with an `Aabb`, with its `StableKey` and a hash of each material's reflected fields;
  - per camera view: Opaque3d, AlphaMask3d and both prepasses, bin by bin in draw order, their
    unbatchable bins and how many non-mesh items they hold;
  - Transparent3d's items, with keys.
- Entity ids follow load timing, so they come after a tab, where `cut -f1` drops them.
- Without the variable, none of it is in the app. A test inserts `OrderDump` itself.
- `docs/build.md` says how to use it.

## Capabilities

### Modified Capabilities

- `headless-run`: a shot can write what its frame drew, in order, for a diff.
