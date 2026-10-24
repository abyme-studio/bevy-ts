## MODIFIED Requirements

### Requirement: Thumbnails for the asset viewer
`bevy-ts/asset_thumbnail` (`path`, `size` in pixels, at most 512) SHALL answer a PNG (base64) of the asset: images scaled to fit; meshes, glTF scenes and `.bsn` levels rendered by the engine on an isolated render layer with a fixed three-quarter camera and light, framed to their bounds; and `png: null` for every asset whose listing says `thumbnail: false`. It SHALL load the asset if needed, render offscreen without touching the viewport's ring or the game's cameras, and cache by path and modification time. While an offscreen render is in progress it SHALL answer `pending: true` with no PNG, and the client asks again. Since render pipelines compile in the background, it SHALL read the picture back only once the render world has no pipeline compiling, or after 30 s, so a subject whose pipelines are new is drawn, not left out.

A level SHALL be composed with its includes and its scene components expanded, as loading it would, and spawned as a tree of its own that is script-hidden (no TS system query matches it), absent from `bevy-ts/tsx_tree`, `bevy-ts/entities+watch` and `.bsn` writing, without colliders, and despawned once read back. A level that fails to compose SHALL answer `png: null` and leave the game unchanged.

#### Scenario: A glTF thumbnail while the game is paused
- **WHEN** in edit mode the asset viewer asks for `models/enemy.glb` at 256
- **THEN** after `pending` answers it gets a 256×256 PNG of the model, and the viewport's frames and the game's world are unchanged

#### Scenario: The arena level's thumbnail
- **WHEN** the asset viewer asks for `levels/arena.bsn` at 128 while the arena plays
- **THEN** after `pending` answers it gets a 128×128 PNG showing the level's enemies, and no TS system saw a new enemy, the enemy count in `tsx_tree` is unchanged, and no physics body was added

#### Scenario: A font
- **WHEN** the asset viewer asks for `fonts/ui.ttf` at 128
- **THEN** it gets `png: null` at once, with no `pending`

#### Scenario: A thumbnail on a fresh engine
- **WHEN** the asset viewer asks for `models/crate.gltf` before any pipeline for its material has compiled
- **THEN** `pending` answers until the pipelines are ready, and the PNG shows the crate
