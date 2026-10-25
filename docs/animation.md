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
| `graph: Handle<AnimationGraph>` | none | a graph of the root's own, played in place of `clip` ([below](#blending-clips-a-graph)) |
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
  is. A root with no mesh is never culled. A root plays its first frame before `cull` judges it,
  since Bevy has not yet worked out which of its new meshes are in view. Off by default.
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

`seek(root, secs)`, `rewind(root)` and `replay(root)` drive the root's clip on every player under
it: Bevy's `seek_to` (events between the old time and the new fire on the next update), `rewind`
(back to 0) and `replay` (back to no time elapsed and no completions). Each returns false, and
changes nothing, where `status` is `undefined`. A seek on a parked root moves its clip; the pose
shows it once the root wakes. Seeking each villager to a different time once its clip has loaded
starts them out of step:

```ts
const started = new Set<Entity>();
export function offsets(anims: Animations, villagers: Query<[Entity, PlayAnimation]>) {
  villagers.each((v) => {
    const s = anims.status(v);
    if (s?.loaded && !started.has(v)) {
      started.add(v);
      anims.seek(v, Math.random() * s.duration);
    }
  });
}
```

## Blending clips: a graph

A root with a `graph` plays every clip node of that `AnimationGraph`, blended by the graph's
weights and masks, in place of `clip`. `bevy-ts/bevy`'s `AnimationGraph` is Bevy's builder,
camelCased, building the graph as a TS value with no host call:

```tsx
import { AnimationGraph, AnimationTargetId, PlayAnimation, WorldAssetRoot } from "bevy-ts/bevy";

const g = new AnimationGraph();
const blend = g.addBlend(1, g.root);
const walk = g.addClip("models/Fox.glb#Animation1", 0.5, blend);
g.addClip("models/Fox.glb#Animation2", 0.5, blend);

<entity name="fox">
  <WorldAssetRoot value="models/Fox.glb#Scene0" />
  <PlayAnimation graph={g} />
</entity>
```

- Each `add*` returns the new node's number in this graph, as Bevy returns its
  `AnimationNodeIndex`; the next `add*` takes it as `parent`. The numbers mean nothing outside the
  graph value. `addBlend` normalizes its children's weights; `addAdditiveBlend` adds each child,
  scaled by its weight, onto what is below it. `addClips`, the `*WithMask` forms, `addEdge`,
  `removeEdge`, `nodes()` and the statics `AnimationGraph.fromClip(clip)` and `fromClips(clips)`
  (each returning the graph and its node or nodes) are Bevy's.
- `g.get(n)` is a live view of a node: its `weight` and `mask` write through, and it has
  `addMask`, `removeMask`, `addMaskGroup` and `removeMaskGroup`.
- A clip is an asset path with a label, or an `AnimationClip` ref or value.
- The graph is an asset value: it goes in `graph={g}`, `assets.add(g)` and `assets.set(ref, g)`
  ([assets](assets.md)). `new AnimationGraph()` builds an `AnimationGraphBuilder`; the type
  `AnimationGraph` is the JSON Rust reads and writes, which `assets.get(ref)` returns.
