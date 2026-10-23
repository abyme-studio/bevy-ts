## Parked (2026-10-23)

The operator put the toon look off ("no toon look for now; shaders come later", via lead@bevy-ts).
Branch `toon-look`, not landed. To resume:

- Done and green before parking: the engine change, the toon render test (`toon.rs`), the flag
  tests (`materials.rs`), the docs, and the bench (`bench/toon/results/2026-10-23.md`: 3.5 ms pbr,
  4.2 ms toon, 4.5 ms toon+ink for 100 foxes).
- Not done: task 5.1, the full gate (process `toon-gate`, through `gate-slot`). It was stopped
  mid-run after the rebase onto main at `54ccbe2`.
- Lands after `reflect-enums-color`, which owns `<Projection value>` for the toon camera.
- Needs a go (performance): existing opaque script materials start casting shadows.
- Stages 2 and 3 are proposals only: `scene-material-textures`, `script-material-prepass-shaders`.

## 1. Engine: prepass and shadow pass

- [x] 1.1 `kinds.rs`: `MaterialDecl` reads optional `shadows` and `prepass`; `Kind` carries both, resolved by the defaults in design decision 3.
- [x] 1.2 `render.rs`: register the prepass and shadow draw functions, set `prepass_specialize` (a copy of Bevy's, with its source named), `shadows_enabled` and `prepass_enabled` from the kind.
- [x] 1.3 `ts/material.js` and `ts/material.d.ts` (and the `packages/bevy-ts` copies): `shadows?: boolean`, `prepass?: boolean`, with a boolean check at the `material()` call.

## 2. Tests

- [x] 2.1 `crates/bevy-ts-material/tests/toon.rs`: the proof's three-panel headless render, asserting a shadowed ground pixel on the toon panel is darker than an open one, and that the ink panel has ink-coloured pixels along a figure's silhouette and none inside the flat ground. Writes the PNGs only when `TOON_OUT` is set.
- [x] 2.2 Flags: `shadows: false` casts no shadow; a `vertex` kind defaults out of both passes; explicit `true` brings it in.
- [ ] 2.3 The toon camera in TSX is reflect-enums-color's (its `reflect_values.rs` and `test/tsx/reflect.tsx`); this change lands after it.
- [x] 2.4 `kinds.rs` unit tests: defaults per alpha mode and `vertex`, and a non-boolean flag fails at `material()`.

## 3. Bench

- [x] 3.1 `crates/bevy-ts-material/tests/toon_bench.rs` (ignored; process `toon-bench`): 100 animated `Fox.glb` (fetched at a pinned Bevy commit into a cache, not committed) with a toon script material, orthographic camera, shadowed sun, offscreen, release. Arms: no prepass; prepass plus ink. Record mean and p95 frame time in `bench/toon/results/`.

## 4. Docs

- [x] 4.1 `docs/materials.md`: a toon and ink section (the camera in TSX, the shader, the flags and their defaults, tuning notes: clamp ink taps at the frame edge, shadow map size), and the bench numbers.
- [x] 4.2 `docs/research/toon-look.md`: point at the change; remove `proof.patch` once the test replaces it.
- [x] 4.3 Follow-ups noted in docs/materials.md's known limits: custom prepass shaders; toon materials on glTF scenes from TS.

## 5. Gate

- [ ] 5.1 `cargo test -p bevy-ts-material`, the TS tests for `material()`, `scripts/gen-bevy-types.sh` check, and the repo's full check pass.
