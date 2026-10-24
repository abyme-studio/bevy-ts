## Context

bevy_render 0.20.0, `src/render_resource/pipeline_cache.rs:1058-1090`, has two `create_pipeline_task`s. The first, for `not(wasm32), not(macos), multi_threaded`, spawns the compile on `AsyncComputeTaskPool` unless the caller asked for `sync`. The second, for `wasm32 or macos or not(multi_threaded)`, always `block_on`s. So on macOS every pipeline compiles on the render thread, one after another, and the first render of a fresh engine stalls 33-91 s.

shader-compile-stall (2026-10-23) dropped the macOS term locally: all 45 arena pipelines ready in 6-26 s cold, under 1 s warm, frames a few ms throughout. Its record is a one-line report and bevy#26151; this change's own measurement (task 5) is the evidence of record. The operator decided to fix it in our engine, no upstream PR.

bevy-ts builds engines from three manifests:

- The workspace `Cargo.toml`: the CLI (`bevy-ts-cli`), which is the prebuilt engine. CI's release workflow builds it with `cargo build --release -p bevy-ts-cli`, npm ships it, and `bevy-ts build` copies it into a game folder for a project without Rust.
- `.bevy-ts/engine/Cargo.toml`, written by `Plan::manifest()` in `crates/bevy-ts-cli/src/engine.rs`: a project engine, for a project with a native crate, ports or adopted crates. It declares its own `[workspace]`, so the workspace's `[patch]` does not reach it. `bevy-ts build` ships its release build in a game folder.
- Crates with their own `[workspace]` (`crates/bevy-ts-rapier3d`, `examples/thrusters/native`, `bench/*`). The first two are only engines inside a generated engine. Benches are not engines.

`bevy-ts new` writes no `Cargo.toml`: a new project runs the prebuilt engine, or a project engine once it gains `native`.

## Goals / Non-Goals

**Goals:**
- Every engine bevy-ts builds or ships compiles pipelines async on macOS.
- The diff against upstream is the cfg terms and nothing else, so it is easy to audit and to drop.

