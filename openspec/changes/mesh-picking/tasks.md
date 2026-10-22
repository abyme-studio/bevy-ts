## 1. Prove the one unproven point

- [x] 1.1 `crates/bevy-ts-cli/tests/headless_pointer.rs`: a fixture project with `"meshPicking": true` and a cube under `onPointerClick`, run with `bevy-ts dev --headless`; post `bevy-ts/viewport_input` `move`, `down`, `up` at the cube's pixel and assert the handler fires once. Build only what it needs (tasks 2.1, 2.2, 3.1–3.3). If the custom pointer does not produce `PointerClick`, stop and report to the lead before any other task.

## 2. The switch

- [x] 2.1 `project.rs`: `mesh_picking: bool` on `ProjectConfig` (`meshPicking`, default false); update the module's `bevy-ts.json` comment; unit tests for absent, true and a non-bool.
- [x] 2.2 `cli.rs`: `base_app` takes `mesh_picking` and adds `MeshPickingPlugin`; `run_dev` reads the project once for `prefabEdits` and `meshPicking`, and exits 1 on a `bevy-ts.json` that does not parse; `skeleton` passes false.
- [x] 2.3 `play.rs` / `build.rs`: `GameFile.mesh_picking` (`meshPicking`, skipped when false, default false); `build` copies it; `play::app` passes it. Extend `tests/build.rs` and `tests/play.rs`: built with it on, `game.json` holds it and the played app has `MeshPickingSettings`; without it, neither.
- [x] 2.4 Test: no `meshPicking` means no `MeshPickingSettings` under `dev --headless`.

## 3. The headless pointer

- [x] 3.1 `bevy-ts-viewport`: move `CRATE` to a cross-platform place; add the `offscreen` module with `OffscreenPointerPlugin`, built on every platform.
- [x] 3.2 The method: validate all events first (pointer only, `viewport` none or 0), queue them, answer `{ queued }`; errors name the refused type.
- [x] 3.3 The system: one event per frame before `PickingSystems::Input`, mapped to `PointerInput` on the offscreen image as design decision 5; `run_dev` adds the plugin under `Display::Headless`.
- [x] 3.4 Tests: a `key` event is refused naming `key` and nothing applies; `viewport: 1` is refused; `hello` still reports `viewport: false`; the contract check passes with `viewport_input` registered headless.
- [ ] 3.5 Run task 1.1's test on Linux (CI) as well as macOS.

## 4. The warning

- [x] 4.1 `cli.rs`: the once-per-process warning system, run while `MeshPickingSettings` is absent, naming the entity and `"meshPicking": true`.
- [x] 4.2 Tests: two cubes with `onPointerClick` and no switch log one warning; with the switch, none; with `MeshPickingPlugin` added by an extra plugin and no switch, none.

## 5. Docs and specs

- [x] 5.1 `docs/picking.md`: what picks out of the box (sprites, UI), `meshPicking`, the warning, and clicking headless over `viewport_input` with an example request.
- [x] 5.2 README: one line linking `docs/picking.md`; `docs/viewport.md`: note that headless serves `viewport_input` for pointers, linking `docs/picking.md`.
- [ ] 5.3 `openspec validate mesh-picking --strict` and the repo's full check pass.
