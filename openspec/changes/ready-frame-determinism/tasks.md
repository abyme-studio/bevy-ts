## 1. Engine

- [x] 1.1 `Ready.steps`: `FixedStep` time stops at a waiting `ShotWhenReady`'s `at` steps; a unit test steps a ready frame, a waiting one, then ready frames, and the shot's clock is `at` steps
- [x] 1.2 `ReadyPlugin` gives every camera `NoIndirectDrawing` on spawn under `FixedStep`
- [x] 1.3 `docs/build.md`: the shot's clock and drawing, and what `bevy-ts/screenshot` without the fixed step keeps

## 2. Tests

- [ ] 2.1 `stacked_instances_at_one_depth_are_the_same_every_shot`: 64 copies of a textured quad at one depth, shot four times, two with 30 ms of extra work a frame, byte-identical; it fails without 1.2
- [ ] 2.2 Agent town's civ hazards frame, 8 runs with a build of this branch, byte-identical
- [ ] 2.3 Agent town's learn frame, 6 runs with a build of this branch, byte-identical
