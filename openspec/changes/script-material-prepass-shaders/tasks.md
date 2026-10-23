## 1. Engine

- [ ] 1.1 `kinds.rs`: `prepassFragment`, `prepassVertex` stages: decl, build, hot reload, check under the prepass pipeline's defs; defaults per the spec.
- [ ] 1.2 `render.rs`: `PrepassFragmentShader`/`PrepassVertexShader` on the kind's properties; `Kind` carries both handles.
- [ ] 1.3 `ts/material.js`/`material.d.ts` and the `packages/bevy-ts` copies.

## 2. Tests

- [ ] 2.1 Headless render: a masked leaf quad's shadow and ink follow the leaf, not the square.
- [ ] 2.2 Headless render: a swaying `vertex` + `prepassVertex` kind's shadow moves with it.
- [ ] 2.3 Reload and error tests for both stages; defaults per alpha mode and stages.

## 3. Docs and gate

- [ ] 3.1 `docs/materials.md`: foliage example; shared cutout function through a module import.
- [ ] 3.2 The repo's gate, every cargo step through `gate-slot`.