**Non-Goals:**
- An upstream PR (bevy#26151 tracks it; not ours to drive).
- Changing pipeline behaviour on other platforms, or the `sync` path callers opt into.
- A pipeline cache on disk, or pre-warming pipelines.
- Patching `bench/*`.

## Decisions

**Vendor the whole crate under `vendor/bevy_render/`, patch by path.** Copy the crates.io 0.20.0 source as published (normalized `Cargo.toml`, licences, `src/`), then edit the two cfgs. Alternatives: a git fork of Bevy (a second repo to keep, and a `[patch]` to a git repo for every build); `cargo-patch`/a build-time diff (a new tool and build step). A vendored copy is plain Cargo and reviewable as one diff.

**Exclude it from the workspace.** A path dependency inside the workspace root becomes a member, so `cargo test --workspace` and `cargo clippy --workspace -D warnings` would build and lint Bevy's crate. `exclude = [..., "vendor/bevy_render"]` keeps it out, and `[profile.dev.package."*"] opt-level = 3` still optimises it as a non-member.

**The diff.** First cfg: `all(not(target_arch = "wasm32"), feature = "multi_threaded")`. Second: `any(target_arch = "wasm32", not(feature = "multi_threaded"))`. bevy-ts already enables `multi_threaded` (`crates/bevy-ts/Cargo.toml`). `vendor/bevy_render/PATCH.md` names the file, lines 1058-1090, the reason and bevy#26151, and how to drop it: delete `vendor/bevy_render`, the `[patch]` and `exclude` entries, the manifest's patch lines in `engine.rs`, bump `GENERATOR`, and `cargo update -p bevy_render`.

**A Bevy bump must re-vendor.** A `[patch]` whose version no longer matches what the lock wants is ignored with only a warning (`patch.unused`), which would silently drop the fix. `PATCH.md` says a Bevy bump copies the new version and re-applies the cfg edit, or deletes the patch if Bevy fixed it. Task 2.3 adds a check that the vendored version equals the lock's `bevy` version, so a bump without re-vendoring fails instead of warning.

**Project engines patch in their generated manifest.** `Plan::manifest()` appends `[patch.crates-io]` with `bevy_render` from the same `Source` the bevy-ts crates come from:
- `Source::Path(root)`: `bevy_render = { path = "<root>/vendor/bevy_render" }`.
- `Source::Registry`: `bevy_render = { git = "https://github.com/abyme-studio/bevy-ts", tag = "v<VERSION>" }`. Cargo finds the package by name inside the repository. Alternatives: publishing the patched crate under another name (a crates.io package we maintain, and a renamed patch); writing the vendored source out of the CLI binary (1.6 MB embedded). The git tag needs nothing new published and is pinned by the version already in the hash. It is dormant until bevy-ts's crates are on crates.io (`docs/release.md`); the release workflow already tags `v<version>`. Decided by lead@bevy-ts (2026-10-23): the git source, from the public `abyme-studio/bevy-ts` repository, pinned by tag `v<version>`. It goes, with the rest of the patch, once Bevy ships the async-on-macOS fix (bevy#26151).

**Thumbnails wait for their pipelines.** With pipelines compiling in the background, a mesh draws only once its pipeline is ready, so a capture a fixed number of frames after spawning can miss it. The asset viewer's thumbnail (`crates/bevy-ts-viewport/src/assets.rs`) reads back only once, for three updates in a row, no pipeline in the render world's `PipelineCache` is compiling (`Creating`), the cache holds no new pipeline, and the subject has no newly loaded mesh (a glTF part spawns and loads frames after its subject; it is reframed when it does), or after 30 s. A render-world system in `Cleanup` publishes that to the main world through a shared flag. `BEVY_TS_SCREENSHOT`, a walk hook at fixed frames 60 and 75, is unchanged and may now catch a mesh before its pipeline is ready on a cold cache; no test asserts on its pixels.

**Pixel tests compile synchronously.** `bevy-ts-viewport`'s tests, and `bevy-ts-cli`'s `headless_ui` and `ambient_light`, read pixels a set number of frames in. With background compiling a mesh, outline or gizmo draws a varying number of frames late, so they failed at random under load. Their apps set `RenderPlugin { synchronous_pipeline_compilation: true }`, Bevy's own switch for this, so they see what they always saw; they test the viewport, not compile timing. Waiting on the pipeline cache from the tests was tried and dropped: a mesh queues its pipeline frames after it spawns and a scene with no camera has none, so no idle test was both safe and finite.

**`GENERATOR` goes to `"2"`.** The manifest changed shape, so cached engines (local and shared) built without the patch miss. Under `Source::Path`, edits to the vendored crate already change the hash through the CLI binary's stamp, since the CLI links the same crate.

**The project's lock.** `generate()` seeds `.bevy-ts/engine/Cargo.lock` from the native crate's lock, which resolves bevy_render from crates.io. Cargo re-resolves the patched package on the next build; `check_lock` checks `bevy` and `bevy-ts-native` only, so it is unaffected.

## Risks / Trade-offs

- [Upstream kept macOS synchronous for a reason we have not found] → the measurement runs arena on macOS cold and warm and watches for crashes, validation errors and wrong draws; the shader-compile-stall run saw none. Dropping the patch is one revert.
- [A native crate's own `cargo check`/`cargo test`, outside bevy-ts, builds unpatched bevy_render] → harmless (no engine runs from it); `docs/native.md` says so.
- [A Bevy bump silently ignores the patch] → the version check (task 2.3) fails the build.
- [Registry engines fetch from GitHub at build time; offline or GitHub down fails a first build] → cargo caches git sources like registry ones; the shared engine cache still serves authors without a toolchain. Decided by lead@bevy-ts (2026-10-23).
- [Vendored source bloats the repo by ~1.6 MB] → accepted; it is one crate.

## Migration Plan

Lands with the code. Existing project engines rebuild once (the `GENERATOR` bump). Rollback: revert the commit; engines rebuild unpatched on their next hash miss.

## Measurements

Arena, headless, on the operator's M5 Max (macOS 26.4), the debug CLI (`opt-level = 1`, dependencies 3), 2026-10-23. `scripts/pipeline-bench.sh` (`pipelines-*` processes) builds the arena with each arm's CLI and plays it.

- All pipelines ready: `BEVY_TS_PIPELINE_TRACE=1` makes the engine print, from a render-world system after the render, each time the pipeline cache's queue empties: seconds since start, pipelines held, longest render frame so far. A run ends 15 s after its last line; that line is the cell.
- Cold: the run sits in a `sandbox-exec` profile that denies the user's Metal shader cache (`$(getconf DARWIN_USER_CACHE_DIR)com.apple.metal`), so every shader compiles afresh. Clearing that cache instead would have stalled every other process sharing it. Warm: the same binary straight after, cache allowed.
- The machine was shared with other teams' builds; load varied 22-94, so absolute times are noisy. The arms are far outside each other's noise, so 3 runs each stand.

Each cell: time from engine start to all 46 pipelines ready / longest render frame meanwhile (load average at the run).

| Arm | Run 1 | Run 2 | Run 3 |
|---|---|---|---|
| before, cold | 10.95 s / 7.74 s (38) | 11.25 s / 7.73 s (37) | 14.60 s / 10.97 s (41) |
| before, warm | 53.75 s / 43.70 s (94) | 4.58 s / 2.58 s (76) | 2.44 s / 1.41 s (68) |
| after, cold | 4.33 s / 95 ms (22) | 4.05 s / 107 ms (23) | 4.59 s / 103 ms (28) |
| after, warm | 4.59 s / 120 ms (29) | 1.60 s / 283 ms (27) | 1.60 s / 409 ms (27) |

Before, the render thread stalls 1.4-44 s while pipelines compile on it. After, no frame passes 0.41 s, and pipelines are ready sooner cold too (4.0-4.6 s against 11-15 s), since they compile in parallel. No run logged an error or panic; 46 pipelines in every run. Draws: the arena's lit-viewport checks (`play_after_edit`, ignored by default, run by hand) pass after. `bevy-ts-cli`'s `viewport_thumbnail` failed after: a thumbnail read back three frames after its subject spawned, before the subject's pipelines compiled, so the crate came out empty (Decisions, Thumbnails wait).

## Open Questions

None. The git source for registry-built engines was decided by lead@bevy-ts (Decisions).
