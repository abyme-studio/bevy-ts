## Context

Bevy 0.20 plays a glTF clip with Rust wiring (`examples/animation/animated_mesh.rs`, `examples/stress_tests/many_foxes.rs`): spawn `WorldAssetRoot("x.glb#Scene0")`, observe `WorldInstanceReady` on the root, find the `AnimationPlayer` among its descendants, insert `AnimationGraphHandle` and `AnimationTransitions`, then `transitions.play(&mut player, node, duration).repeat()`. Bevy ships no clip prop and no autoplay.

bevy-ts today:
- TSX and levels insert reflect-tier Rust components through one path (`crates/bevy-ts-tsx/src/reflected.rs`): from `ReflectDefault`, props set by reflection, a string in a `Handle<T>` field loaded as an asset path (levels decision 12).
- TSX cannot set a one-field tuple struct: props map to `.<field>`, and `.0` is no JSX attribute. Only `Name` has a `value` prop (`resolve.rs`). So `<WorldAssetRoot>` cannot take its handle from TSX; levels can (`WorldAssetRoot("…")`).
- The asset preview already attaches a graph and plays clips (`crates/bevy-ts-viewport/src/preview.rs`, `players`, `attach_graph`), for the editor only.
- TS cannot observe Rust entity events, walk children, insert reflect-tier components from `Commands`, or call component methods. Research named these four gaps for a script-driven form. lead@bevy-ts ruled (2026-10-23): no method calls from TS now; cover the need with a declarative component and Rust wiring.

## Goals / Non-Goals

**Goals:**
- A TS game, a level or the editor plays a glTF clip by placing one component beside `WorldAssetRoot`, following Bevy's own wiring.
- Changing the clip cross-fades; speed and repeat apply live.
- 50 to 100 animated characters cost no more than Bevy's own Rust pattern does, measured.

**Non-Goals:**
- Calling Rust component methods from TS (its own change later, ask-first).
- TS observers on Rust entity events, a descendants walk from TS, reflect-tier inserts from `Commands`. Rust does the wiring, so none is needed here.
- Blending trees, masks, several clips at once, events on clip end. A game that needs them writes Rust in its native crate, as in Bevy.
- Retargeting a clip from one file onto another file's skeleton. Bevy's glTF loader adds `AnimationPlayer` only where the file's own clips reach (`bevy_gltf` loader), so a mesh-only file gets none; the warning says so.

## Decisions

1. **One reflected component, not an element or a method API.** `PlayAnimation { clip: Handle<AnimationClip>, repeat: bool, speed: f32, transition_secs: f32 }` with `#[reflect(Component, Default)]`. Its `Handle` field is a hole, so by the native tiering rules it is a column for its plain fields and is never inserted from bytes: TSX, levels, the editor's inspector and the protocol write it through reflection, with no new path. `bevy-ts/bevy` types an element's props where they differ from its columns (`static __element`), so `<PlayAnimation clip>` and `<WorldAssetRoot value>` type-check. Alternatives: a `<scene clip loop>` intrinsic (rejected by lead@slice-2 and lead@bevy-ts: a special element, and levels could not place it); a TS script form over the four gaps (rejected: gap 4 is a new pattern, and the other three exist only to serve it). The name says what it asks for; it does not clash with bevy-ts's "scene component" (`SceneComponent`). `repeat` is a `bool` rather than Bevy's `RepeatAnimation` enum, so TSX and levels write `repeat={false}`; a count is a later field if a game needs it.

2. **A crate of its own, `bevy-ts-animation`.** Built and registered like `bevy-ts-material`: a `PlayAnimationPlugin` the engine adds in `bevy-ts-cli` (`cli.rs`, `types.rs`). The bridge (`bevy-ts`) stays engine-neutral; TSX is the element runtime and should not own animation. It needs only Bevy's `bevy_animation` and the scene types, already built by the `3d` feature.

3. **One shared graph, a node per distinct clip.** A resource holds one `Handle<AnimationGraph>` and a map from clip `AssetId` to `AnimationNodeIndex`; a clip seen first is added with `graph.add_clip(handle, 1.0, root)`. Every player gets the same `AnimationGraphHandle`, as `many_foxes` does, so 100 foxes share one graph, and any player can cross-fade to any clip without swapping graphs. Alternative: a graph per root (`AnimationGraph::from_clip`), which cannot cross-fade without rebuilding and costs an asset per character.

4. **Readiness from Bevy, not polled.** A global observer on `WorldInstanceReady` plays for a root that has `PlayAnimation` and records its players in `AnimationTargets(Vec<Entity>)` on the root. A system in `PostUpdate`, before `AnimationSystems`, handles `Changed<PlayAnimation>` (and `Added`): with `AnimationTargets` present it applies the change; without, it checks the root's `WorldInstance` with `WorldInstanceSpawner::instance_is_ready` and walks then (the added-after-ready case); otherwise it waits for the observer. A second `WorldInstanceReady` (asset reload) replaces `AnimationTargets`. `RemovedComponents<PlayAnimation>` stops the players and removes `AnimationTargets`. All Rust, no JS call.

5. **Applying a change.** New clip: `transitions.play(&mut player, node, Duration::from_secs_f32(transition_secs))`, then `.set_speed(speed)` and `.repeat()` or `.set_repeat(RepeatAnimation::Never)`. Same clip, other speed or repeat: `player.animation_mut(node)` and set them in place. Comparing the clip handle to the transition's current node tells which.

6. **`value` for one-field tuple structs in TSX.** `resolve.rs` maps a `value` prop to field `.0` when the reflected type is a tuple struct of one field, generalising what `Name` does; any other prop on such a type is an error naming `value`. Handles inside go through the same asset-path load. This is the only TSX change, and it serves every such Bevy component (`WorldAssetRoot`, `Mesh3d`, `MeshMaterial3d`).

7. **The player walk has one owner.** `bevy_ts_animation::players(world, root)` (and a `Query<&Children>` form for systems) replaces `preview.rs`'s private `players`; the preview depends on the crate.

8. **Protocol: no change.** `PlayAnimation` is reached through the existing reflected-component methods and appears in `bevy-ts/bevy` by regeneration. No method, param or result changes, so the protocol version stays.

## Risks / Trade-offs

- [A clip whose bone names do not match the scene's] → nothing moves and Bevy says nothing. Mitigation: docs say a clip must come from the same file or one with identical bone names; slice-2's asset test checks villager files.
- [`transition_secs` on the first play] → `AnimationTransitions::play` with nothing playing just starts the clip; no fade-in from T-pose. Accepted, as Bevy does.
- [The shared graph grows by one node per distinct clip, never shrinks] → a few bytes per clip for the life of the app. Accepted.
- [Many characters in one frame of readiness] → the observer walks each root once; at 200 roots this is one-off work. The bench measures spawn-to-ready.
- [`bevy-ts/bevy` gains a type] → regenerated, CI checks it with `scripts/gen-bevy-types.sh --check`.

## Migration Plan

Additive. Nothing existing changes behaviour except TSX accepting `value` on one-field tuple structs, which was an error before.

## Open Questions

None blocking. The perf result may move the target: if arm B is not within 10% of arm A at N=100, the fix belongs in this change before it lands.
