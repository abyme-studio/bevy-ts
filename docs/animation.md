# Animation

A glTF clip plays on a scene when its root carries `PlayAnimation` beside `WorldAssetRoot`. Rust
does the wiring; no JS runs to play, cross-fade or stop a clip.

```tsx
import { PlayAnimation, WorldAssetRoot } from "bevy-ts/bevy";

<entity name="fox">
  <WorldAssetRoot value="models/Fox.glb#Scene0" />
  <PlayAnimation clip="models/Fox.glb#Animation2" />
</entity>
```

`examples/foxes` is a grid of these, with a button that switches every fox's clip.

## Bevy's pattern, and how `PlayAnimation` maps onto it

Bevy plays a glTF clip with Rust (`examples/animation/animated_mesh.rs`, `many_foxes.rs`): spawn
`WorldAssetRoot`, observe `WorldInstanceReady` on the root, find the `AnimationPlayer` among its
descendants, insert an `AnimationGraphHandle` and `AnimationTransitions`, then
`transitions.play(&mut player, node, duration).repeat()`.

`bevy-ts-animation` does exactly that for every root with `PlayAnimation`:

| `PlayAnimation` | Default | What Rust does |
| --- | --- | --- |
| `clip: Handle<AnimationClip>` | none | an asset path with a label, `"models/Fox.glb#Animation2"`; a node in the shared graph |
| `repeat: RepeatAnimation` | `Forever` | `ActiveAnimation::set_repeat`: `Never`, `Count(n)` or `Forever` |
| `speed: f32` | `1.0` | `ActiveAnimation::set_speed`; negative plays backwards |
| `transition_secs: f32` | `0.25` | the cross-fade when `clip` changes, through `AnimationTransitions::play` |
| `paused: bool` | `false` | `pause_all`, and parks the root (below) |
| `cull: bool` | `false` | parks the root while none of its meshes is in any view |

- When the scene is ready (`WorldInstanceReady`), every `AnimationPlayer` under the root gets the
  shared graph and fresh transitions, and plays the clip. A root that gets `PlayAnimation` after
  its scene is ready plays in that frame. A reloaded scene plays again on its new players.
- A changed `clip` cross-fades over `transition_secs`. A changed `speed` or `repeat` applies to the
  playing clip without restarting it. Removing `PlayAnimation` stops the players.
- `repeat` is Bevy's enum. TSX writes `repeat="Never"`, `"Forever"` or `{ Count: 3 }`; a level
  writes `repeat: Count(3)`. The old `repeat={true}` is refused: drop it (`Forever` is the
  default), and write `repeat={false}` as `repeat="Never"`.
- Every player shares one `AnimationGraph`, with one node per distinct clip, added the first time
  a root names it, as `many_foxes` does. A hundred foxes cost one graph.
