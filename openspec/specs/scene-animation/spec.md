# scene-animation Specification

## Purpose
A game plays a glTF clip on a spawned scene by placing one component, `PlayAnimation`, beside its `WorldAssetRoot`, from TSX, a level or the editor. Rust does Bevy's own wiring (the shared graph, `AnimationTransitions`, cross-fades), so no JS runs to play, switch or stop a clip.

## Requirements

### Requirement: PlayAnimation names a clip for a scene
The engine SHALL register a reflected Rust component `PlayAnimation` with `ReflectComponent` and `ReflectDefault`, and these fields: `clip: Handle<AnimationClip>` (default: none), `repeat: bool` (default `true`), `speed: f32` (default `1.0`) and `transition_secs: f32` (default `0.25`). It SHALL be tiered by the native-components rules: its plain fields (`repeat`, `speed`, `transition_secs`) are columns TS systems read and write, and `clip` is a hole TS columns never see. It SHALL be named `PlayAnimation` in `bevy-ts/bevy`, never inserted from bytes, and written through reflection, `clip` included, wherever it is placed: a TSX component element (its type listing every field as a prop), a level patch and the editor. `clip` SHALL be written as an asset path with a label (`"models/Fox.glb#Animation2"`) and loaded as a `Handle<AnimationClip>`. It SHALL go on the entity that carries `WorldAssetRoot`.

#### Scenario: Placed in TSX
- **WHEN** a scene describes `<entity><WorldAssetRoot value="models/Fox.glb#Scene0" /><PlayAnimation clip="models/Fox.glb#Animation2" /></entity>`
- **THEN** the entity carries `PlayAnimation` with that clip handle, `repeat` true, `speed` 1 and `transition_secs` 0.25

#### Scenario: Placed from a level
- **WHEN** a level file patches an entity with `PlayAnimation { clip: "models/Fox.glb#Animation1" }` beside `WorldAssetRoot("models/Fox.glb#Scene0")`
- **THEN** the fox plays its walk clip after the level loads, with no TS involved

### Requirement: The clip plays once the scene is ready
When a `WorldAssetRoot`'s instance is ready (`WorldInstanceReady` on the root) and the root carries `PlayAnimation`, Rust SHALL find every `AnimationPlayer` among the root's descendants, insert on each an `AnimationGraphHandle` to the engine's shared graph and an `AnimationTransitions`, and play the clip's node, repeating forever when `repeat` is true and once otherwise, at `speed`. When `PlayAnimation` is added to a root whose instance is already ready, it SHALL play the same way that frame. When the scene is spawned again (its asset reloaded), it SHALL play again on the new players. Playing SHALL make no call into JS.

#### Scenario: A fox runs
- **WHEN** a root has `WorldAssetRoot("models/Fox.glb#Scene0")` and `PlayAnimation { clip: "models/Fox.glb#Animation2" }`
- **THEN** after the scene is ready, its `AnimationPlayer` is playing the run clip on repeat at speed 1

#### Scenario: Added after ready
- **WHEN** a fox's scene is ready with no `PlayAnimation`, and `PlayAnimation` is then inserted on its root
- **THEN** its player starts the clip in the frame the insert is applied

#### Scenario: Many foxes, one graph
- **WHEN** 100 roots play the same clip
- **THEN** all their players share one `AnimationGraph` asset with one node for that clip

### Requirement: Changing PlayAnimation changes what plays
A change to `clip` SHALL cross-fade every player under the root from the current clip to the new one over `transition_secs` seconds, through `AnimationTransitions`. A change to `speed` or `repeat` SHALL apply to the clip already playing, without restarting it. Removing `PlayAnimation` SHALL stop every animation on those players. Each distinct clip SHALL get one node in the shared graph, added the first time any root names it.

#### Scenario: Run to walk
- **WHEN** a TSX binding changes a running fox's `clip` from `#Animation2` to `#Animation1` with `transition_secs` 0.25
- **THEN** over the next 0.25 seconds the run's weight falls to 0 and the walk's rises to 1, and the walk then repeats

#### Scenario: Speed only
- **WHEN** `speed` changes from 1 to 2 on a fox mid-cycle
- **THEN** the same clip keeps playing from where it was, twice as fast

### Requirement: A scene with nothing to animate says so
When a root's instance is ready, carries `PlayAnimation`, and has no `AnimationPlayer` among its descendants, Rust SHALL log one warning per root naming the entity, the scene's asset path and the clip, and saying the scene has no animation of its own, so its glTF file needs an animation for a player to exist. A clip handle that fails to load SHALL log the asset error once per path. Neither SHALL stop the game.

#### Scenario: A mesh-only model
- **WHEN** a root has `WorldAssetRoot("models/villager.glb#Scene0")`, whose file has no animations, and `PlayAnimation { clip: "models/idle.glb#Animation0" }`
- **THEN** one warning names the entity, `models/villager.glb#Scene0` and `models/idle.glb#Animation0`, and nothing plays

### Requirement: One walk for a scene's animation players
The animation crate SHALL own the walk that lists the `AnimationPlayer` entities under a root (the root included), and the asset preview (asset-preview) SHALL use it. The preview's behaviour SHALL NOT change.

#### Scenario: The preview still plays
- **WHEN** a preview of a glTF with animations gets `asset_preview_control { animation: "walk", play: true }`
- **THEN** the walk plays as before, its players found by the shared walk
