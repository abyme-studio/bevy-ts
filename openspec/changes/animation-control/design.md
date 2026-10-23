## Context

`bevy-ts-animation` plays `PlayAnimation` the `many_foxes` way: one `SharedGraph`, an `AnimationTransitions` per player, `AnimationTargets` on the root listing its players. research.md measured what each Bevy control costs: pause and speed 0 sample and write every bone; `stop_all` and removing `AnimationGraphHandle` skip it (about 1 ms of update and 75 CPU points at 400 foxes); hiding saves only GPU. Bevy 0.20 has no visibility culling for animation.

The crate and the `scene-animation` spec were script-gltf's; lead@script-gltf settled the shared items with lead@animation-control on 2026-10-23, and they are now ours.

## Goals / Non-Goals

**Goals:**
- A paused root costs nothing per frame; an off-screen root can opt into the same.
- TS and TSX reach every animation control Bevy has, by clip path, never by node index.
- Measured: fox bench arms and agent town's 100 villagers, before and after.

**Non-Goals:**
- Several clips at once, blend trees, masks, per-clip weights from TS. Rust, as today.
- Culling by default, distance LOD, or advancing a parked clip's time.
- Exposing raw `AnimationPlayer` to TS systems. That is reflect-coverage stage 3, and its node-keyed map would still not be path-keyed.

## Decisions

1. **Park by removing `AnimationGraphHandle`.** `advance_animations` and `animate_targets` need it, so without it Bevy neither advances nor writes; the player keeps seek time, weights and transitions. Over `stop_all`: an exact resume, and no restart at 0. Over removing `AnimationPlayer`: no state lost. Cost: one archetype move per player each way, only on a change.
2. **One park system, a decision per root.** In `PostUpdate`, after `apply_changed`, before `AnimationSystems`: `parked = paused || (cull && meshes non-empty && no mesh has ViewVisibility)`. It compares with whether the first player has the handle, and removes or inserts on every player when they differ. It queries only roots with `paused` or `cull` true, plus roots whose `PlayAnimation` changed (to wake one whose flags were cleared), so the default costs nothing. `stop_removed` inserts the handle back on a parked root's players.
3. **`paused` also calls `pause_all`.** Parking alone stops the work; `pause_all` keeps Bevy's own state true for anything reading `is_paused` (and `status().paused`), and costs nothing more.
4. **Meshes recorded with the players.** `AnimationTargets` gains `meshes: Vec<Entity>`, every descendant with `Mesh3d`, filled in `start` from the same walk (the walk stays `players`; a second function `meshes` beside it, both owned here). A TSX child mesh counts: if it is seen, the root is seen. Recording once avoids a per-frame tree walk; a respawned scene re-records in `start`.
5. **Previous frame's visibility.** `ViewVisibility` is computed in `PostUpdate` after propagation, so the park system reads last frame's. A waking root shows one stale frame, as Unity's `CullUpdateTransforms` does. Running after visibility would need animation after propagation, which Bevy does not do.
6. **`repeat` takes Bevy's `RepeatAnimation`.** Bevy's own type gives `Count(n)` with no new field. It is reflected and registered by `AnimationPlugin`. It becomes a hole for TS systems (an enum with data); a column bool plus count would be a second spelling of one concept.
7. **`speed` 0 stays Bevy's.** Parking on speed 0 would change Bevy's meaning and surprise a game that animates speed through 0. Docs point to `paused`.
8. **`Animations` is an exported param, keyed by root.** A path-keyed read needs logic (root to players, clip to node to `ActiveAnimation`), which the generic reflected path cannot give without node indices. Exported-param args take no strings, so the key is the root, which names its clip by path. `status` is read-only and works today; `seek`, `rewind`, `replay` need `Query<&mut AnimationPlayer>`, which writable-native-params allows. Module `bevy-ts/animation`, registered by every engine as `Cameras` is. The param holds `Query<(&PlayAnimation, &AnimationTargets)>`, `Query<(&AnimationPlayer, Has<AnimationGraphHandle>)>` (mut for the writers), `Res<SharedGraph>` and `Res<Assets<AnimationClip>>`.
9. **`seek` is `seek_to`.** Events between the old and new time fire, Bevy's documented seek. `set_seek_time` stays Rust.

## Risks / Trade-offs

- [A parked root still runs `advance_transitions`, so a cross-fade under way finishes while parked] → Accept: the fade is short and the wake shows the new clip. Documented.
- [A culled clip's time stands still, so villagers that wake are out of step with where they would be] → Fine for loops; a game that cares seeks on wake. Documented.
- [The archetype move on park/wake churns if a root flickers in and out of view] → Moves happen once per change; the bench's culled arm pans a camera to measure it.
- [Breaking `repeat` for agent town] → One line in `world.tsx` when it takes the release; the proposal names it.
- [`seek`/`rewind`/`replay` wait on writable-native-params] → They land behind it in this change (task 4); if it slips, this change lands them later in a second commit on the same branch, never a hand-written host function.

## Migration Plan

`repeat={true}` → drop it (`Forever` is the default); `repeat={false}` → `repeat="Never"`; a level's `repeat: false` → `repeat: Never`. Rollback is reverting the change; no data or protocol state.

## Open Questions

None.