- `graph` also takes a `.animgraph.ron` path, Bevy's graph file
  (`graph="anims/fox_blend.animgraph.ron"`, or a level's `PlayAnimation { graph: "..." }`), with no
  TS involved.

Rust gives the root's players that graph's handle in place of the shared graph's, and plays every
clip node with the root's `repeat` and `speed`. `clip` and `transition_secs` go unused: a root with
both `clip` and `graph` warns once and plays the graph. A changed `graph` stops the old clips and
plays the new graph's from 0, with no cross-fade; clearing it returns the root to its `clip`.
Parking, `cull` and `Animations` hold as for any root: `status` reads the graph's first clip node,
and `seek`, `rewind` and `replay` act on every clip it plays.

**Live edits.** Bevy reads a graph's weights and masks from the asset each frame, so a TS system
reweights by editing its builder and setting the asset; no clip restarts. A new clip node starts,
and a node that is no longer a clip stops:

```ts
const ref = assets.add(g);  // <PlayAnimation graph={ref} />
export function lean(anims: Animations) {
  g.get(walk)!.weight = 0.2;
  assets.set(ref, g);
}
```

Each `set` is a reflected write and a rebuild of Bevy's evaluation cache for that graph, so a set
every frame costs that every frame: set when the weight changes.

**Masks.** A mask group is a set of bones; a node whose `mask` has a group's bit animates none of
that group's bones. Bevy keys a bone by a hash of the names from the animation root down to it,
which TS cannot compute, so TS names the bone and Rust hashes it with Bevy's `from_names`:

```ts
const leg = AnimationTargetId.fromNames(["root", "_rootJoint", "b_Root_00", "b_Hip_01", "b_LeftLeg01_015"]);
g.addTargetToMaskGroup(leg, 0);
g.addClipWithMask("models/Fox.glb#Animation2", 0b1, 1, g.root);  // runs, leg still
```

A target is `AnimationTargetId.fromNames(names)`, `fromName(name)`, `AnimationTargetId.root` (the
animation root itself), or a UUID string; reads give UUIDs. Masks are numbers, exact up to group
52 (Bevy allows 64).

The graph's JSON is Bevy's `SerializedAnimationGraph` (the `.animgraph.ron` shape) with
`mask_groups` as `[target, mask]` pairs: `{ graph: { nodes: [{ node_type, mask, weight }],
node_holes: [], edge_property: "directed", edges: [[from, to, null]] }, root, mask_groups }`, a
`node_type` being `"Blend"`, `"Add"` or `{ Clip: path }`. `assets.get(ref)` reads it back; a
clip with no path and no ref fails naming its node.

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
  (`Query<[Mut<PlayAnimation>]>`); `clip` and `graph` hold handles and `repeat` an enum with
  data, so only TSX, a level or the editor set them.

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

Every public item of Bevy 0.20's `AnimationPlayer`, `ActiveAnimation`, `AnimationTransitions`,
`AnimationGraph`, `AnimationGraphNode` and `AnimationTargetId`, and the graph's loader and
serialized types. No TS name takes an index into the shared graph; a TS-built graph's own node
numbers are the only node indices TS sees.

| Bevy item | TS name | Test |
| --- | --- | --- |
| `AnimationPlayer::play`, `AnimationTransitions::play` | `PlayAnimation.clip`, `transition_secs` | `play.rs::a_new_clip_cross_fades`, `animation.rs::a_tsx_fox_plays_and_its_bound_clip_cross_fades` |
| `AnimationPlayer::stop`, `stop_all` | removing `PlayAnimation` | `play.rs::removing_it_stops_the_players` |
| `AnimationPlayer::start`, `ActiveAnimation::replay` | `Animations.replay` | `animation.rs::animations_seek_rewind_replay` |
| `ActiveAnimation::pause`, `resume`, `is_paused`, `AnimationPlayer::pause_all`, `resume_all`, `all_paused` | `PlayAnimation.paused`; `status().paused` | `park.rs::paused_parks_and_resumes_where_it_stood`, `animation.rs::animations_status` |
| `ActiveAnimation::speed`, `set_speed`, `is_playback_reversed` | `PlayAnimation.speed` (negative reverses) | `play.rs::speed_applies_in_place`, `park.rs::speed_zero_does_not_park` |
| `AnimationPlayer::adjust_speeds` | Rust only: `speed` sets the one clip a root plays | – |
| `ActiveAnimation::repeat`, `set_repeat`, `repeat_mode` | `PlayAnimation.repeat` | `play.rs::repeat_count_finishes`, `animation.rs::a_level_fox_plays_with_no_ts` |
| `ActiveAnimation::seek_to`, `rewind`, `AnimationPlayer::rewind_all` | `Animations.seek`, `rewind` | `animation.rs::animations_seek_rewind_replay` |
| `ActiveAnimation::set_seek_time`, `AnimationPlayer::seek_all_by` | Rust only: `seek` is the one seek, Bevy's event-firing `seek_to` | – |
| `ActiveAnimation::seek_time`, `elapsed`, `completions`, `just_completed`, `is_finished`, `AnimationPlayer::all_finished` | `status()` fields | `animation.rs::animations_status` |
| `ActiveAnimation::last_seek_time` | Rust only: an internal of event firing | – |
| `ActiveAnimation::weight` | `status().weight` | `animation.rs::animations_status` |
| `ActiveAnimation::set_weight` | Rust only: `AnimationTransitions` owns weights during a fade | – |
| `AnimationPlayer::is_playing_animation`, `animation`, `animation_mut`, `playing_animations`, `playing_animations_mut` | Rust only: node-keyed; `status()` reads the root's clip | – |
| `AnimationTransitions::new`, `get_main_animation`, its `transitions` | Rust only: node-keyed; `status().weight` shows a fade | – |
| `AnimationGraphHandle` on a player | `PlayAnimation.graph` chooses it; parking removes it | `park.rs::culled_roots_park_off_screen`, `graph.rs::a_parked_graph_root_wakes_with_its_graph` |
| `AnimationClip::duration` | `status().duration`, `loaded` | `animation.rs::animations_status` |
| `AnimationGraph::new`, `Default` | `new AnimationGraph()` | `animation-graph.test.ts` |
| `AnimationGraph::from_clip`, `from_clips` | `AnimationGraph.fromClip`, `fromClips` (the graph and its node or nodes) | `animation-graph.test.ts` |
| `AnimationGraph::add_clip`, `add_clip_with_mask`, `add_clips` | `addClip`, `addClipWithMask`, `addClips` | `graph.rs::a_graph_blends_two_clips`, `graph.rs::a_mask_keeps_a_bone_still`, `animation.rs::a_ts_built_graph_blends` |
| `AnimationGraph::add_blend`, `add_blend_with_mask` | `addBlend`, `addBlendWithMask` | `graph.rs::a_graph_blends_two_clips` |
| `AnimationGraph::add_additive_blend`, `add_additive_blend_with_mask` | `addAdditiveBlend`, `addAdditiveBlendWithMask` | `graph.rs::an_additive_blend_adds` |
| `AnimationGraph::add_edge`, `remove_edge` | `addEdge`, `removeEdge` | `animation-graph.test.ts`, `graph.rs::an_edited_graph_plays_new_clips` |
| `AnimationGraph::get`, `get_mut`, `Index`, `IndexMut`, `nodes` | `get(node)` (a live node view), `nodes()` | `animation-graph.test.ts` |
| `AnimationGraph.root`, `.graph` | `root`; `graph` is the hand-mapped JSON the builder holds | `reflect_asset/animation.rs::animation_graph_round_trips` |
| `AnimationGraph.mask_groups`, `add_target_to_mask_group` | `mask_groups`, `addTargetToMaskGroup(target, group)` | `graph.rs::a_mask_keeps_a_bone_still` |
| `AnimationGraphNode.node_type`, `weight`, `mask`, `add_mask`, `remove_mask`, `add_mask_group`, `remove_mask_group` | the node view's `node_type`, `weight`, `mask`, `addMask`, `removeMask`, `addMaskGroup`, `removeMaskGroup` | `animation-graph.test.ts`, `animation.rs::a_ts_graph_reweights_live` |
| `AnimationNodeType::Clip`, `Blend`, `Add` | `{ Clip: path }`, `"Blend"`, `"Add"` | `reflect_asset/animation.rs::animation_graph_round_trips` |
| `AnimationTargetId::from_names`, `from_name`, its UUID | `AnimationTargetId.fromNames(names)`, `fromName(name)`, or a UUID string | `graph.rs::a_mask_keeps_a_bone_still`, `animation-graph.test.ts` |
| `AnimationGraphAssetLoader` (`.animgraph.ron`, `.animgraph`) | `PlayAnimation.graph` as an asset path | `graph.rs::a_ron_graph_plays` |
| `SerializedAnimationGraph`, `SerializedAnimationGraphNode`, `SerializedAnimationNodeType` | Rust only: the shape the hand-map reads and writes | `reflect_asset/animation.rs::animation_graph_round_trips` |
| `AnimationGraph::save` | Rust only: writes a file; `assets.get` reads the value | – |
| `ThreadedAnimationGraph`, `ThreadedAnimationGraphs`, `ThreadedAnimationSubgraph` | Rust only: Bevy's evaluation cache | – |

## When to write Rust instead

Events at a clip's end, and retargeting onto another file's skeleton, are Rust in the project's
native crate, as in Bevy ([docs/native.md](native.md)). Blends and masks are a graph (above).

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

### Paused and culled (change `animation-control`)

The fox bench at N = 400 (`ARMS="B G P H K" scripts/animation-bench.sh 3 400`, process
`animation-control-bench`): TSX foxes playing (B), the same with every player's
`AnimationGraphHandle` removed once all play (G, the floor), `paused` (P), the camera turned away
(H) and turned away with `cull` (K). 3 runs per arm, interleaved, niced, on a shared machine; mean
± standard deviation over runs.

| Arm | CPU | Update mean | Update p95 | Game GPU p50 |
| --- | --- | --- | --- | --- |
| B, playing | 256 ± 13 % | 4.27 ± 0.64 ms | 8.10 ms | 3.32 ms |
| G, no graph (floor) | 180 ± 14 % | 3.04 ± 0.69 ms | 4.80 ms | 3.18 ms |
| P, `paused` | 177 ± 9 % | 2.61 ± 0.28 ms | 3.21 ms | 3.11 ms |
| H, off screen | 234 ± 10 % | 3.53 ± 0.30 ms | 5.27 ms | 1.47 ms |
| K, off screen with `cull` | 176 ± 3 % | 2.92 ± 0.35 ms | 4.02 ms | 1.46 ms |

`paused` and `cull` cost what the floor costs: about 80 CPU points and 1.3 ms of update less than
playing. Off screen alone saves only GPU.

Agent town's 100 villagers (each 102 joints, a 258-channel idle) at the fit, every villager
spawned, headless 960x540, niced, 8 runs of 300 frames per arm
(`node bench/animation-control/villagers.mjs target/release/bevy-ts`, process
`animation-control-villagers`, agent town b9bf91c patched in a temp copy). Frame time is paced
by the headless window, so CPU is the number that moves.

| Arm | CPU | Frame |
| --- | --- | --- |
| 0 villagers | 54.0 ± 6.6 % | 20.72 ± 0.81 ms |
| 100 playing | 102.9 ± 22.1 % | 21.28 ± 2.17 ms |
| 100 at `speed` 0 | 108.4 ± 21.1 % | 22.50 ± 2.88 ms |
| 100 `paused` | 64.7 ± 6.9 % | 20.78 ± 0.88 ms |
| 100 hidden (`Visibility::Hidden`) | 103.4 ± 23.1 % | 20.72 ± 1.15 ms |
| 100 hidden with `cull` | 58.5 ± 8.6 % | 20.44 ± 0.34 ms |

Speed 0 and hiding cost what playing does; `paused` and `cull` bring 100 villagers to within about
5 to 10 points of none.
