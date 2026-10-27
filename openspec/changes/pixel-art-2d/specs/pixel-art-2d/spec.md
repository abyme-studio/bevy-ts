## ADDED Requirements

### Requirement: Y-sort within a layer
`YSort { offset }` SHALL draw its entity by its base y: the drawn z SHALL be the entity's z minus its base y (its global y plus `offset`) times 1/100,000, with base y clamped to ±49,999 px. A lower base SHALL draw in front. Layers whole z units apart SHALL never mix. The entity's descendants SHALL move with it, keeping their z relative to it, unless a descendant has its own `YSort`. Only `GlobalTransform` SHALL change: `Transform`, and a level saved from it, SHALL keep the z the script gave.

#### Scenario: The lower sprite in front
- **WHEN** two overlapping `<YSort />` sprites at z 1, a red one at y 0 and a blue one at y 8, render
- **THEN** the overlap is red; moving the blue one to y -8 makes the overlap blue

#### Scenario: A layer above
- **WHEN** a y-sorted sprite at z 1, y -40000 overlaps a sprite at z 2, y 40000
- **THEN** the z 2 sprite draws in front

#### Scenario: A shadow child
- **WHEN** a `<YSort />` character has a child shadow sprite at local z -0.001
- **THEN** the shadow draws just behind the character wherever the character's y puts it

#### Scenario: Transform untouched
- **WHEN** a y-sorted entity at z 1 moves
- **THEN** its `Transform` z stays 1 and a saved level holds 1

### Requirement: The pass is idempotent
The pass that adjusts `GlobalTransform` SHALL give the same result each frame for an unchanged entity, whether or not Bevy's propagation rewrote its `GlobalTransform` that frame. It SHALL run in `PostUpdate` after transform propagation and before visibility checks, and SHALL do no work when no entity has `YSort`, `ScreenSize` or a `PixelCamera` in the world.

#### Scenario: A still sprite over many frames
- **WHEN** a `<YSort />` sprite does not move for 100 frames
- **THEN** its drawn z is the same in each

### Requirement: A pixel camera zooms by whole numbers
`PixelCamera { zoom, ease }` on a `Camera2d` SHALL set the orthographic projection's `scale` to `1 / zoom`. `zoom` SHALL be a whole number from 1; another value SHALL fail naming `zoom` and the value. A change of `zoom` SHALL move the scale there over `ease` seconds (default 0.25) with a cubic ease-out, about the camera's centre; `ease` 0 SHALL jump. The `PixelCamera` SHALL own `scale`: a script's write to it is overwritten.

#### Scenario: 2x
- **WHEN** `<Camera2d />` has `<PixelCamera zoom={2} />`
- **THEN** its projection scale is 0.5 and a 16 px sprite covers 32 screen pixels

#### Scenario: An eased move
- **WHEN** `zoom` goes from 2 to 4 with `ease` 0.25
- **THEN** the scale is strictly between 0.5 and 0.25 at 0.1 s and exactly 0.25 from 0.25 s on

### Requirement: A pixel camera snaps to whole screen pixels
With a `PixelCamera`, the camera's and every `Sprite`'s drawn x and y SHALL be rounded to the camera's screen-pixel step (its current scale, in world units), with the camera offset by half a step on an axis whose target size in pixels is odd, so a sprite's texels land on whole screen pixels. Two `PixelCamera`s SHALL log one warning naming both; the first added SHALL rule.

#### Scenario: A sub-pixel position
- **WHEN** at zoom 4 a sprite's global x is 10.3
- **THEN** its drawn x is 10.25, and its `Transform` x stays 10.3

### Requirement: Screen-size sprites
`ScreenSize` SHALL scale its entity's drawn size by the ruling camera's scale (the `PixelCamera`, else the first `Camera2d`'s orthographic scale), so one image pixel is one screen pixel at every zoom. Its descendants SHALL scale with it about its origin. Sprite picking SHALL hit the drawn size.

#### Scenario: A marker at two zooms
- **WHEN** a 16 px `<ScreenSize />` marker is drawn at zoom 1 and at zoom 4
- **THEN** it covers 16 screen pixels in both, and a click on its drawn area at zoom 4 runs its handler

### Requirement: Sprite clips
`SpriteClip { first, last, fps, mode, playing }` SHALL step its entity's `Sprite.texture_atlas.index` from `first` to `last` at `fps` frames a second of virtual time. `mode` SHALL be `Repeat`, `Once` (holds `last` and sets `finished`) or `PingPong` (first to last and back). `playing: false` SHALL hold the frame. A change of `first`, `last` or `mode` SHALL restart at `first`. `last` below `first`, `fps` not above 0, or a sprite without an atlas SHALL fail naming the field or the missing atlas. `last` past the layout's tiles SHALL fail naming the index and the count.

#### Scenario: A repeating walk
- **WHEN** a sprite with a 4-tile atlas has `<SpriteClip first={0} last={3} fps={10} mode="Repeat" />` and 0.45 s of virtual time pass
- **THEN** its atlas index is 0 (frames 0, 1, 2, 3, then 0)

#### Scenario: Once
- **WHEN** the same clip has `mode="Once"` and 1 s passes
- **THEN** the index is 3 and `finished` is true

#### Scenario: Ping-pong
- **WHEN** the clip has `mode="PingPong"` and 0.45 s passes
- **THEN** the index is 2 (frames 0, 1, 2, 3, 2)

#### Scenario: Paused time
- **WHEN** virtual time is paused
- **THEN** the index does not change

### Requirement: The components from TSX and Rust
`YSort`, `PixelCamera`, `ScreenSize` and `SpriteClip` SHALL be reflected components with Bevy's field names, mounted from TSX as `<YSort offset={-8} />` and typed in `bevy-ts/bevy`. A Rust app SHALL get them from one plugin beside `DefaultPlugins`. `docs/2d.md`'s coverage table SHALL mark each working with its test.

#### Scenario: From TSX
- **WHEN** a script mounts `<entity><Sprite image="hero.png" /><YSort offset={-8} /></entity>`
- **THEN** the entity has `YSort { offset: -8.0 }`
