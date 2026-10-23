## Why

A glTF scene's body cannot take a pick handler. Agent town's units are clickable only on their disc: `onPointerClick` on the entity carrying `WorldAssetRoot` never fires for a click on the fox, the villager or any mesh of the scene spawned under it. Bevy's picking events bubble up `ChildOf`, and the scene's meshes are pickable and parented under the root, but each TSX handler drops an event whose original target is not its own entity (`crates/bevy-ts-tsx/src/picking.rs`). That rule was a choice made when picking props landed ("Events SHALL NOT bubble in this change"), not a ruling.

## What Changes

- **BREAKING (behaviour)**: TSX picking handlers (`onPointerClick`, `onPointerPress`, `onPointerRelease`, `onPointerOver`, `onPointerOut`) see events bubbled from their entity's descendants, as a Bevy entity observer does. A handler on a scene root fires for a click on any mesh of its scene; a handler on a `<node>` fires for a click on a button inside it.
- The event gains `target`: the entity the pointer hit. `entity` stays the entity whose handler runs. A handler that wants only its own hits compares the two.
- Docs: the bubbling rule and `target` in `docs/picking.md` and `jsx-runtime.d.ts`, and a scene root taking clicks for its scene in `docs/animation.md`.

Out of scope, each its own later change:
- Stopping propagation from TS. A handler runs in the frame's one host call, after Bevy has finished the walk, so it cannot stop it; comparing `target` to `entity` covers the cases slice-1 needs.
- `onPointerEnter` and `onPointerLeave`, over Bevy's `PointerEnter`/`PointerLeave`, which bubble only to ancestors the previous hover did not share. Without them, a panel's `onPointerOut` then `onPointerOver` fire as the pointer crosses into a child (docs/picking.md, "Bubbling"); hover state ends right, counting does not.

## Capabilities

### Modified Capabilities
- `reactive-ui`: picking handlers see bubbled events; the event names the entity it hit.

## Impact

- `crates/bevy-ts-tsx/src/picking.rs`: the observers no longer drop bubbled events; each queued event carries `target`.
- `crates/bevy-ts-v8/js/jsx-runtime.js` (and its synced copy): the handler's event carries `target`.
- `packages/bevy-ts/jsx-runtime.d.ts`: `PointerEvent.target`.
- `crates/bevy-ts-cli/tests/tsx_reactive.rs`: the no-bubbling assertion becomes a bubbling one; a new test clicks a mesh of a spawned scene.
- Performance: none. Bevy already walks the ancestors and calls their observers for every picking event; the handler now queues instead of returning.
- Protocol: no change.
