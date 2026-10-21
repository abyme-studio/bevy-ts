## Context

- `crates/bevy-ts-viewport/src/assets.rs` serves `assets`, `assets+watch` and `asset_thumbnail`. Thumbnails run as jobs: `Loading` (wait for the handle), `Rendering` (spawn the subject, a light and a camera on layer 30 into an image, settle, frame the bounds), `Reading` (a `Readback`, then PNG). Images skip the jobs. Every other kind answers `png: null`, fonts included, though the `asset-list` spec says fonts get a sample line.
- A level is composed by `bevy_ts_levels::compose_level` (includes resolved, scene components expanded through TS `scene()` in one `bevy_ts_tsx::expand` call) and then reconciled into the game as root `level:<path>`. Nothing spawns a composed level anywhere else.
- `viewport_pick` returns `edit::hits(ray)`: entity, distance along the ray, and the entity a click selects. The ray is `editor_ray`.
- The arena's `assets/` holds only `levels/arena.bsn` and `levels/room.bsn`.
- abyme's half is team bevy-assets (spec@bevy-assets agreed this protocol, 2026-10-22). viewport-tools (spec@viewport-gizmos) bumps the protocol to 1.11.0 and does not touch picking.

## Goals / Non-Goals

**Goals:** a thumbnail for every level and font; the listing says which assets have one; a world point for a drop; an example that shows every kind.

**Non-Goals:** audio waveforms, shader previews, `.scn.ron` thumbnails, live level previews, any import or spawn method (abyme copies files and writes levels itself).

## Decisions

1. **A level thumbnail is a picture, not a running copy.** The composed level's nodes are resolved (`bevy_ts_tsx::resolve_node`) and only what draws is spawned: `Transform`, `Visibility`, mesh and material intrinsics, `Mesh3d`/`MeshMaterial3d`, lights and glTF scene roots. TS components, colliders, rigid bodies, cameras and everything else are skipped. Every spawned entity carries `EditorOnly`, `ScriptHidden` and layer 30 from its first insert.
   - Why: nothing a TS observer, a TS query, rapier or the reconciler reacts to is ever inserted, so "invisible to the game" holds by construction, not by filtering after the fact.
   - Rejected: reconciling the level under a second root (`thumbnail:<path>`) with a hidden marker. TS `Add` observers and physics would fire before any marker could stop them, and the reconciler's maps and `tsx_tree` would see the root.
   - Rejected: drawing the live level's entities. It works only for a level that is loaded, and shows its state in play, not the file.
   - The level's own lights are kept; the thumbnail's light is added as for a mesh, so an unlit level still reads.
2. **Composing runs TS.** Expansion calls each placed prefab's `scene()` once, as loading the level would. `scene()` is a pure description (the levels spec forbids side effects in it), so this changes no game state. It runs in the job's `Loading` step once every file the level includes is loaded (`is_loaded_with_dependencies`), then the job goes on to `Rendering` with the spawned tree as subject. A compose error caches `png: null` and logs nothing to the game's log.
3. **Fonts render as `Text2d`.** A job loads the `Font`, then spawns a `Camera2d` into the image and a `Text2d` "Aa Bb 123" in that font sized to the width, both on layer 30, and reads back as meshes do. No new dependency.
4. **`thumbnail` is decided by `kind` and extension in one function** next to `kind()`, so the listing and `asset_thumbnail` can never disagree.
5. **Pick points come from the ray.** `point = origin + direction × depth` per hit (mesh hits already give the distance; icon hits use the distance along the ray to the icon's centre). `ground` is the ray's crossing of y = 0 when `direction.y` points toward the plane and the crossing is in front of the camera; null otherwise. Both are plain `[f32; 3]`.
6. **Example assets are generated, not downloaded.** A `.gltf` crate with an embedded base64 buffer and a PBR material, a 64×64 checker `.png` (written with `assets.rs`'s own PNG writer) and a short sine `.wav`, made by a test helper and committed. No third-party art, no licence question.
7. **Protocol 1.12.0.** `AssetInfo.thumbnail` (boolean, required in what the engine sends, optional for older engines in clients), `PickHit.point`, `ViewportPickResult.ground` (nullable). viewport-tools takes 1.11.0; whichever lands second rebases to the next minor.

## Risks / Trade-offs

- [A large level is slow to compose and draw] → It runs only when asked, once per path and mtime; the job is the same few frames as a glTF. Measure the arena and a 200-entity level in tasks and report the numbers to the operator before landing.
- [Skipping components changes the look] → A level whose look comes from a TS system at runtime (a material set in `Update`) shows its authored look. That is the file's look, which is what the asset list shows.
- [`scene()` with side effects] → Forbidden by the levels spec; a thumbnail calls it no more than loading the level does.
- [The arena tests count assets] → The levels walk and asset tests are rerun; any count that includes the new files is updated.

## Migration Plan

Additions only. An older engine sends no `thumbnail`; bevy-assets pins 1.12.0 and does not talk to one. Rollback is reverting the change.

## Open Questions

None.
