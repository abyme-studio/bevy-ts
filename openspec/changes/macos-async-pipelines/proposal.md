## Why

A fresh engine on macOS freezes 33-91 s on its first render. bevy_render 0.20 compiles render pipelines on a background task everywhere except macOS: there a `target_os = "macos"` cfg sends every pipeline through `block_on` on the render thread (`src/render_resource/pipeline_cache.rs:1058-1090`). shader-compile-stall found it (2026-10-23, reported as bevy#26151): with the cfg dropped, all 45 arena pipelines are ready in 6-26 s cold and under 1 s warm, and frames take a few ms meanwhile. The operator decided (2026-10-23) to fix it in our engine and not wait on an upstream PR.

## What Changes

- **A vendored bevy_render.** `vendor/bevy_render/` holds bevy_render at exactly the version in `Cargo.lock` (0.20.0), with one change: both `create_pipeline_task` cfgs lose their `target_os = "macos"` term, so macOS takes the async path other desktop platforms take. Nothing else differs from the crates.io source.
- **Wired by `[patch.crates-io]`** in the workspace `Cargo.toml`, and excluded from the workspace so `--workspace` builds, tests and lints never take it as a member.
- **`vendor/bevy_render/PATCH.md`** (the crate keeps upstream's `README.md`): the file and line, why, bevy#26151, and how to drop the patch when Bevy ships the fix.
- **Every engine gets it.** The prebuilt CLI and a shipped game's prebuilt engine build from the workspace. A project engine's generated `Cargo.toml` carries its own `[patch.crates-io]`, since it is a workspace of its own: a path to the checkout's `vendor/bevy_render` when built from a checkout, the bevy-ts git repository at the release tag when built from crates.io. A test checks the generated manifest carries it.
- **Docs** say the patch exists, why, and that a native crate's own `cargo` commands outside bevy-ts do not see it.
- **Measured.** Arena cold and warm, 3 runs each, before and after, recorded in this change.

## Capabilities

### New Capabilities

None.

### Modified Capabilities

- `project-engine`: every engine bevy-ts builds or ships, prebuilt or project, compiles render pipelines off the render thread on macOS; the generated engine manifest carries the patch.

## Impact

- `vendor/bevy_render/` (new): the patched crate and its `PATCH.md`.
- `Cargo.toml`: `[patch.crates-io] bevy_render`, `exclude` gains `vendor/bevy_render`. `Cargo.lock` re-resolves bevy_render to the path source.
- `crates/bevy-ts-cli/src/engine.rs`: `Plan::manifest()` writes the patch for both `Source`s; `GENERATOR` goes to `"2"` so cached engines built without it miss; a unit test.
- `docs/native.md` (project engines), `docs/build.md` (the shipped engine): the patch and why.
- Unchanged, and why: `.github/workflows/release.yml` builds `-p bevy-ts-cli` in the workspace, so npm's engines get the patch from `Cargo.toml`; `crates/bevy-ts-rapier3d` and `examples/thrusters/native` are only ever built inside a generated engine; `bench/*` crates are benchmarks, not engines, and stay unpatched.
- Needs the operator's go: a project engine built from crates.io fetches the patch over git from the public `abyme-studio/bevy-ts` repository at tag `v<version>`. That is a new place a user's build reaches at build time. It is dormant until bevy-ts's crates are on crates.io.
