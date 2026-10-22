## 1. The animation crate

- [ ] 1.1 Create `crates/bevy-ts-animation` (workspace member and dependency), modelled on `bevy-ts-material`: `PlayAnimationPlugin`, `PlayAnimation` with `#[reflect(Component, Default)]` and the defaults in the spec, registered for TS
- [ ] 1.2 Shared graph resource: one `Handle<AnimationGraph>`, a node per clip `AssetId`, added on first use
- [ ] 1.3 `players(world, root)` and its `Query<&Children>` form; move `preview.rs`'s walk onto it and delete the private copy
- [ ] 1.4 Observer on `WorldInstanceReady`: for a root with `PlayAnimation`, find players, insert `AnimationGraphHandle` and `AnimationTransitions`, play, record `AnimationTargets`; warn once per root when there are none (entity, scene path, clip)
- [ ] 1.5 `PostUpdate` system before `AnimationSystems`: `Changed<PlayAnimation>` cross-fades a new clip over `transition_secs`, applies speed and repeat in place, and plays a root added after ready (`instance_is_ready`); `RemovedComponents<PlayAnimation>` stops players
- [ ] 1.6 Log a failed clip load once per path
- [ ] 1.7 Add the plugin to the engine in `bevy-ts-cli` (`cli.rs`, `types.rs`) and to the generated project engine if it lists plugins itself

## 2. TSX one-field tuple structs

- [ ] 2.1 `crates/bevy-ts-tsx/src/resolve.rs`: `value` maps to `.0` for a reflect-tier tuple struct of one field; another prop on one fails with file, line and "takes `value`"
- [ ] 2.2 Tests in `crates/bevy-ts-tsx/tests`: `<WorldAssetRoot value=…>` inserts the loaded handle; a reactive `value` change writes the field; `path=` fails as specified

## 3. Tests

- [ ] 3.1 `crates/bevy-ts-animation/tests` (headless, `MinimalPlugins` plus asset, scene, glTF and animation plugins, `Fox.glb`): plays on ready; added after ready; clip change cross-fades (weights at 0, half and full `transition_secs`); speed change keeps the clip's time; removal stops; 100 roots share one graph with one node per clip; mesh-only scene warns once
- [ ] 3.2 `crates/bevy-ts-cli/tests`: a TSX scene with `<WorldAssetRoot>` and `<PlayAnimation>` plays; a level patch with `PlayAnimation { clip: … }` plays with no TS
- [ ] 3.3 Asset preview tests (`viewport_preview.rs`) still pass on the shared walk
- [ ] 3.4 Run `scripts/gen-bevy-types.sh` and commit `bevy-ts/bevy` with `PlayAnimation`; `--check` passes

## 4. The foxes example

- [ ] 4.1 `examples/foxes`: `assets/models/Fox.glb` from Bevy's repo, `src/game.tsx` with a camera, a light, a ground and a grid of foxes (`<WorldAssetRoot value>` + `<PlayAnimation clip={() => …}>`), and a `<button>` that cycles survey, walk and run
- [ ] 4.2 `example.json`, `README.md` crediting Fox.glb (CC0 model by PixelMannen; CC-BY 4.0 rigging and animation by @tomkranis), `thumbnail.png`
- [ ] 4.3 Packing test: the package holds `examples/foxes` with its README and asset; `bevy-ts examples --json` lists it

## 5. Docs

- [ ] 5.1 `docs/animation.md`: Bevy's pattern and how `PlayAnimation` maps onto it, placing it from TSX, levels and the editor, clips must share bone names with the scene, the mesh-only warning, when to write Rust instead, and the bench results from 6
- [ ] 5.2 Link it from `README.md` and `docs/levels.md` (placing a model), and note `value` for tuple structs where TSX props are documented

## 6. Performance

- [ ] 6.1 Bench `crates/bevy-ts-cli/tests/animation_bench.rs` on `preview_bench.rs`'s method (release engine, game viewport ticked at 120 Hz, one arm per run, a row appended to `target/animation-bench.md`: frame time mean and p95, CPU, RSS), driven by `scripts/animation-bench.sh` like `scripts/preview-bench.sh`. `ARM` and `N` select the run, all with `Fox.glb`:
  - A: a Rust plugin spawning N foxes the `many_foxes` way (own observer, graph, transitions), no TS
  - B: the TS form, N foxes in TSX with `PlayAnimation`
  - C: the same N foxes with no `PlayAnimation` (static)
- [ ] 6.2 Spawn-to-ready for A and B: time from the spawn frame to every player having an active animation, in the same rows
- [ ] 6.3 Run release, 3 runs per arm, at N = 50, 100 and 200; record the table in `docs/animation.md`
- [ ] 6.4 Check the target: at N = 50 and 100, arm B's frame time p95 is within 10% of arm A's and B's spawn-to-ready within one frame of A's. If not, fix before landing, and say so in the docs

## 7. Gate

- [ ] 7.1 `cargo test` for the touched crates, `cargo clippy`, the TSX and cli suites, `scripts/gen-bevy-types.sh --check`, `openspec validate scene-animation --strict`
