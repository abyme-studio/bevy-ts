## MODIFIED Requirements

### Requirement: An example ships when it declares itself
An example SHALL ship in the `bevy-ts` npm package exactly when its directory under `examples/` holds an `example.json` with `id` (`[a-z0-9-]+`, unique), `title`, `description` and `entry` (the path of the entry file, relative to the example). An example SHALL declare itself only when it runs on the prebuilt engine, that is when its `bevy-ts.json` has nothing under `native`. The package SHALL carry each shipped example's `src/`, `assets/`, `bevy-ts.json`, `README.md` and `thumbnail.png` where present, and its `example.json`, and nothing else from it. An example that carries third-party assets SHALL credit them, with their licences, in its `README.md`.

#### Scenario: The first release
- **WHEN** the `bevy-ts` package is packed
- **THEN** it holds `examples/cube`, `examples/arena` and `examples/foxes` and no `rapier`, `thrusters`, `WALK.md` or `walk.mjs`

#### Scenario: An example that needs Rust
- **WHEN** an example with `native.ports` in `bevy-ts.json` holds an `example.json`
- **THEN** packing fails and names that example

#### Scenario: The foxes example
- **WHEN** `bevy-ts new ~/games/f --example foxes --no-install` runs and the project is started with `bevy-ts dev`
- **THEN** a grid of foxes runs on the prebuilt engine, a button switches every fox between survey, walk and run with a cross-fade, and the README credits `Fox.glb` (CC0 model by PixelMannen; CC-BY 4.0 rigging and animation by @tomkranis)
