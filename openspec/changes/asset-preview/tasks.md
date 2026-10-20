## 1. Several viewports

- [ ] 1.1 Key viewports by id in `bevy-ts-viewport` (service, ring, pacing, input queue, camera target); `view` on `viewport_attach`, replace-per-view; the game viewport keeps the virtual window, edit mode, picking and edits
- [ ] 1.2 `viewport` on `viewport_detach`, `viewport_resize`, `viewport_input`, `viewport_camera`, `viewport_stats`, defaulting to the game viewport; render only attached and visible viewports
- [ ] 1.3 Tests: two viewports deliver frames at their own pace; input to one moves only its camera; a 1.6 client (no `view`, no `viewport`) behaves as before

## 2. Preview scenes

- [ ] 2.1 `asset_preview`: load through `assets.rs`'s path, `pending` while loading, the layer pool 20-29 with least-recently-attached eviction, a far root, a framing camera and a layer-only light rig, game camera's tonemapping, exposure and environment when a game runs
- [ ] 2.2 `PreviewOnly` beside `EditorOnly`: excluded from TS queries, `tsx_tree`, `entities+watch`, `.bsn` writing; no colliders
- [ ] 2.3 Orbit, zoom and `f` from the preview viewport's input in Rust; `asset_preview_control` for glTF animations; `asset_preview_close`; 30 s keep-alive after detach
- [ ] 2.4 Tests: the arena's frames are byte-identical with a preview open; preview entities never reach a TS query or `entities+watch`; an eleventh preview evicts the oldest

## 3. Always up

- [ ] 3.1 A failed first generation applies an empty generation and keeps serving the protocol, thumbnails and previews; the next good save applies as a first load
- [ ] 3.2 Test: `bevy-ts dev --viewport` on a project whose entry throws answers `hello` and `asset_preview`, reports `failed`, and runs the game after the fix

## 4. Contract, measurement, docs

- [ ] 4.1 Protocol 1.7.0: the new methods and fields (`view`, `viewport`, `asset_preview*`), regenerated Rust and TS types, CI diff check
- [ ] 4.2 Measure on this Mac, for the arena and the same model (`models/grunt.glb` or an equal glTF): the game's frame time (CPU and GPU, p50 and p99) with no preview, with an engine preview attached and visible, attached and hidden, and with abyme's three.js viewer showing the model beside the game; the RSS and GPU memory of each and the engine's idle cost with no viewport visible. Record it in `docs/research/asset-preview.md` and get the operator's ruling before landing
- [ ] 4.3 Docs: the viewport and asset-preview sections of the README and `docs/`; a note for abyme's `bevy-studio` on switching its glTF view to the engine preview when `hello` reports 1.7