- The glTF file a clip comes from stays loaded from the first time a root names it. Bevy 0.20
  loads a file again for a label asked for before it is alive
  ([bevy#10549](https://github.com/bevyengine/bevy/issues/10549)), and a second load
  spawns every scene from that file again; with the file held, a clip named later (the button
  in `examples/foxes`) is already there. The file is never freed: memory is the whole file per
  distinct file a clip came from.

## Pausing and culling: parking

Bevy's own pause (`pause_all`) and `speed` 0 still sample every curve and write every bone each
frame, so a paused or held fox costs what a playing one does. A **parked** root costs nothing:
Rust removes `AnimationGraphHandle` from each of its players and keeps the `AnimationPlayer`, so
Bevy's `animate_targets` writes no bone and transform propagation skips the skeleton. Waking puts
the handle back, and the clip goes on from the seek time, weights and transitions it had.

- `paused: true` parks the root and calls `pause_all`, so Bevy's own state says paused too;
  `paused: false` resumes where it stood. A root spawned paused holds the clip at 0.
- `cull: true` parks the root while none of its meshes (every `Mesh3d` under it, recorded when
  its players are found; a TSX child mesh counts) is in any camera's view, and wakes it once one
  is. A root with no mesh is never culled. Off by default.
- A parked clip's time stands still: a culled villager that walks back into view goes on from
  where it left, not where it would be. A game that cares seeks on wake.
- The decision runs in `PostUpdate` before Bevy's animation, and reads the visibility Bevy
  computed the frame before, so a root coming into view shows one frame of its parked pose
  (Unity's `CullUpdateTransforms` does the same).
- A cross-fade under way when a root parks still finishes, since Bevy advances transitions
  without the graph; the root wakes on the new clip.
- `speed` 0 stays Bevy's: it holds the pose and still costs a playing clip. Use `paused`.
- Parking adds nothing for a root with `paused` and `cull` both false. Parking and waking each
  move every player once between archetypes, so a root flickering at a screen edge pays that on
  each change.

## Reading a clip from TS: `Animations`

Every engine exports `Animations` from `bevy-ts/animation`, keyed by the root that carries
`PlayAnimation`:

```ts
import type { Animations } from "bevy-ts/animation";

export function progress(anims: Animations, foxes: Query<[Entity, PlayAnimation]>) {
  foxes.each((fox) => {
    const s = anims.status(fox);
    if (s?.finished) console.log(`${fox} is done after ${s.completions} plays`);
  });
}
```

`status(root)` reads the root's clip as its first player has it: `loaded`, `duration` (0 until
loaded), `seekTime`, `elapsed`, `completions`, `justCompleted`, `finished`, `paused`, `parked`
and `weight`. It returns `undefined` for an entity that is not a `PlayAnimation` root, or whose
scene has no player yet. The root names its clip by path; no node index reaches TS.

`seek`, `rewind` and `replay` come with writable exported params (change
writable-native-params).

## Placing it

- **TSX**: as above. Every prop can be bound, so a signal switches the clip:
  `<PlayAnimation clip={() => clips[current()]} transition_secs={0.4} />`.
  `WorldAssetRoot` is a tuple struct of one field, so TSX gives it as `value`; any one-field
  tuple struct works the same way.
- **A level** places it like any reflected component:

  ```
  #Fox
  WorldAssetRoot("models/Fox.glb#Scene0")
  PlayAnimation { clip: "models/Fox.glb#Animation1" }
  ```

- **The editor** lists it among the components it can add, and edits its fields in the inspector.
- **A TS system** reads and writes `speed`, `transition_secs`, `paused` and `cull` as columns
  (`Query<[Mut<PlayAnimation>]>`); `clip` holds a handle and `repeat` an enum with data, so only
  TSX, a level or the editor set them.

A click on an animated model reaches a handler on its root: `onPointerClick` on the entity with
`WorldAssetRoot` fires for any of the scene's meshes ([docs/picking.md](picking.md), "Bubbling").

## Clips and skeletons

A clip moves the bones it names. It must come from the same glTF file as the scene, or from a
file with the same bone names. A clip whose names do not match moves nothing, and Bevy says
nothing.

Bevy's glTF loader adds an `AnimationPlayer` only where the file's own clips reach. A mesh-only
file has none, so `PlayAnimation` on it plays nothing; Rust warns once per root, naming the
entity, the scene and the clip. A clip path that fails to load (a wrong label) is logged once.

## Every control, and its TS path

Every public item of Bevy 0.20's `AnimationPlayer`, `ActiveAnimation` and `AnimationTransitions`.
No TS name takes a node index.

| Bevy item | TS name | Test |
| --- | --- | --- |
| `AnimationPlayer::play`, `AnimationTransitions::play` | `PlayAnimation.clip`, `transition_secs` | `play.rs::a_new_clip_cross_fades`, `animation.rs::a_tsx_fox_plays_and_its_bound_clip_cross_fades` |
| `AnimationPlayer::stop`, `stop_all` | removing `PlayAnimation` | `play.rs::removing_it_stops_the_players` |
| `AnimationPlayer::start`, `ActiveAnimation::replay` | `Animations.replay` (with writable-native-params) | `animation.rs::animations_seek_rewind_replay` |
| `ActiveAnimation::pause`, `resume`, `is_paused`, `AnimationPlayer::pause_all`, `resume_all`, `all_paused` | `PlayAnimation.paused`; `status().paused` | `park.rs::paused_parks_and_resumes_where_it_stood`, `animation.rs::animations_status` |
| `ActiveAnimation::speed`, `set_speed`, `is_playback_reversed` | `PlayAnimation.speed` (negative reverses) | `play.rs::speed_applies_in_place`, `park.rs::speed_zero_does_not_park` |
| `AnimationPlayer::adjust_speeds` | Rust only: `speed` sets the one clip a root plays | – |
| `ActiveAnimation::repeat`, `set_repeat`, `repeat_mode` | `PlayAnimation.repeat` | `play.rs::repeat_count_finishes`, `animation.rs::a_level_fox_plays_with_no_ts` |
| `ActiveAnimation::seek_to`, `rewind`, `AnimationPlayer::rewind_all` | `Animations.seek`, `rewind` (with writable-native-params) | `animation.rs::animations_seek_rewind_replay` |
| `ActiveAnimation::set_seek_time`, `AnimationPlayer::seek_all_by` | Rust only: `seek` is the one seek, Bevy's event-firing `seek_to` | – |
| `ActiveAnimation::seek_time`, `elapsed`, `completions`, `just_completed`, `is_finished`, `AnimationPlayer::all_finished` | `status()` fields | `animation.rs::animations_status` |
| `ActiveAnimation::last_seek_time` | Rust only: an internal of event firing | – |
| `ActiveAnimation::weight` | `status().weight` | `animation.rs::animations_status` |
| `ActiveAnimation::set_weight` | Rust only: `AnimationTransitions` owns weights during a fade | – |
| `AnimationPlayer::is_playing_animation`, `animation`, `animation_mut`, `playing_animations`, `playing_animations_mut` | Rust only: node-keyed; `status()` reads the root's clip | – |
| `AnimationTransitions::new`, `get_main_animation`, its `transitions` | Rust only: node-keyed; `status().weight` shows a fade | – |
| `AnimationGraphHandle` on a player | Rust only: parking | `park.rs::culled_roots_park_off_screen` |
| `AnimationClip::duration` | `status().duration`, `loaded` | `animation.rs::animations_status` |

## When to write Rust instead

`PlayAnimation` plays one clip at a time. Blend trees, masks, several clips at once, events at a
clip's end, or retargeting onto another file's skeleton are Rust in the project's native crate,
as in Bevy ([docs/native.md](native.md)).

## Cost

Measured by `scripts/animation-bench.sh` (`crates/bevy-ts-cli/tests/it/animation_bench.rs`): the
release engine, the game viewport ticked at 120 Hz, `Fox.glb`, one arm per run, 3 runs each.

- **A**: a Rust plugin spawning N foxes the `many_foxes` way, no TS.
- **B**: N foxes in TSX with `PlayAnimation`.
- **C**: the same N foxes, static (no `PlayAnimation`).
- **D** (a diagnostic, `ARMS="A D B"`): A's Rust-spawned foxes, played by `PlayAnimation`.

Medians over every run (12 to 17 per arm at N = 50 and 100, 3 at 200), Apple M5 Max, a machine
other builds shared; frame time is the main world's update, `First` to `Last`. Arms ran
interleaved, so each B run is compared with the A run just before it.

| N | Arm | Update mean | Update p95 | B ÷ A p95, paired | Ready (frames after spawn) |
| --- | --- | --- | --- | --- | --- |
| 50 | A, Rust | 3.70 ms | 7.09 ms | | 0 |
| 50 | B, TSX + `PlayAnimation` | 3.50 ms | 6.27 ms | 1.00 (12 rounds) | 0–1 |
| 50 | C, static | 3.45 ms | 6.19 ms | | – |
| 100 | A, Rust | 3.75 ms | 6.27 ms | | 0 |
| 100 | B, TSX + `PlayAnimation` | 3.85 ms | 7.12 ms | 1.01 (16 rounds) | 0–1 |
| 100 | C, static | 3.28 ms | 4.56 ms | | – |
| 100 | D, Rust + `PlayAnimation` | 3.80 ms | 7.16 ms | | 0 |
| 200 | A, Rust | 3.87 ms | 7.18 ms | | 0 |
| 200 | B, TSX + `PlayAnimation` | 3.91 ms | 7.47 ms | 0.99 (3 rounds) | 0–1 |
| 200 | C, static | 3.22 ms | 4.98 ms | | – |

The medians of B and A come from runs at different times; the paired column compares each B run
with the A run just before it, which is what the target is checked on. D ran in one set of 4
rounds beside A (A's p95 median in that set 7.02 ms).

B against the A run beside it, median ratio: p95 1.00 at 50, 1.01 at 100, 0.99 at 200; mean
1.00, 1.04, 1.00. The target (B's p95 within 10% of A's at 50 and 100, ready within one frame of
A's) holds. One run's p95 swings from 4.5 to 13 ms on a shared machine, so a few runs prove
nothing; pairs over many rounds do. A diagnostic arm D, Rust-spawned foxes played by
`PlayAnimation`, is level with A: the plugin does nothing per frame once a clip plays, and
playing costs what Bevy's own wiring costs. B is ready a frame after A at most, the frame
`PlayAnimation`'s system applies what the observer saw.
