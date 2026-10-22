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
| `repeat: bool` | `true` | `.repeat()`, or `RepeatAnimation::Never` to play once |
| `speed: f32` | `1.0` | `ActiveAnimation::set_speed` |
| `transition_secs: f32` | `0.25` | the cross-fade when `clip` changes, through `AnimationTransitions::play` |

- When the scene is ready (`WorldInstanceReady`), every `AnimationPlayer` under the root gets the
  shared graph and fresh transitions, and plays the clip. A root that gets `PlayAnimation` after
  its scene is ready plays in that frame. A reloaded scene plays again on its new players.
- A changed `clip` cross-fades over `transition_secs`. A changed `speed` or `repeat` applies to the
  playing clip without restarting it. Removing `PlayAnimation` stops the players.
- Every player shares one `AnimationGraph`, with one node per distinct clip, added the first time
  a root names it, as `many_foxes` does. A hundred foxes cost one graph.
- The glTF file a clip comes from stays loaded from the first time a root names it. Bevy 0.20
  loads a file again for a label asked for before it is alive (bevy#10549), and a second load
  spawns every scene from that file again; with the file held, a clip named later (the button
  in `examples/foxes`) is already there.

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
- **A TS system** reads and writes `repeat`, `speed` and `transition_secs` as columns
  (`Query<[Mut<PlayAnimation>]>`); `clip` holds a handle, so only TSX, a level or the editor
  set it.

## Clips and skeletons

A clip moves the bones it names. It must come from the same glTF file as the scene, or from a
file with the same bone names. A clip whose names do not match moves nothing, and Bevy says
nothing.

Bevy's glTF loader adds an `AnimationPlayer` only where the file's own clips reach. A mesh-only
file has none, so `PlayAnimation` on it plays nothing; Rust warns once per root, naming the
entity, the scene and the clip. A clip path that fails to load (a wrong label) is logged once.

## When to write Rust instead

`PlayAnimation` plays one clip at a time. Blend trees, masks, several clips at once, events at a
clip's end, or retargeting onto another file's skeleton are Rust in the project's native crate,
as in Bevy ([docs/native.md](native.md)).

## Cost

Measured by `scripts/animation-bench.sh` (`crates/bevy-ts-cli/tests/animation_bench.rs`): the
release engine, the game viewport ticked at 120 Hz, `Fox.glb`, one arm per run, 3 runs each.

- **A**: a Rust plugin spawning N foxes the `many_foxes` way, no TS.
- **B**: N foxes in TSX with `PlayAnimation`.
- **C**: the same N foxes, static (no `PlayAnimation`).
- **D** (a diagnostic, `ARMS="A D B"`): A's Rust-spawned foxes, played by `PlayAnimation`.

Medians over every run (12 to 17 per arm at N = 50 and 100, 3 at 200), Apple M5 Max, a machine
other builds shared; frame time is the main world's update, `First` to `Last`. Arms ran
interleaved, so each B run is compared with the A run just before it.

| N | Arm | Update mean | Update p95 | Ready (frames after spawn) |
| --- | --- | --- | --- | --- |
| 50 | A, Rust | 3.70 ms | 7.09 ms | 0 |
| 50 | B, TSX + `PlayAnimation` | 3.50 ms | 6.27 ms | 0–1 |
| 50 | C, static | 3.45 ms | 6.19 ms | – |
| 100 | A, Rust | 3.75 ms | 6.27 ms | 0 |
| 100 | B, TSX + `PlayAnimation` | 3.85 ms | 7.12 ms | 0–1 |
| 100 | C, static | 3.28 ms | 4.56 ms | – |
| 200 | A, Rust | 3.87 ms | 7.18 ms | 0 |
| 200 | B, TSX + `PlayAnimation` | 3.91 ms | 7.47 ms | 0–1 |
| 200 | C, static | 3.22 ms | 4.98 ms | – |

B against the A run beside it, median ratio: p95 1.00 at 50, 1.01 at 100, 0.99 at 200; mean
1.00, 1.04, 1.00. The target (B's p95 within 10% of A's at 50 and 100, ready within one frame of
A's) holds. One run's p95 swings from 4.5 to 13 ms on a shared machine, so a few runs prove
nothing; pairs over many rounds do. A diagnostic arm D, Rust-spawned foxes played by
`PlayAnimation`, is level with A: the plugin does nothing per frame once a clip plays, and
playing costs what Bevy's own wiring costs. B is ready a frame after A at most, the frame
`PlayAnimation`'s system applies what the observer saw.
