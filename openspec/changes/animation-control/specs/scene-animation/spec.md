## MODIFIED Requirements

### Requirement: PlayAnimation names a clip for a scene
The engine SHALL register a reflected Rust component `PlayAnimation` with `ReflectComponent` and `ReflectDefault`, and these fields: `clip: Handle<AnimationClip>` (default: none), `repeat: RepeatAnimation` (Bevy's enum, default `Forever`), `speed: f32` (default `1.0`), `transition_secs: f32` (default `0.25`), `paused: bool` (default `false`), `cull: bool` (default `false`) and `graph: Handle<AnimationGraph>` (default: none; see the graph requirement). It SHALL be tiered by the native-components rules: its plain fields (`speed`, `transition_secs`, `paused`, `cull`) are columns TS systems read and write, and `clip`, `repeat` and `graph` are holes TS columns never see. It SHALL be named `PlayAnimation` in `bevy-ts/bevy`, never inserted from bytes, and written through reflection, `clip`, `repeat` and `graph` included, wherever it is placed: a TSX component element (its type listing every field as a prop), a level patch and the editor. `clip` SHALL be written as an asset path with a label (`"models/Fox.glb#Animation2"`) and loaded as a `Handle<AnimationClip>`. `repeat` SHALL be written as Bevy names its variants: `"Never"`, `"Forever"` or `{ Count: n }` in TSX, `Never`, `Forever` or `Count(n)` in a level. It SHALL go on the entity that carries `WorldAssetRoot`, or on any other entity, which then is its own player (see the clip requirement).

#### Scenario: Placed in TSX
- **WHEN** a scene describes `<entity><WorldAssetRoot value="models/Fox.glb#Scene0" /><PlayAnimation clip="models/Fox.glb#Animation2" /></entity>`
- **THEN** the entity carries `PlayAnimation` with that clip handle, `repeat` `Forever`, `speed` 1, `transition_secs` 0.25, `paused` false, `cull` false and no `graph`

#### Scenario: Placed from a level
- **WHEN** a level file patches an entity with `PlayAnimation { clip: "models/Fox.glb#Animation1", repeat: Count(3) }` beside `WorldAssetRoot("models/Fox.glb#Scene0")`
- **THEN** the fox plays its walk clip three times after the level loads, with no TS involved

#### Scenario: The old bool is refused
- **WHEN** a TSX element writes `<PlayAnimation clip="models/Fox.glb#Animation2" repeat={true} />`
- **THEN** `tsc` reports the prop's type, and the element's reflected write fails naming `repeat` and `RepeatAnimation`

### Requirement: The clip plays once the scene is ready
When a `WorldAssetRoot`'s instance is ready (`WorldInstanceReady` on the root) and the root carries `PlayAnimation`, Rust SHALL find every `AnimationPlayer` among the root's descendants, insert on each an `AnimationGraphHandle` to the engine's shared graph and an `AnimationTransitions`, and play the clip's node with `repeat` as its `RepeatAnimation`, at `speed`. A root that is `paused` SHALL start paused and parked. When `PlayAnimation` is added to a root whose instance is already ready, it SHALL play the same way that frame. When the scene is spawned again (its asset reloaded), it SHALL play again on the new players. Playing SHALL make no call into JS.

#### Scenario: A fox runs
- **WHEN** a root has `WorldAssetRoot("models/Fox.glb#Scene0")` and `PlayAnimation { clip: "models/Fox.glb#Animation2" }`
- **THEN** after the scene is ready, its `AnimationPlayer` is playing the run clip on repeat at speed 1

#### Scenario: Added after ready
- **WHEN** a fox's scene is ready with no `PlayAnimation`, and `PlayAnimation` is then inserted on its root
- **THEN** its player starts the clip in the frame the insert is applied

#### Scenario: Many foxes, one graph
- **WHEN** 100 roots play the same clip
- **THEN** all their players share one `AnimationGraph` asset with one node for that clip

#### Scenario: Spawned paused
- **WHEN** a root has `PlayAnimation { clip: "models/Fox.glb#Animation2", paused: true }` and its scene becomes ready
- **THEN** its player holds the run clip at seek time 0, paused, with no `AnimationGraphHandle`

### Requirement: Changing PlayAnimation changes what plays
A change to `clip` SHALL cross-fade every player under the root from the current clip to the new one over `transition_secs` seconds, through `AnimationTransitions`. A change to `speed` or `repeat` SHALL apply to the clip already playing, without restarting it. `speed` SHALL be written as Bevy's `set_speed` and nothing else: speed 0 holds the pose and does not park. A change to `paused` or `cull` SHALL park or wake the root (see the parking requirement), without restarting the clip. Removing `PlayAnimation` SHALL stop every animation on those players and give back any graph handle parking removed (the root's graph handle). Each distinct clip SHALL get one node in the shared graph, added the first time any root names it.

