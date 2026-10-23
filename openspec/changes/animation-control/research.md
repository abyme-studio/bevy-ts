Recommendation: bevy-ts-animation parks a root's players (removes their `AnimationGraphHandle`, keeps
`AnimationPlayer`) while `PlayAnimation` is paused or none of the root's meshes is in any view,
and puts the handle back to resume; pause and speed 0 do not save anything.

# Animation cost when paused or hidden

Research for `animation-control`, 2026-10-23. Bevy 0.20.0 (the version `Cargo.toml` pins), source
in `~/.cargo/registry/src/*/bevy_animation-0.20.0`, `bevy_transform-0.20.0`, `bevy_pbr-0.20.0`.

## 1. What each control does per frame

`AnimationPlugin` runs, in `PostUpdate` before transform propagation: `advance_transitions`,
`advance_animations`, `animate_targets`, `trigger_untargeted_animation_events`,
`expire_completed_transitions` (`bevy_animation/src/lib.rs:1322`).

`animate_targets` iterates every bone (every entity with `AnimationTargetId` + `AnimatedBy`), not
every player. Per bone it looks up its player; then for each active clip it samples every curve
and `commit_all` writes the result through `get_mut`. That write marks `Transform` changed even when
the value is the same. `ActiveAnimation::paused` only skips events (`lib.rs:1241`) and the time
advance (`lib.rs:1073`); the curves are still sampled at the same `seek_time` and written.

| Control | Curves sampled | `Transform` written (changed) | Propagation | Resume |
| --- | --- | --- | --- | --- |
| playing | yes | yes | every bone | — |
| `set_speed(0)` | yes | yes | every bone | set speed; exact |
| `pause` / `pause_all` | yes | yes | every bone | `resume_all`; exact |
| `stop` / `stop_all` | no | no | skipped (static) | `start`/`transitions.play`: restarts at 0, no fade from the old clip |
| remove `AnimationGraphHandle` | no (bone returns at its player lookup, `lib.rs:1115`) | no | skipped (static) | insert the handle again; seek time, weights, transitions kept |
| remove `AnimationPlayer` (+ graph, transitions) | no | no | skipped | re-insert and replay; state lost |

With `stop_all` the per-bone walk still runs, but finds no active clip and writes nothing. With the
graph handle removed the walk returns at the first lookup. `advance_transitions` (no graph needed)
still runs over every player and would fade a parked cross-fade; the remove/insert costs one
archetype move per player each way.

Propagation: `StaticTransformOptimizations` is on by default (`bevy_transform/src/systems.rs:89`).
`mark_dirty_trees` flags ancestors of changed `Transform`s, and propagation skips any subtree whose
tree flag and parent `GlobalTransform` are unchanged (`systems.rs:708`). So a skeleton nobody
writes costs nothing in propagation; one written every frame (pause, speed 0) is walked whole.
`GlobalTransform` is set with `set_if_neq`, so a paused skeleton's joints do not re-upload.

Skinning already culls: `extract_skins` allocates and fills joint matrices only for skins whose
`ViewVisibility` is true, and only joints whose `GlobalTransform` changed (`bevy_pbr/src/render/skin.rs:272`).
So hidden villagers cost CPU in `animate_targets` and propagation, not in skin upload.

## 2. Upstream

