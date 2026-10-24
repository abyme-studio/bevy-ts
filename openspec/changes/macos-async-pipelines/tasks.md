Every cargo job runs through `~/dev/abyme/scripts/gate-slot run bevy-ts:macos-async-pipelines -- <cmd>`, one at a time. The first build waits until lead@bevy-ts lifts the current load freeze. Nothing runs on the operator's screen: arena runs `--headless`.

## 1. Before numbers

- [x] 1.1 Pick how "all pipelines ready" is read (a debug log when `PipelineCache` first has no pending pipelines, or an existing trace) and how cold is made (a fresh engine build, or the Metal shader cache cleared); write both into design.md's Measurements
- [x] 1.2 On main's engine, run `examples/arena` headless cold 3 times and warm 3 times; record time to all pipelines ready and the longest frame in design.md's table

## 2. The vendored crate

- [x] 2.1 Copy bevy_render 0.20.0 from the cargo registry into `vendor/bevy_render/` as published (`Cargo.toml`, `LICENSE-*`, `README.md`, `src/`); commit it unedited so the next commit is the whole diff
- [x] 2.2 Edit `src/render_resource/pipeline_cache.rs` `create_pipeline_task`: drop `not(target_os = "macos")` from the first cfg and `target_os = "macos"` from the second; check `diff -r` against the registry copy shows only those two lines
- [x] 2.3 Add a `bevy-ts-cli` test that `vendor/bevy_render/Cargo.toml`'s version equals `BEVY_VERSION` (from `Cargo.lock`), so a Bevy bump without re-vendoring fails
- [x] 2.4 Write `vendor/bevy_render/PATCH.md` (the crate's own `README.md` stays upstream's): the file and lines 1058-1090, what changed and why, the measured numbers, bevy#26151, how to re-vendor on a Bevy bump, and how to drop the patch

## 3. Every engine

- [x] 3.1 Workspace `Cargo.toml`: `[patch.crates-io] bevy_render = { path = "vendor/bevy_render" }`; add `vendor/bevy_render` to `exclude`; `cargo update -p bevy_render` and check `Cargo.lock` resolves bevy_render with no `source` (path) and no `[[patch.unused]]`. This covers the prebuilt CLI, the release workflow's `cargo build --release -p bevy-ts-cli`, and a game folder's copied engine
- [x] 3.2 `crates/bevy-ts-cli/src/engine.rs` `Source`: a `patch()` giving the `bevy_render` patch line, path `<root>/vendor/bevy_render` for `Path`, git `https://github.com/abyme-studio/bevy-ts` tag `v{VERSION}` for `Registry`
- [x] 3.3 `Plan::manifest()`: append `[patch.crates-io]` with that line; bump `GENERATOR` to `"2"`
- [x] 3.4 Unit tests beside `manifest` (around `engine.rs:831`): the generated manifest carries the `bevy_render` patch for a `Path` source (pointing into the checkout) and for `Registry` (git, tag `v{VERSION}`); `cargo metadata` on a generated engine for `examples/thrusters` resolves bevy_render to the vendored path
- [x] 3.5 `bevy-ts build examples/thrusters` (a project engine, release) and `bevy-ts build examples/cube` (prebuilt) still produce game folders that play headless

## 4. Docs

- [x] 4.1 `docs/native.md`, Project engines: the generated engine patches bevy_render (why, bevy#26151, link to `vendor/bevy_render/PATCH.md`); a native crate's own `cargo` commands outside bevy-ts build unpatched Bevy, which is harmless
- [x] 4.2 `docs/build.md`, The folder: the shipped engine, prebuilt or project, carries the patch
- [x] 4.3 `README.md` or `docs/release.md` wherever a Bevy bump is described: a bump re-vendors bevy_render (link to PATCH.md)

## 5. After numbers and the gate

- [x] 5.1 On this branch's engine, run arena headless cold 3 times and warm 3 times the same way as 1.2; record them in design.md's table and in `PATCH.md`; note any validation error, crash or missing draw
- [x] 5.2 Gate: `cargo build --workspace --all-targets`, `cargo test --workspace` (viewport with `--test-threads=1`), `cargo clippy --workspace --all-targets -- -D warnings`, each through gate-slot
- [x] 5.3 `openspec validate macos-async-pipelines --strict`