#### Scenario: Run to walk
- **WHEN** a TSX binding changes a running fox's `clip` from `#Animation2` to `#Animation1` with `transition_secs` 0.25
- **THEN** over the next 0.25 seconds the run's weight falls to 0 and the walk's rises to 1, and the walk then repeats

#### Scenario: Speed only
- **WHEN** `speed` changes from 1 to 2 on a fox mid-cycle
- **THEN** the same clip keeps playing from where it was, twice as fast

#### Scenario: Speed 0 is not a pause
- **WHEN** `speed` is set to 0 on a playing fox
- **THEN** its player keeps its `AnimationGraphHandle` and the clip is not paused

#### Scenario: Count changed mid-play
- **WHEN** `repeat` changes from `Forever` to `Count(2)` on a fox that has completed the clip 3 times
- **THEN** the clip is finished, as Bevy's `ActiveAnimation::is_finished` says for 3 completions of a count of 2

## ADDED Requirements

### Requirement: A paused or culled root is parked
A root SHALL be parked while its `PlayAnimation` has `paused` true, or has `cull` true and none of its recorded meshes has `ViewVisibility` true. Parking SHALL remove `AnimationGraphHandle` from each player under the root and keep its `AnimationPlayer` and `AnimationTransitions`, so Bevy's `animate_targets` writes no bone of it and transform propagation skips its skeleton. Waking SHALL insert the root's graph handle again (its `graph`, else the shared graph's); the clip SHALL go on from the seek time, weights and transitions it had. `paused` SHALL also `pause_all` the players, and its clearing `resume_all`, so Bevy's own pause state matches. A parked clip's time SHALL stand still. The meshes SHALL be recorded when the root's players are found: every entity under the root carrying `Mesh3d`. A root with `cull` true and no meshes SHALL never be parked by `cull`. The park decision SHALL run in `PostUpdate` before `AnimationSystems`, reading the `ViewVisibility` of the previous frame, so a root waking shows one frame of its parked pose. Parking SHALL add nothing per frame for a root with `paused` and `cull` both false.

#### Scenario: Paused costs nothing
- **WHEN** a playing fox's `paused` is set to true
- **THEN** its player has no `AnimationGraphHandle`, its bones' `Transform`s are not changed on the next frames, and `is_paused` is true

#### Scenario: Resume where it stood
- **WHEN** a fox paused at seek time 0.6 s is unpaused
- **THEN** its player has the shared graph's handle again and the clip plays on from 0.6 s

#### Scenario: Off screen with cull
- **WHEN** a fox with `cull: true` leaves every camera's view
- **THEN** the frame after its meshes' `ViewVisibility` turns false its player has no `AnimationGraphHandle`, and the frame after one turns true it has the handle again

#### Scenario: Off screen without cull
- **WHEN** a fox with `cull: false` leaves every camera's view
- **THEN** its player keeps its `AnimationGraphHandle` and plays on

#### Scenario: Paused while culled
- **WHEN** a culled fox with `cull: true` comes into view while `paused` is true
- **THEN** it stays parked until `paused` is false

### Requirement: Animations reads and drives a root's clip from TS
Every engine SHALL export a param `Animations` from module `bevy-ts/animation`, through the exported-param mechanism (native-functions), keyed by the root entity that carries `PlayAnimation`. A root names its clip by asset path; no `AnimationNodeIndex`, and no part of `SharedGraph`'s clip-to-node map, SHALL reach TS. `status(root: Entity): AnimationStatus | undefined` SHALL return, for the root's clip as its first recorded player has it: `loaded` (the clip asset is loaded), `duration` (the clip's, in seconds; 0 until loaded), `seekTime`, `elapsed`, `completions`, `justCompleted`, `finished`, `paused`, `parked` and `weight`. It SHALL return `undefined` for an entity that is not a root with `PlayAnimation`, or whose scene has no player yet. Once writable-native-params lands, `seek(root, secs)` SHALL call Bevy's `ActiveAnimation::seek_to` on the root's clip on every player under it (so events between fire on the next update), `rewind(root)` its `rewind`, and `replay(root)` its `replay`; each SHALL return false, changing nothing, when `status` would be `undefined`. A seek on a parked root SHALL move the clip; the pose SHALL show it once the root wakes.

#### Scenario: Reading a fox's progress
- **WHEN** a TS system takes `anims: Animations` and calls `anims.status(fox)` on a fox playing `#Animation2` for 0.5 s at speed 1
- **THEN** it gets `loaded` true, the clip's duration, `seekTime` near 0.5, `paused` false and `parked` false

#### Scenario: Not a root
- **WHEN** a system calls `anims.status(e)` on an entity with no `PlayAnimation`
- **THEN** it gets `undefined`

#### Scenario: Start offset
- **WHEN** a system calls `anims.seek(villager, 0.7)` on the frame the villager's scene is ready
- **THEN** every player under it plays on from 0.7 s, and villagers seeked to different times idle out of step

#### Scenario: Seek on a non-root
- **WHEN** a system calls `anims.seek(e, 1)` on an entity with no `PlayAnimation`
- **THEN** it returns false and nothing changes

### Requirement: A root plays an AnimationGraph built in TS
`bevy-ts/bevy` SHALL export `AnimationGraph` as a builder with Bevy's names and shape: `new AnimationGraph()`, the static `fromClip(clip)` and `fromClips(clips)`, the field `root`, and the methods `addClip(clip, weight, parent)`, `addClipWithMask(clip, mask, weight, parent)`, `addClips(clips, weight, parent)`, `addBlend(weight, parent)`, `addBlendWithMask(mask, weight, parent)`, `addAdditiveBlend(weight, parent)`, `addAdditiveBlendWithMask(mask, weight, parent)`, `addEdge(from, to)`, `removeEdge(from, to)`, `get(node)`, `nodes()` and `addTargetToMaskGroup(target, group)`. A clip SHALL be an asset path with a label (`"models/Fox.glb#Animation0"`). Each `add*` SHALL return the new node's number in this graph, as Bevy's returns its `AnimationNodeIndex`; it SHALL mean nothing outside the graph value. `get(node)` SHALL return a view whose `weight` and `mask` are writable and which has `addMask`, `removeMask`, `addMaskGroup` and `removeMaskGroup`. A target SHALL be `AnimationTargetId.fromNames(names)`, `fromName(name)` or a UUID string; Rust SHALL compute the id with Bevy's `from_names`. Building SHALL make no host call. A built graph SHALL be an asset value: it goes in any `Handle<AnimationGraph>` slot, and into `assets.add`, `assets.get` and `assets.set`, by the reflected-assets rules.

Rust SHALL hand-map `AnimationGraph` in the reflected-asset writer and reader, whose reflection leaves `graph` opaque: write takes Bevy's `SerializedAnimationGraph` shape, with `mask_groups` as `[target, mask]` pairs, and loads each clip by path; read gives the same shape, with targets as UUID strings, and fails naming the node for a clip with no path. A write then a read SHALL give the value written, targets as UUIDs.

A root whose `PlayAnimation` has a `graph` SHALL give its players that graph's handle in place of the shared graph's, and play every clip node in it, with `repeat` and `speed`; node weights and masks blend them as Bevy does. `graph` SHALL also take a `.animgraph.ron` asset path. `clip` and `transition_secs` SHALL be unused on such a root, and a root with both `clip` and `graph` SHALL log one warning and play the graph. A changed `graph` SHALL stop the old clips and play the new graph's, with no cross-fade. A change to the graph asset (`assets.set`) SHALL apply from the next frame: weights and masks as Bevy reads them, a new clip node started, a removed one stopped. Clearing `graph` SHALL return the root to its `clip` in the shared graph. `Animations.status` SHALL read a graph root's first clip node, in node order; `seek`, `rewind` and `replay` SHALL act on every clip the root's players play. Parking SHALL hold for a graph root as for any root.

#### Scenario: Two clips blend
- **WHEN** a script builds `g = new AnimationGraph()`, `b = g.addBlend(1, g.root)`, `g.addClip("models/Fox.glb#Animation0", 0.5, b)`, `g.addClip("models/Fox.glb#Animation1", 0.5, b)`, and a fox has `<PlayAnimation graph={g} />`
- **THEN** its player has a graph handle that is not the shared graph's, plays both clips, and each bone's pose lies between the two clips' poses

#### Scenario: An additive layer
- **WHEN** a graph has `addClip(run, 1, g.root)` and `a = g.addAdditiveBlend(1, g.root)` holding `addClip(wave, 1, a)`
- **THEN** the root plays both, and the bones the wave clip animates differ from the run alone

#### Scenario: A masked bone stays still
- **WHEN** a graph calls `g.addTargetToMaskGroup(AnimationTargetId.fromNames(path), 0)`, `path` being the names from the fox's animation root down to one bone, and adds its only clip with `addClipWithMask(clip, 1, 1, g.root)`
- **THEN** that bone's `Transform` does not change while the others animate

#### Scenario: Weights change live
- **WHEN** a TS system calls `assets.set(ref, ...)` with the blend's first clip's weight 1 and the second's 0, on a fox playing that graph
- **THEN** from the next frame the fox's pose is the first clip's alone, and no clip restarts

#### Scenario: A graph file
- **WHEN** a level patches a fox with `PlayAnimation { graph: "anims/fox_blend.animgraph.ron" }`
- **THEN** the fox plays every clip node in that file, with no TS involved

#### Scenario: Clip and graph both set
- **WHEN** a root has `clip` `#Animation2` and a `graph`
- **THEN** it plays the graph, and one warning names the root and both fields

#### Scenario: A built graph round-trips
- **WHEN** a script calls `assets.add(g)` and then `assets.get(ref)`
- **THEN** it reads the same nodes, edges, weights, masks and root, with each target as a UUID string

#### Scenario: The hole check passes
- **WHEN** the reflected-coverage hole check walks `AnimationGraph`
- **THEN** `graph` round-trips, and `reflect_value::UNCOVERED` has no entry for `AnimationGraph`

### Requirement: Any entity plays an AnimationClip built in TS
`bevy-ts/bevy` SHALL export `AnimationClip` as a builder with Bevy's names: `new AnimationClip()`, `addCurveToTarget(target, curve)`, `setDuration(secs)` and `duration`. A curve SHALL be `new AnimatableCurve(animatedField(Component, field), curve)`, `animatedField` standing for Bevy's `animated_field!`: any reflected, mutable component and one of its top-level fields whose type is `Animatable`; `tsc` SHALL reject an unknown field. `curve` SHALL be built from Bevy's constructors, by their names: `EasingCurve(start, end, ease)` with every `EaseFunction` variant, `ConstantCurve`, `SampleAutoCurve`, `UnevenSampleAutoCurve`, `AnimatableKeyframeCurve`, `SteppedKeyframeCurve`, `CubicKeyframeCurve`, `CubicRotationCurve`, and `toCurve()` or `toCurveCyclic()` of `CubicBezier`, `CubicHermite`, `CubicCardinalSpline`, `CubicBSpline`, `CubicNurbs` and `LinearSpline`; and from the adaptors `reparametrizeLinear`, `reverse`, `repeat`, `forever`, `pingPong`, `chain` and `chainContinue`. A target SHALL be as in the graph requirement, plus `AnimationTargetId.root`. Building SHALL make no host call; a built clip SHALL be an asset value, and go in `PlayAnimation.clip`, a graph's `addClip` and `assets.add`.

Rust SHALL hand-map `AnimationClip` in the reflected-asset writer and reader: write builds Bevy's curves with Bevy's `add_curve_to_target`, so the clip's duration is the longest curve's domain end unless `duration` is given, and fails, naming the curve, on an unknown component or field, a value of the wrong type, or an unbounded curve with no `duration`. Read SHALL give back the JSON a script wrote, and `duration` with `curves: null` for a clip from a file; writing `curves: null` SHALL keep the clip's curves.

`PlayAnimation` on an entity with no `WorldAssetRoot` SHALL play the frame it is added. When that entity has no `AnimationPlayer` under it, Rust SHALL insert one on the entity, and give the entity (with the empty path) and every descendant with a `Name` and no `AnimationTargetId` a target id from its name path below it, and `AnimatedBy` the entity. Everything `PlayAnimation` does on a scene root (`repeat`, `speed`, `paused`, `cull`, `graph`, cross-fades, `Animations`) SHALL hold for it. A curve on a field the target lacks SHALL be skipped, as Bevy skips it. An animated `MaterialAlpha.alpha` SHALL be drawn the frame it is sampled.

#### Scenario: Translate with an ease
- **WHEN** a TSX entity has `<Transform /><PlayAnimation clip={clip} repeat="Never" />`, `clip` holding `AnimatableCurve(animatedField(Transform, "translation"), new EasingCurve([0, 0, 0], [0, 2, 0], "CubicOut").reparametrizeLinear(interval(0, 0.5)))` on `AnimationTargetId.root`
- **THEN** at 0.25 s its `translation.y` is the cubic-out ease of 0.5 times 2, at 0.5 s it is 2 and stays 2, and no TS ran after the insert

#### Scenario: Rotate
- **WHEN** the curve is `EasingCurve([0, 0, 0, 1], [0, sin(PI/4), 0, cos(PI/4)], "Linear").reparametrizeLinear(interval(0, 1))` (identity to a quarter turn about Y, as `[x, y, z, w]`) on `Transform` `rotation`
- **THEN** each frame's rotation is Bevy's own curve sampled at the seek time, and at 0.5 s the entity has turned an eighth of a turn about Y

#### Scenario: Scale, back and forth
- **WHEN** the curve is `EasingCurve([1, 1, 1], [2, 2, 2], "SineInOut").reparametrizeLinear(interval(0, 0.4)).pingPong()` on `Transform` `scale`
- **THEN** the scale reaches 2 at 0.4 s and is back at 1 at 0.8 s

#### Scenario: Fade out
- **WHEN** a mesh entity with `MaterialAlpha` plays a curve `EasingCurve(1, 0, "QuadraticIn").reparametrizeLinear(interval(0, 1))` on `MaterialAlpha` `alpha`
- **THEN** its drawn material's alpha follows the curve, the same frame it is sampled, and is 0 at 1 s

#### Scenario: A named child
- **WHEN** a TSX entity with `PlayAnimation` has a child `<entity><Name value="lid" /><Transform /></entity>`, and its clip targets `AnimationTargetId.fromNames(["lid"])`
- **THEN** the child moves and the parent does not

#### Scenario: A wrong field
- **WHEN** a script writes a clip with `animatedField(Transform, "translation")` over a curve of `f32`
- **THEN** the write fails naming `Transform::translation`, `Vec3` and `f32`

#### Scenario: A paused tween parks
- **WHEN** a tweening entity's `paused` is set to true
- **THEN** its player has no `AnimationGraphHandle` and its `Transform` stops changing

#### Scenario: A script clip round-trips
- **WHEN** a script calls `assets.add(clip)` and then `assets.get(ref)`
- **THEN** it reads the curves it wrote, and writing that back gives an equal clip

### Requirement: Every animation control has a stated TS path
Every public field and method of Bevy 0.20's `AnimationPlayer`, `ActiveAnimation`, `AnimationTransitions`, `AnimationGraph`, `AnimationGraphNode`, `AnimationTargetId`, `AnimatedBy` and `AnimationClip`, the graph's loader and serialized types, `AnimatableCurve` and its property types, and `bevy_curve`'s curve constructors, eases and adaptors, SHALL be listed below with its TS name, or the reason it stays in Rust, and the test that proves it. Raw `AnimationPlayer` and `AnimationTransitions` stay reflect-tier components as they are today; no TS name SHALL take an index into the shared graph; a TS-built graph's own node numbers are the only node indices TS sees (see the graph requirement). A new public item in a Bevy upgrade SHALL be added to this table in the change that takes the upgrade.

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
| `AnimationGraph::from_clip`, `from_clips` | `AnimationGraph.fromClip`, `fromClips` (return the graph and its node or nodes) | `animation-graph.test.ts` |
| `AnimationGraph::add_clip`, `add_clip_with_mask`, `add_clips` | `addClip`, `addClipWithMask`, `addClips` | `graph.rs::a_graph_blends_two_clips`, `graph.rs::a_mask_keeps_a_bone_still` |
| `AnimationGraph::add_blend`, `add_blend_with_mask` | `addBlend`, `addBlendWithMask` | `graph.rs::a_graph_blends_two_clips` |
| `AnimationGraph::add_additive_blend`, `add_additive_blend_with_mask` | `addAdditiveBlend`, `addAdditiveBlendWithMask` | `graph.rs::an_additive_blend_adds` |
| `AnimationGraph::add_edge`, `remove_edge` | `addEdge`, `removeEdge` | `animation-graph.test.ts`, `graph.rs::an_edited_graph_plays_new_clips` |
| `AnimationGraph::get`, `get_mut`, `Index`, `IndexMut`, `nodes` | `get(node)` (a live node view), `nodes()` | `animation-graph.test.ts` |
| `AnimationGraph.root`, `.graph` | `root`; `graph` is the hand-mapped JSON the builder holds | `bevy-ts-animation/src/hand.rs::animation_graph_round_trips` |
| `AnimationGraph.mask_groups`, `add_target_to_mask_group` | `mask_groups`, `addTargetToMaskGroup(target, group)` | `graph.rs::a_mask_keeps_a_bone_still` |
| `AnimationGraphNode.node_type`, `weight`, `mask`, `add_mask`, `remove_mask`, `add_mask_group`, `remove_mask_group` | the node view's `node_type`, `weight`, `mask`, `addMask`, `removeMask`, `addMaskGroup`, `removeMaskGroup` | `animation-graph.test.ts`, `animation.rs::a_ts_graph_reweights_live` |
| `AnimationNodeType::Clip`, `Blend`, `Add` | `{ Clip: path }`, `"Blend"`, `"Add"` | `bevy-ts-animation/src/hand.rs::animation_graph_round_trips` |
| `AnimationTargetId::from_names`, `from_name`, its UUID | `AnimationTargetId.fromNames(names)`, `fromName(name)`, or a UUID string | `graph.rs::a_mask_keeps_a_bone_still` |
| `AnimationGraphAssetLoader` (`.animgraph.ron`, `.animgraph`) | `PlayAnimation.graph` as an asset path | `graph.rs::a_ron_graph_plays` |
| `SerializedAnimationGraph`, `SerializedAnimationGraphNode`, `SerializedAnimationNodeType` | Rust only: the shape the hand-map reads and writes | `bevy-ts-animation/src/hand.rs::animation_graph_round_trips` |
| `AnimationGraph::save` | Rust only: writes a file; `assets.get` reads the value | – |
| `ThreadedAnimationGraph`, `ThreadedAnimationGraphs`, `ThreadedAnimationSubgraph` | Rust only: Bevy's evaluation cache | – |
| `AnimationClip` (`Default`), `add_curve_to_target`, `duration`, `set_duration` | `new AnimationClip()`, `addCurveToTarget`, `duration`, `setDuration` | `clip.rs::a_tween_translates_with_an_ease`, `animation-clip.test.ts` |
| `AnimationClip::curves`, `curves_mut`, `curves_for_target`, `curves_for_target_mut`, `add_variable_curve_to_target`, `VariableCurve` | Rust only: boxed curves; `assets.get` reads a script clip's JSON | `bevy-ts-animation/src/hand.rs::animation_clip_round_trips` |
| `AnimationClip::add_event`, `add_event_to_target`, `add_event_fn`, `add_event_fn_to_target`, `AnimationEvent` | Rust only: a typed Rust event; TS cannot observe one yet | – |
| `AnimationClip::sample_clamped` | Rust only: a read of a typed property | – |
| `AnimatableCurve::new`, `animated_field!`, `AnimatedField` | `new AnimatableCurve(animatedField(C, field), curve)` | `clip.rs::a_tween_translates_with_an_ease`, `clip.rs::a_wrong_field_fails` |
| `AnimatableProperty`, `EvaluatorId::ComponentField` | Rust only: `ReflectedField`, Bevy's id | `clip.rs::a_script_rotation_blends_with_a_gltf_clip` |
| `EvaluatorId::Type`, `AnimationCurve`, `AnimationCurveEvaluator` | Rust only: custom properties are Rust | – |
| `Animatable` value types | the types `animatedField` accepts | `animation-clip.test.ts` |
| `EasingCurve::new`, `EaseFunction` (every variant), `JumpAt` | `new EasingCurve(start, end, ease)`, Bevy's variant names | `clip.rs::every_ease_samples_as_bevy` |
| `ConstantCurve`, `SampleAutoCurve`, `UnevenSampleAutoCurve`, `AnimatableKeyframeCurve` | constructors of the same names | `clip.rs::every_curve_samples_as_bevy` |
| `SteppedKeyframeCurve`, `CubicKeyframeCurve`, `CubicRotationCurve` | constructors of the same names | `clip.rs::every_curve_samples_as_bevy` |
| `WideLinearKeyframeCurve`, `WideSteppedKeyframeCurve`, `WideCubicKeyframeCurve`, `WeightsCurve` | Rust only: morph weights, iterable curves no field takes | – |
| `CubicBezier`, `CubicHermite`, `CubicCardinalSpline`, `CubicBSpline`, `CubicNurbs`, `LinearSpline`, `to_curve`, `to_curve_cyclic` | constructors of the same names, `toCurve`, `toCurveCyclic` | `clip.rs::every_curve_samples_as_bevy` |
| `Curve::reparametrize_linear`, `reverse`, `repeat`, `forever`, `ping_pong`, `chain`, `chain_continue`, `interval` | `reparametrizeLinear`, `reverse`, `repeat`, `forever`, `pingPong`, `chain`, `chainContinue`, `interval` | `clip.rs::every_curve_samples_as_bevy`, `clip.rs::scale_ping_pongs` |
| `FunctionCurve`, `Curve::map`, `reparametrize`, `reparametrize_by_curve`, `zip`, `graph`, `SampleCurve`, `UnevenSampleCurve`, `resample*` | Rust only: closures, or not an animatable value | – |
| `AnimationTargetId` on a plain entity, `AnimatedBy` | given by `PlayAnimation` (design 19); `AnimationTargetId.root` | `clip.rs::a_named_child_moves_alone` |
| `MaterialAlpha.alpha` as a curve target | `animatedField(MaterialAlpha, "alpha")` | `clip.rs::a_fade_draws_the_frame_it_samples` |

#### Scenario: The table is the coverage
- **WHEN** a reviewer reads `crates/bevy-ts-animation`, `docs/animation.md` and the table
- **THEN** every row's TS name exists and is documented, and every named test exists and passes
