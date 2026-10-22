## Why

A script-only game cannot click a 3D mesh: Bevy keeps `MeshPickingPlugin` out of `DefaultPlugins`, and today only a native crate can add it. Nor can a test click anything under `bevy-ts dev --headless`: `bevy-ts/viewport_input` exists only with a viewport, and the mouse pointer targets a window that headless runs do not have. agent-town hit both (research: `openspec/research/mesh-picking.md`).

## What Changes

- `"meshPicking": true` in `bevy-ts.json` (a bool, default `false`) adds Bevy's `MeshPickingPlugin` to the engine, under `dev` and in a built game. No Cargo feature and no project engine: the prebuilt engine already compiles the backend.
- `bevy-ts build` writes `meshPicking` into `game/game.json`; playing reads it from there. A `game.json` without it plays with mesh picking off.
- `bevy-ts dev --headless` serves `bevy-ts/viewport_input` for `pointer` events. They drive a Bevy custom pointer aimed at the headless offscreen image, so `bevy_picking` hits and `onPointer*` handlers fire as in a window. Other event types are refused with an error naming the type. The method keeps one owner, `bevy-ts-viewport`, through a module built on every platform.
- The engine warns once when an entity with `Mesh3d` has an `onPointer*` handler and mesh picking is off, naming `"meshPicking": true`.
- No protocol change: `viewport_input`'s params and result are unchanged, the contract stays 1.23.0, and `hello.viewport` stays `false` headless.
- Docs: a new `docs/picking.md`, a README line, and the `bevy-ts.json` comment in `project.rs`.

## Capabilities

### New Capabilities
- `mesh-picking`: the `meshPicking` project switch, its bake into `game.json`, and the warning when it is off.

### Modified Capabilities
- `viewport-input`: `bevy-ts/viewport_input` is also served under `--headless`, for `pointer` events, through a custom pointer on the offscreen image.
- `shared-surface`: the windowed-mode requirement names headless as the other place `viewport_input` is served.
- `shipped-game`: `game.json` may hold `meshPicking`.

## Impact

- Code: `bevy-ts-cli` (`project.rs`, `cli.rs` `base_app` and `run_dev`, `build.rs`, `play.rs`), `bevy-ts-viewport` (a new cross-platform `offscreen` module; `lib.rs` gating), tests in `bevy-ts-cli/tests`.
- Protocol: none. No version bump.
- Dependencies: none new. `bevy-ts-cli` already enables `3d`, which pulls `mesh_picking`.
- Performance: zero with the switch off (the default): no plugin, no ray casts. On, one ray per pointer per camera per frame while the pointer is over it. The headless pointer system is idle without input. The warning is one system over newly added rows, only while mesh picking is off.
- New pattern, for the operator's go: the first engine plugin toggled from `bevy-ts.json`, and the first `bevy-ts.json` field baked into `game.json`.
