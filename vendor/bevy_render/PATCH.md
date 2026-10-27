# bevy-ts's patch to bevy_render

This is bevy_render 0.20.0 as published on crates.io, with two changes. The workspace `Cargo.toml`
and every generated project engine (`crates/bevy-ts-cli/src/engine.rs`, `Source::patch`) use it
through `[patch.crates-io]`.

## Async pipelines on macOS

`src/render_resource/pipeline_cache.rs`, lines 1058-1090, the two `create_pipeline_task`s: the
first cfg loses `not(target_os = "macos")`, the second loses `target_os = "macos"`. So macOS spawns
each pipeline's compile on `AsyncComputeTaskPool`, as other multi-threaded desktop targets do,
instead of `block_on` on the render thread. A caller that asks for a synchronous compile still
gets one.

`diff -r ~/.cargo/registry/src/*/bevy_render-0.20.0/src src` shows those two lines and the
stable bin order below, nothing else.

## Why

On macOS every pipeline compiled on the render thread, one after another, so a fresh engine froze
on its first render until all of them were done. Reported upstream as bevy#26151. The arena,
headless on an M5 Max, before and after:

Each cell: time from engine start to all 46 pipelines ready / longest render frame meanwhile (load average at the run).

| Arm | Run 1 | Run 2 | Run 3 |
|---|---|---|---|
| before, cold | 10.95 s / 7.74 s (38) | 11.25 s / 7.73 s (37) | 14.60 s / 10.97 s (41) |
| before, warm | 53.75 s / 43.70 s (94) | 4.58 s / 2.58 s (76) | 2.44 s / 1.41 s (68) |
| after, cold | 4.33 s / 95 ms (22) | 4.05 s / 107 ms (23) | 4.59 s / 103 ms (28) |
| after, warm | 4.59 s / 120 ms (29) | 1.60 s / 283 ms (27) | 1.60 s / 409 ms (27) |

The method is in `openspec/changes/archive/*-macos-async-pipelines/design.md`, Measurements.

## Stable bin order

`src/render_phase/mod.rs` adds the resource `StableBinOrder` and `RenderBin::sort`;
`src/batching/mod.rs`'s `sort_binned_render_phase` takes `Option<Res<StableBinOrder>>` and, while
it is there, sorts each batchable bin's entities by `MainEntity` (only when they are not sorted
already). Without it nothing changes, so an app that never inserts it pays nothing.
`bevy-ts-remote`'s `ReadyPlugin` keeps it in the render world under `FixedStep`.

Why: under `NoIndirectDrawing` (which `FixedStep` gives every camera) a batchable bin draws its
instances in the order they entered the bin, an `IndexMap` in insertion order. An entity whose
pipeline was ready on its first frame is queued in sorted visible order; one that was not comes
back a frame later from a `HashSet` of pending queues, in hash order. Which happens is timing, so
which of two instances at one depth is drawn last changed between runs: the `bevy-ts-cli` test
`stacked_instances_at_one_depth_are_the_same_every_shot` failed on CI's rust job (352701 of 2073600
bytes). `a_bin_draws_in_entity_order_with_stable_bin_order` tests the patch.

## A Bevy bump

Cargo ignores a `[patch]` whose version no longer matches the lock, with only a warning, so the
fix would silently go. The `bevy-ts-cli` test `vendored_bevy_render_matches_the_locked_bevy` fails
instead. On a bump:

1. If the new Bevy compiles pipelines async on macOS (bevy#26151 fixed) and sorts a bin's
   entities, drop the patch (below); if only one, keep the other.
2. Otherwise copy the new crate from `~/.cargo/registry/src/*/bevy_render-<version>/`
   (`Cargo.toml`, `LICENSE-*`, `README.md`, `src/`) over this folder, keeping this file, commit it
   unedited, re-apply the two cfg edits and the stable bin order, and `cargo update -p bevy_render`.

## Dropping the patch

Delete `vendor/bevy_render`; remove `bevy_render` from the workspace `Cargo.toml`'s
`[patch.crates-io]` and `vendor/bevy_render` from its `exclude`; remove `Source::patch` and the
`[patch.crates-io]` lines from `Plan::manifest()` in `crates/bevy-ts-cli/src/engine.rs` with their
tests; bump `GENERATOR`; `cargo update -p bevy_render`; drop the notes in `docs/native.md`,
`docs/build.md` and `docs/release.md`.
