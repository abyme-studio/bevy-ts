## ADDED Requirements

### Requirement: Every engine compiles render pipelines off the render thread on macOS
Every engine bevy-ts builds or ships SHALL build Bevy's render crate from bevy-ts's vendored `bevy_render`, which is the crates.io source at exactly the Bevy version in the workspace `Cargo.lock` with one change: macOS compiles render pipelines on Bevy's async compute pool, as other multi-threaded desktop targets do, instead of blocking the render thread. This holds for the prebuilt engine (the CLI npm ships), a project engine, and the engine a game folder ships. A project engine's generated `Cargo.toml` SHALL carry a `[patch.crates-io]` entry for `bevy_render`: the checkout's vendored crate by path when bevy-ts's crates come from a checkout, and the bevy-ts git repository at the tag `v<bevy-ts version>` when they come from crates.io. The vendored crate SHALL carry a `PATCH.md` naming the changed file and lines, the reason, bevy#26151, and how to remove the patch once Bevy ships the fix.

#### Scenario: A fresh engine's first frames on macOS
- **WHEN** an engine starts on macOS with no pipeline cache and the scene needs pipelines not yet compiled
- **THEN** frames keep presenting while the pipelines compile in the background, and each mesh draws once its pipeline is ready, instead of the window freezing until all are compiled

#### Scenario: A project engine from a checkout
- **WHEN** bevy-ts generates `.bevy-ts/engine/Cargo.toml` with bevy-ts's crates taken from a checkout
- **THEN** the manifest holds `[patch.crates-io]` with `bevy_render` at the path of that checkout's `vendor/bevy_render`

#### Scenario: A project engine from crates.io
- **WHEN** bevy-ts generates `.bevy-ts/engine/Cargo.toml` with bevy-ts's crates taken from crates.io
- **THEN** the manifest holds `[patch.crates-io]` with `bevy_render` from the bevy-ts git repository at tag `v<bevy-ts version>`

#### Scenario: An engine cached before the patch
- **WHEN** a project's local or shared cache holds an engine generated before the manifest carried the patch
- **THEN** its hash no longer matches and bevy-ts builds a patched engine instead of reusing it
