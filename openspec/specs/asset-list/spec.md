# asset-list Specification

## Purpose
The asset viewer's methods: the project's assets listed from disk and the asset server without loading them, a watch of their changes, and thumbnails rendered offscreen.

## Requirements

### Requirement: Listing the project's assets
`bevy-ts/assets` (optional `prefix`, `kind`) SHALL list every file under the game's asset root (Bevy's `AssetPlugin::file_path`, `assets/` by default) with: `path` (the asset path Bevy loads it by), `kind` (`image`, `mesh` for glTF and OBJ, `audio`, `font`, `shader`, `scene` for `.bsn` and `.scn.ron`, `other`), `loader` (the type name of the loader Bevy picks by extension, or null), `size` in bytes, `modified`, `load_state` (`not_loaded`, `loading`, `loaded`, `failed` with the error), and for glTF its labelled sub-assets (`Scene0`, `Mesh0/Primitive0`, `Material0`, `Animation0`) once loaded. It SHALL work in play, edit and windowed modes and SHALL NOT load anything to answer.

#### Scenario: The arena's assets
- **WHEN** a client calls `bevy-ts/assets` on a project with `assets/models/enemy.glb` and `assets/textures/floor.png`
- **THEN** it gets both, `enemy.glb` as `mesh` with `GltfLoader` and `floor.png` as `image`, each with its load state

### Requirement: Watching assets change
`bevy-ts/assets+watch` SHALL send the list, then one message per frame in which files were added, removed or modified under the asset root (from Bevy's file watcher, `file_watcher` feature on under `bevy-ts dev`) or an asset's load state changed (`AssetEvent`), naming each path and what changed.

#### Scenario: Dropping a texture into the folder
- **WHEN** `assets/textures/wall.png` is copied into the project
- **THEN** a watch message adds it as `image`, `not_loaded`

### Requirement: Thumbnails for the asset viewer
`bevy-ts/asset_thumbnail` (`path`, `size` in pixels, at most 512) SHALL answer a PNG (base64) of the asset: images scaled to fit, meshes and glTF scenes rendered by the engine on an isolated render layer with a fixed three-quarter camera and light, fonts as a sample line, and nothing for other kinds. It SHALL load the asset if needed, render offscreen without touching the viewport's ring or the game's cameras, and cache by path and modification time. While an offscreen render is in progress it SHALL answer `pending: true` with no PNG, and the client asks again.

#### Scenario: A glTF thumbnail while the game is paused
- **WHEN** in edit mode the asset viewer asks for `models/enemy.glb` at 256
- **THEN** after `pending` answers it gets a 256×256 PNG of the model, and the viewport's frames and the game's world are unchanged
