## MODIFIED Requirements

### Requirement: PlayAnimation names a clip for a scene
The engine SHALL register a reflected Rust component `PlayAnimation` with `ReflectComponent` and `ReflectDefault`, and these fields: `clip: Handle<AnimationClip>` (default: none), `repeat: RepeatAnimation` (Bevy's enum, default `Forever`), `speed: f32` (default `1.0`), `transition_secs: f32` (default `0.25`), `paused: bool` (default `false`) and `cull: bool` (default `false`). It SHALL be tiered by the native-components rules: its plain fields (`speed`, `transition_secs`, `paused`, `cull`) are columns TS systems read and write, and `clip` and `repeat` are holes TS columns never see. It SHALL be named `PlayAnimation` in `bevy-ts/bevy`, never inserted from bytes, and written through reflection, `clip` and `repeat` included, wherever it is placed: a TSX component element (its type listing every field as a prop), a level patch and the editor. `clip` SHALL be written as an asset path with a label (`"models/Fox.glb#Animation2"`) and loaded as a `Handle<AnimationClip>`. `repeat` SHALL be written as Bevy names its variants: `"Never"`, `"Forever"` or `{ Count: n }` in TSX, `Never`, `Forever` or `Count(n)` in a level. It SHALL go on the entity that carries `WorldAssetRoot`.

#### Scenario: Placed in TSX
- **WHEN** a scene describes `<entity><WorldAssetRoot value="models/Fox.glb#Scene0" /><PlayAnimation clip="models/Fox.glb#Animation2" /></entity>`
- **THEN** the entity carries `PlayAnimation` with that clip handle, `repeat` `Forever`, `speed` 1, `transition_secs` 0.25, `paused` false and `cull` false

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
A change to `clip` SHALL cross-fade every player under the root from the current clip to the new one over `transition_secs` seconds, through `AnimationTransitions`. A change to `speed` or `repeat` SHALL apply to the clip already playing, without restarting it. `speed` SHALL be written as Bevy's `set_speed` and nothing else: speed 0 holds the pose and does not park. A change to `paused` or `cull` SHALL park or wake the root (see the parking requirement), without restarting the clip. Removing `PlayAnimation` SHALL stop every animation on those players and give back any graph handle parking removed. Each distinct clip SHALL get one node in the shared graph, added the first time any root names it.

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
A root SHALL be parked while its `PlayAnimation` has `paused` true, or has `cull` true and none of its recorded meshes has `ViewVisibility` true. Parking SHALL remove `AnimationGraphHandle` from each player under the root and keep its `AnimationPlayer` and `AnimationTransitions`, so Bevy's `animate_targets` writes no bone of it and transform propagation skips its skeleton. Waking SHALL insert the shared graph's handle again; the clip SHALL go on from the seek time, weights and transitions it had. `paused` SHALL also `pause_all` the players, and its clearing `resume_all`, so Bevy's own pause state matches. A parked clip's time SHALL stand still. The meshes SHALL be recorded when the root's players are found: every entity under the root carrying `Mesh3d`. A root with `cull` true and no meshes SHALL never be parked by `cull`. The park decision SHALL run in `PostUpdate` before `AnimationSystems`, reading the `ViewVisibility` of the previous frame, so a root waking shows one frame of its parked pose. Parking SHALL add nothing per frame for a root with `paused` and `cull` both false.

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

### Requirement: Every animation control has a stated TS path
Every public field and method of Bevy 0.20's `AnimationPlayer`, `ActiveAnimation` and `AnimationTransitions` SHALL be listed below with its TS name, or the reason it stays in Rust, and the test that proves it. Raw `AnimationPlayer` and `AnimationTransitions` stay reflect-tier components as they are today; no TS name SHALL take a node index. A new public item in a Bevy upgrade SHALL be added to this table in the change that takes the upgrade.

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
| `AnimationGraphHandle` on a player | Rust only: parking | `park.rs::culled_roots_park_off_screen` |
| `AnimationClip::duration` | `status().duration`, `loaded` | `animation.rs::animations_status` |

#### Scenario: The table is the coverage
- **WHEN** a reviewer reads `crates/bevy-ts-animation`, `docs/animation.md` and the table
- **THEN** every row's TS name exists and is documented, and every named test exists and passes
