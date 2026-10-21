## 1. Tools

- [ ] 1.1 A `ViewportTools` resource (mode, space, snap, origin, change counter) as the one writer of `TransformGizmoSettings` mode, space and snap; `edit_keys` writes it with `origin: viewport`
- [ ] 1.2 `bevy-ts/viewport_tools` and `bevy-ts/viewport_tools+watch` in `methods.rs`
- [ ] 1.3 Tests in `tests/edit.rs`: the method sets the gizmo and answers the steps; `KeyX` streams `space: local` with `origin: viewport`; a snapped x drag of 1.4 from 3 ends at 4; tools survive a mode switch and a reload

## 2. Orientation gizmo

- [ ] 2.1 The corner camera on layer 30 (edit mode, game viewport only), its axes and ends, its rotation following the editor camera, sized by the viewport's scale and kept on resize
- [ ] 2.2 Input: a press in the square is claimed before picking and the transform gizmo; a click on an end turns the editor camera down that axis at its focus; a drag orbits
- [ ] 2.3 Tests: clicking `+Y` gives a straight-down pose at the same focus distance and leaves the selection; no corner camera in play or in a preview; a corner click in play reaches the game

## 3. Camera presence

- [ ] 3.1 `camera` in `ViewportModeResult`, computed each frame in play; `bevy-ts/viewport_mode+watch` sending the full result on open and on any change (mode, pause, step, camera, reload)
- [ ] 3.2 Tests: a game with no camera answers `camera: false` in play; spawning a `Camera3d` streams `camera: true`; pause and step stream `paused`

## 4. Contract, measurement, docs

- [ ] 4.1 Protocol 1.11.0: the three methods, `ViewportTools`, `camera`; regenerated Rust and TS types; CI diff check
- [ ] 4.2 Measure the arena in edit mode on this Mac with and without the orientation gizmo: frame time (CPU and GPU, p50 and p99); record it in `docs/research/viewport-tools.md` and get the operator's ruling before landing
- [ ] 4.3 Docs: the tools, the orientation gizmo and `camera` in `docs/viewport.md`, and a note for abyme's studio to gate on `hello.protocol >= 1.11`
