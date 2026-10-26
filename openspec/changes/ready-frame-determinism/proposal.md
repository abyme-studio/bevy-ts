## Why

Agent town's civ hazards frame, shot with `BEVY_TS_SCREENSHOT`, comes out as one of three pictures:
the golden, (779,286) off by 25, or (624,295) off by 47, each with a halo of 1-step pixels from
bloom. Its learn frame comes out as one of two. `docs/build.md` promises the same picture at any
load.

Evidence (agent town civ-followups, 2026-10-27):

- Released alpha.32, alpha.33 and alpha.34 all flip: 6 of 6, 5 of 6 and 5 of 6 runs off the golden.
  It is not a regression of alpha.33.
- The clock is not the cause. A traced build logged every ready frame: 17 of 18 runs reached the
  shot with elapsed exactly 1.000 s, and the pixels still flipped between them.
- The same world draws two ways. With time frozen after the 60th ready frame, a second shot of
  the same run differed from the first at the same pixels in 5 of 8 runs, while no
  `GlobalTransform`, mesh, material, visibility or asset changed in 86 frames.
- Bevy's GPU culling orders instances by an atomic add (`bevy_pbr`'s `mesh_preprocess.wesl`,
  `batch_output_index = atomicAdd(...)`). The order of a batch's instances changes from frame to
  frame, and of two fragments at the same depth the one drawn last wins.
- With `NoIndirectDrawing` on every camera from its spawn, 16 shots of 8 runs were byte-identical,
  and within 5 pixels by 1 of the golden. Even civ's post noise (one channel step in a few pixels)
  went away.

The clock has a smaller hole too, as suspected: in 1 of 18 traced runs a ready frame came before
the script's 2995 loads were asked for, stepped time, and the shot's clock was 61 steps.

## What Changes

- Under `FixedStep` (`BEVY_TS_SCREENSHOT`, `BEVY_TS_FIXED_STEP=1`), `ReadyPlugin` gives every camera
  Bevy's `NoIndirectDrawing` when it is spawned. Added later, Bevy draws no mesh with it.
- While a `ShotWhenReady` waits, `FixedStep` time stops at the shot's `at` steps, so the shot's
  clock never counts a ready frame that came before a frame that was not.
- `bevy-ts/screenshot` without the fixed step keeps Bevy's GPU culling: its shots can differ in
  such pixels, as docs/build.md says.

Turning GPU preprocessing off whole (`PbrPlugin::use_gpu_instance_buffer_builder: false`) was
tried and rejected: in Bevy 0.20 it draws no mesh at all, in a plain Bevy app too.

## Capabilities

### Modified Capabilities

- `headless-run`: the env shot's clock stops at its frame; cameras draw without GPU culling.
- `remote-protocol`: `BEVY_TS_FIXED_STEP=1` draws as `BEVY_TS_SCREENSHOT` does.

## Impact

`crates/bevy-ts-remote/src/screenshot.rs`, `crates/bevy-ts-cli/tests/it/screenshot_ready.rs`,
`docs/build.md`. Only fixed-step runs change: they cull on the CPU and draw each batch directly.
