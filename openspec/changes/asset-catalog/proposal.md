## Why

abyme's Bevy studio shows the arena's assets as two levels with a "?" icon (the editor study, 2026-10-22, change 8 "Assets"). `bevy-ts/assets` already lists every file under the asset root and `asset_thumbnail` already exists, but the engine answers `png: null` for levels, fonts and everything else but images and meshes, and the arena ships nothing but its two `.bsn` levels. The `asset-list` spec also promises font thumbnails the code never drew; Bevy's editor plans none, so the promise goes. Dragging a model into the scene also needs the world point under the pointer, which `viewport_pick` does not give.

## What Changes

- **Level thumbnails.** `asset_thumbnail` renders a `.bsn` level offscreen like a glTF scene: composed with its prefabs expanded, spawned isolated on the thumbnail layer, invisible to the game, its TS systems and the protocol, with no colliders, then despawned.
- **No font thumbnails.** The spec's unbuilt promise of a sample line is removed; a font answers `png: null` and gets a kind icon, as it does today.
- **Which kinds get one.** Each `AssetInfo` from `assets` and `assets+watch` gains `thumbnail: bool`, true for `image`, `mesh` and `.bsn` `scene`. The studio asks only for those and shows a kind icon for the rest. `font`, `audio`, `shader`, `.scn.ron` and `other` stay `png: null`.
- **A world point from picking.** `viewport_pick` gives each hit its world `point` `[x, y, z]`, and the result a `ground` `[x, y, z]`: where the ray meets the plane y = 0, or null when it misses. A model dropped on empty floor lands there.
- **The arena shows every kind.** `examples/arena/assets` gains a small model, a texture and a sound, made in-repo, so the studio's list shows a mesh, an image, audio and levels.
- **Protocol:** additions only, the next free minor on main when it lands. Planned 1.17.0, after main's 1.15.0 and viewport-tools' 1.16.0; lead@bevy-ts names the number at landing.
- **Out of scope:**
  - abyme's half (team bevy-assets): thumbnails in `Assets.tsx`, kind icons, import by drop (abyme copies the file into the asset root; `assets+watch` reports it), and drag into the scene (abyme writes the entity through the outliner stream's level write at `point` or `ground`). No other engine method.
  - Font thumbnails, waveforms for audio, shader previews, `.scn.ron` thumbnails.
  - Live previews of levels (`asset_preview` stays meshes and glTF).

## Capabilities

### New Capabilities

None.

### Modified Capabilities

- `asset-list`: the listing says which assets get a thumbnail; thumbnails cover `.bsn` levels; fonts get none.
- `edit-mode`: `viewport_pick` answers world points and the ground point.
- `shared-surface`: the contract records the version that carries these fields.

## Impact

- `crates/bevy-ts-viewport/src/assets.rs`: `thumbnail` on `AssetInfo`; a level render job beside the mesh job.
- `crates/bevy-ts-levels`: a way to compose a level and spawn it as an isolated, script-hidden tree that no root owns (today a composed level is only reconciled into the game).
- `crates/bevy-ts-viewport/src/edit.rs`, `methods.rs`: `point` per hit and `ground` in `viewport_pick`.
- `protocol/bevy-ts.openrpc.json`: the next free minor (planned 1.17.0), generated into Rust and TS types; `packages/bevy-ts/protocol.d.ts` regenerated.
- `examples/arena/assets`: three small generated files (a `.gltf` with an embedded buffer, a `.png`, a `.wav`), no third-party art.
- `docs/viewport.md`: the asset methods and the pick answer.
- Performance (operator's go needed): a level thumbnail composes the level, which calls each placed prefab's TS `scene()` once, and renders it offscreen over a few frames, the same path as a glTF thumbnail. It runs only when asked, once per path and modification time (cached), and never while nothing asks. The cost is measured in tasks.
- abyme: none in this change. bevy-assets pins the bevy-ts release that carries this version and reads `thumbnail` as always present, so its build waits on this one landing.
