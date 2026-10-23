## 1. Engine: prepass and shadow pass

- [ ] 1.1 `kinds.rs`: `MaterialDecl` reads optional `shadows` and `prepass`; `Kind` carries both, resolved by the defaults in design decision 3.
- [ ] 1.2 `render.rs`: register the prepass and shadow draw functions, set `prepass_specialize` (a copy of Bevy's, with its source named), `shadows_enabled` and `prepass_enabled` from the kind.
- [ ] 1.3 `ts/material.js` and `ts/material.d.ts` (and the `packages/bevy-ts` copies): `shadows?: boolean`, `prepass?: boolean`, with a boolean check at the `material()` call.

## 2. Tests

- [ ] 2.1 `crates/bevy-ts-material/tests/toon.rs`: the proof's three-panel headless render, asserting a shadowed ground pixel on the toon panel is darker than an open one, and that the ink panel has ink-coloured pixels along a figure's silhouette and none inside the flat ground. Writes the PNGs only when `TOON_OUT` is set.
- [ ] 2.2 Flags: `shadows: false` casts no shadow; a `vertex` kind defaults out of both passes; explicit `true` brings it in.
- [ ] 2.3 `crates/bevy-ts-cli/tests/toon_camera.rs`: a TSX camera with `<Camera3d />`, `<Projection value={{ Orthographic: ... }} />`, `<DepthPrepass />`, `<NormalPrepass />` and `<Tonemapping value="None" />` spawns with all four, and a changed height writes in place. It failed first: no prop could switch `Projection`'s variant.
- [ ] 2.5 `crates/bevy-ts-tsx/src/reflected.rs`: a reflect-tier enum component is set by `value` (a newtype variant from `Default`/`FromWorld`, partial fields), at insert and on change; `types.rs` declares reflect-tier structs and enums with typed props in `bevy-ts/bevy` and `native.d.ts`; `packages/bevy-ts/test/tsx/toon-camera.tsx` type-checks the docs' camera.
- [ ] 2.4 `kinds.rs` unit tests: defaults per alpha mode and `vertex`, and a non-boolean flag fails at `material()`.

## 3. Bench

- [ ] 3.1 `crates/bevy-ts-material/tests/toon_bench.rs` (ignored; process `toon-bench`): 100 animated `Fox.glb` (fetched at a pinned Bevy commit into a cache, not committed) with a toon script material, orthographic camera, shadowed sun, offscreen, release. Arms: no prepass; prepass plus ink. Record mean and p95 frame time in `bench/toon/results/`.

## 4. Docs

- [ ] 4.1 `docs/materials.md`: a toon and ink section (the camera in TSX, the shader, the flags and their defaults, tuning notes: clamp ink taps at the frame edge, shadow map size), and the bench numbers.
- [ ] 4.2 `docs/research/toon-look.md`: point at the change; remove `proof.patch` once the test replaces it.
- [ ] 4.3 Follow-ups noted in docs/materials.md's known limits: custom prepass shaders; toon materials on glTF scenes from TS.

## 5. Gate

- [ ] 5.1 `cargo test -p bevy-ts-material`, the TS tests for `material()`, `scripts/gen-bevy-types.sh` check, and the repo's full check pass.
