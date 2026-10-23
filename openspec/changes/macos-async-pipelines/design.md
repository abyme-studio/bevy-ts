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
- `Source::Registry`: `bevy_render = { git = "https://github.com/abyme-studio/bevy-ts", tag = "v<VERSION>" }`. Cargo finds the package by name inside the repository. Alternatives: publishing the patched crate under another name (a crates.io package we maintain, and a renamed patch); writing the vendored source out of the CLI binary (1.6 MB embedded). The git tag needs nothing new published and is pinned by the version already in the hash. It is dormant until bevy-ts's crates are on crates.io (`docs/release.md`); the release workflow already tags `v<version>`.

**`GENERATOR` goes to `"2"`.** The manifest changed shape, so cached engines (local and shared) built without the patch miss. Under `Source::Path`, edits to the vendored crate already change the hash through the CLI binary's stamp, since the CLI links the same crate.

**The project's lock.** `generate()` seeds `.bevy-ts/engine/Cargo.lock` from the native crate's lock, which resolves bevy_render from crates.io. Cargo re-resolves the patched package on the next build; `check_lock` checks `bevy` and `bevy-ts-native` only, so it is unaffected.

## Risks / Trade-offs

- [Upstream kept macOS synchronous for a reason we have not found] → the measurement runs arena on macOS cold and warm and watches for crashes, validation errors and wrong draws; the shader-compile-stall run saw none. Dropping the patch is one revert.
- [A native crate's own `cargo check`/`cargo test`, outside bevy-ts, builds unpatched bevy_render] → harmless (no engine runs from it); `docs/native.md` says so.
- [A Bevy bump silently ignores the patch] → the version check (task 2.3) fails the build.
- [Registry engines fetch from GitHub at build time; offline or GitHub down fails a first build] → cargo caches git sources like registry ones; the shared engine cache still serves authors without a toolchain. Needs the operator's go before it ships.
- [Vendored source bloats the repo by ~1.6 MB] → accepted; it is one crate.

## Migration Plan

Lands with the code. Existing project engines rebuild once (the `GENERATOR` bump). Rollback: revert the commit; engines rebuild unpatched on their next hash miss.

## Measurements

Filled by task 5: arena on the operator's Mac, cold (no pipeline or shader cache) and warm, 3 runs each, before and after. Time from engine start to all pipelines ready, the longest frame meanwhile.

| Arm | Run 1 | Run 2 | Run 3 |
|---|---|---|---|
| before, cold | | | |
| before, warm | | | |
| after, cold | | | |
| after, warm | | | |

## Open Questions

- The git source for registry-built engines needs the operator's go (proposal, Impact).