No visibility-based animation culling in Bevy 0.20, and no open issue or PR found for it
(`gh search` over bevyengine/bevy for "animation culling", "offscreen animation", "AnimationPlayer
performance", "animation LOD": no match). Nearby: #21837 (merged) per-joint bounds for frustum
culling skinned meshes, which makes `ViewVisibility` trustworthy for animated skins; #20717 (open)
lets a player use another `Time`, via a query filter. Confidence: medium (search, not an exhaustive read).

Other engines do it as an option on the animator: Unity's `AnimatorCullingMode.CullUpdateTransforms`
skips transform writes while no renderer is visible, `CullCompletely` stops the animator
([Unity docs](https://docs.unity3d.com/ScriptReference/AnimatorCullingMode.CullUpdateTransforms.html)).

The Bevy-idiomatic candidate: no run condition (the systems are Bevy's and run over all players),
but a component the system queries for: `animate_targets` and `advance_animations` require
`AnimationGraphHandle`, so removing it is the off switch Bevy already has. A parked off-screen root
wakes the frame after its `ViewVisibility` turns true, so it shows one frame of a stale pose, as
Unity's mode does.

## 3. Numbers

Agent town, `villager-anim` (build@slice-2's bench, npm engine 0.1.0-alpha.14, headless 960x540,
niced, 8 runs, 300 frames each). One villager: 1 skin, 102 joints, idle clip of 258 channels.

| Arm | CPU % | Frame ms |
| --- | --- | --- |
| 0 villagers | 55.9 ± 3.0 | 20.29 ± 0.38 |
| 100 at the fit, off-screen ones at speed 0 | 100.1 ± 6.2 | 20.04 ± 0.51 |
| close on r0, most off-screen at speed 0 | 104.8 ± 12.6 | 19.70 ± 0.52 |
| all far: all hidden and speed 0 | 107.0 ± 10.2 | 20.27 ± 0.90 |
| all 100 at speed 1 (0 vs 100) | 51.5 ± 3.4 vs 96.1 ± 8.5 | 20.62 vs 20.23 |
| near-only spawn: 0 / fit / close / far | 54.7 / 63.7 / 71.2 / 59.5 | ~21 |

Hidden plus speed 0 costs the same as playing. Spawning only near, in-view villagers removes most of
the cost, so what is left is per live skeleton.

Mechanism, measured on Bevy's fox in bevy-ts (`crates/bevy-ts-cli/examples/animation_bench_engine.rs`
arm A, the `many_foxes` wiring, with a throwaway `ANIMATION_BENCH_MODE` applied once all players
play; not committed). 400 foxes, about the bone count of 100 villagers; release engine, game
viewport at 120 Hz, 1200 sampled updates, 3 runs per mode interleaved, machine at load ~35, so
read the gaps, not the levels. Update ms is the main world's `First` to `Last`.

| Mode | Update mean ms | Update p95 ms | GPU p50 ms | CPU % |
| --- | --- | --- | --- | --- |
| `play` | 3.95 ± 0.26 | 5.86 ± 1.17 | 3.24 ± 0.07 | 259 ± 14 |
| `pause` (`pause_all`) | 3.80 ± 0.23 | 5.39 ± 1.01 | 3.23 ± 0.11 | 248 ± 13 |
| `stop` (`stop_all`) | 3.04 ± 0.30 | 3.99 ± 0.68 | 3.20 ± 0.06 | 186 ± 9 |
| `nograph` (remove `AnimationGraphHandle`) | 2.84 ± 0.23 | 3.67 ± 0.52 | 3.21 ± 0.11 | 180 ± 2 |
| `hidden` (`Visibility::Hidden` on roots) | 3.44 ± 0.20 | 4.27 ± 0.68 | 2.25 ± 0.10 | 230 ± 7 |
| `hidden` + `nograph` | 2.94 ± 0.70 | 4.80 ± 2.15 | 2.36 ± 0.34 | 172 ± 18 |

Pause saves nothing measurable. Stop and removing the graph handle save about 1 ms of update and
75 CPU points, the same amount; the handle keeps the clip's state for an exact resume. Hiding saves
only GPU (skins not uploaded, meshes not drawn) and about 0.5 ms; with the handle removed too it
saves both. At 100 foxes (about 24 joints each) the modes were within noise.

## 4. Coverage table draft

TS reaches a reflect-tier component today only from TSX, a level, the protocol and filters
(docs/native.md), never as query data. Two coming paths:

- reflected: reflect-coverage stage 3 (`Query<[Mut<AnimationPlayer>]>`, needs the operator's go)
  makes the reflected fields readable and writable from a system, through the generic
  `reflect_value` reader and writer. reflect-assets is the asset half of the same path; these are
  components, not assets.
- native: writable-native-params lets a native crate export `Query<&mut AnimationPlayer>` so a
  method with logic can be called.

`AnimationNodeIndex` is the catch: `active_animations` is a map keyed by node index and
`main_animation` is typed `unknown`, so TS needs the clip → node map (`SharedGraph`) before any
per-clip write means anything. The proposed TS names below hang off `PlayAnimation` (per root, the
way agent town uses it) and reach Bevy through Rust in bevy-ts-animation, with the raw fields left
to the reflected path.

| Bevy item | Kind | Proposed TS | Reaches TS by |
| --- | --- | --- | --- |
| `ActiveAnimation.paused`, `pause`/`resume`, `AnimationPlayer::pause_all`/`resume_all`/`all_paused` | field, methods | `PlayAnimation.paused` | PlayAnimation column (Rust applies, and parks) |
| `ActiveAnimation.speed`, `set_speed`, `is_playback_reversed`, `adjust_speeds` | field, methods | `PlayAnimation.speed` (exists) | column (exists) |
| `ActiveAnimation.repeat` (`RepeatAnimation::Never/Count(n)/Forever`), `set_repeat`, `repeat`, `repeat_mode` | field, methods | `PlayAnimation.repeat` (bool today; `Count(n)` missing) | column today; enum via reflected |
| `ActiveAnimation.weight`, `set_weight` | field | `AnimationPlayer.active_animations[n].weight` | reflected |
| `ActiveAnimation.seek_time`, `set_seek_time` | field | `PlayAnimation.seek` or raw field | reflected (raw); `seek_to` needs `last_seek_time` kept, writes both |
| `seek_to`, `rewind`, `rewind_all`, `seek_all_by` | methods | `seek(t, { events })`, `rewind()` | native, or two reflected field writes |
| `ActiveAnimation.elapsed`, `completions`, `just_completed`, `last_seek_time`, `is_finished`, `all_finished` | fields (read) | read-only fields; `finished` | reflected read |
| `replay`, `AnimationPlayer::start` | methods | `replay()` | native (resets five fields; reflected map insert also works) |
| `AnimationPlayer::play`, `stop`, `stop_all`, `is_playing_animation`, `animation(_mut)`, `playing_animations(_mut)` | methods over `active_animations` | `PlayAnimation.clip` (exists), removal (exists) | column/TSX; raw map via reflected |
| `AnimationTransitions::play(player, node, fade)` | method | `PlayAnimation.clip` + `transition_secs` (exists) | Rust (exists) |
| `AnimationTransitions.main_animation`, `get_main_animation`, `.transitions[]` (`current_weight`, `weight_decline_per_sec`, `animation`) | fields | read-only | reflected read; node index needs the clip map |
| `AnimationGraphHandle` presence | component | (internal: parking) | Rust only |

## 5. What scene-animation landed, and what is shared with script-gltf

Landed (archive `2026-10-23-scene-animation`, `crates/bevy-ts-animation`): `PlayAnimation { clip,
repeat, speed, transition_secs }`, played on `WorldInstanceReady` with one `SharedGraph` and an
`AnimationTransitions` per player; a changed clip cross-fades, a changed speed/repeat applies in place,
removal `stop_all`s (already the cheap path). `AnimationTargets` records each root's players.
`players`/`players_in` is the one walk (the preview uses it). `docs/animation.md` documents it.

To settle with lead@script-gltf (not settled here):

1. New fields on `PlayAnimation` (`paused`, maybe `seek`, a `Count(n)` repeat) and whether parking
   lives in `bevy-ts-animation` (their crate and spec `scene-animation`).
2. Changing `speed` 0 semantics: speed 0 could park too, or stay a plain write.
3. Automatic visibility parking: on by default, or opt-in (`PlayAnimation.cull`), and whether it is
   per root (all meshes not in view) or per player.
4. Exposing `SharedGraph`'s clip → node map to TS, which any raw `AnimationPlayer` write needs.
5. `docs/animation.md` and the fox bench (`animation_bench_engine.rs`), theirs, gain the new arms.
