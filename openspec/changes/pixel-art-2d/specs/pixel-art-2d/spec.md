## ADDED Requirements

### Requirement: Y-sort within a layer
`YSort { layer, offset }` SHALL set its entity's `Transform` z to `layer − clamp(y + offset, −49,999, 49,999) × 10⁻⁵`, where `y` is the entity's `Transform` y, before Bevy's transform propagation in the frame. A lower base SHALL draw in front. `layer` SHALL default to 0 and be at most 127 in size; a larger one SHALL fail naming `layer` and the value. Layers a whole unit apart SHALL never mix. A base y past the clamp SHALL log one warning naming the entity and its y. The entity's descendants SHALL follow it by Bevy's propagation, keeping their local z. `YSort` SHALL own `Transform` z: a script's write to it is overwritten the same frame.

#### Scenario: The lower sprite in front
- **WHEN** two overlapping sprites with `<YSort layer={1} />`, a red one at y 0 and a blue one at y 8, render
- **THEN** the overlap is red; moving the blue one to y -8 makes the overlap blue

#### Scenario: The formula
- **WHEN** an entity with `<YSort layer={1} offset={-8} />` is at y 108
- **THEN** after the frame its `Transform` z is 1 − 100 × 10⁻⁵ = 0.999 and its `GlobalTransform` z is the same

#### Scenario: A layer above
- **WHEN** a sprite with `layer` 1 at y -40000 overlaps a sprite with `layer` 2 at y 40000
- **THEN** the `layer` 2 sprite draws in front

#### Scenario: One pixel at the top layer
- **WHEN** two sprites with `layer` 127 are at y 0 and y 1
- **THEN** their z differ, and the one at y 0 draws in front

#### Scenario: A layer out of range
- **WHEN** a script mounts `<YSort layer={200} />`
- **THEN** it fails naming `layer` and 200

#### Scenario: A shadow child
- **WHEN** a y-sorted character has a child shadow sprite at local z -0.00001
- **THEN** the shadow draws just behind the character wherever the character's y puts it

#### Scenario: A script's z is overwritten
- **WHEN** a script writes z 5 to a y-sorted entity's `Transform` with `layer` 1 at y 0
- **THEN** after the frame its z is 1

### Requirement: One pass before propagation
`YSort`, `ScreenSize` and the pixel camera's scale SHALL be applied by one pass in `PostUpdate`, after the TSX reconcile and before `TransformSystems::Propagate`. It SHALL write a field only when its value changes, so an entity that does not move is not marked changed and Bevy does not re-propagate it. Nothing in this capability SHALL write `GlobalTransform`. The pass SHALL do no work when no entity has `YSort`, `ScreenSize` or `PixelCamera`.

#### Scenario: A still sprite over many frames
- **WHEN** a y-sorted sprite does not move for 100 frames
- **THEN** its z is the same in each, and its `Transform` is not marked changed after the first

#### Scenario: A reconciled move draws the same frame
- **WHEN** a TSX binding moves a y-sorted sprite's y in a frame
- **THEN** that frame's `GlobalTransform` has the new y and the z derived from it

### Requirement: A pixel camera zooms by whole numbers
`PixelCamera { zoom, ease }` on a `Camera2d` SHALL set the orthographic projection's `scale` to `1 / zoom`. `zoom` SHALL be a whole number from 1; another value SHALL fail naming `zoom` and the value. A change of `zoom` SHALL move the scale there over `ease` seconds (default 0.25) of real time with a cubic ease-out, about the camera's centre; `ease` 0 SHALL jump. The scale SHALL be written in the pass before propagation. The `PixelCamera` SHALL own `scale`: a script's write to it is overwritten.

#### Scenario: 2x
- **WHEN** `<Camera2d />` has `<PixelCamera zoom={2} />`
- **THEN** its projection scale is 0.5 and a 16 px sprite covers 32 screen pixels

#### Scenario: An eased move
- **WHEN** `zoom` goes from 2 to 4 with `ease` 0.25
- **THEN** the scale is strictly between 0.5 and 0.25 at 0.1 s and exactly 0.25 from 0.25 s on

### Requirement: A pixel camera snaps its view to whole screen pixels
With a `PixelCamera`, a system after `TransformSystems::Propagate` and before Bevy's `CameraUpdateSystems` SHALL round the camera's global x and y to its screen-pixel step (its current scale, in world units), with half a step more on an axis whose target size in pixels is odd, and SHALL shift the projection's `viewport_origin` so the view is drawn from the rounded point. It SHALL write neither the camera's `Transform` nor its `GlobalTransform`. Sprites SHALL keep their positions: at a whole zoom with nearest sampling, each texel covers whole screen pixels. The `PixelCamera` SHALL turn its camera's `Msaa` off, so a pixel is filled when its centre is inside a sprite, or not at all. Two `PixelCamera`s SHALL log one warning naming both; the first added SHALL rule.

#### Scenario: A sub-pixel camera
- **WHEN** at zoom 4 the camera's global x is 10.3
- **THEN** the view is drawn from x 10.25, and the camera's `Transform` x stays 10.3

#### Scenario: A sub-pixel sprite
- **WHEN** at zoom 2 a 16 px sprite with nearest sampling is at x 0.3 and renders
- **THEN** it covers 32 whole screen pixels, the same pixels as at x 0.5 (its nearest step), and its `Transform` x stays 0.3

### Requirement: Screen-size sprites
`ScreenSize { scale }` SHALL set its entity's `Transform` scale x and y, in the pass before propagation, to `scale` (default 1) times the ruling camera's projection scale (the `PixelCamera`'s, else the first `Camera2d`'s orthographic scale), so one image pixel is one screen pixel at every zoom. Its descendants SHALL scale with it by propagation. `ScreenSize` SHALL own `Transform` scale: a script's write to it is overwritten. Sprite picking SHALL hit the drawn size.

#### Scenario: A marker at two zooms
- **WHEN** a 16 px `<ScreenSize />` marker is drawn at zoom 1 and at zoom 4
- **THEN** it covers 16 screen pixels in both, its `Transform` scale is 0.25 at zoom 4, and a click on its drawn area at zoom 4 runs its handler

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
`YSort`, `PixelCamera`, `ScreenSize` and `SpriteClip` SHALL be reflected components with Bevy's field names, mounted from TSX as `<YSort layer={1} offset={-8} />` and typed in `bevy-ts/bevy`. A Rust app SHALL get them from one plugin beside `DefaultPlugins`. `docs/2d.md`'s coverage table SHALL mark each working with its test.

#### Scenario: From TSX
- **WHEN** a script mounts `<entity><Sprite image="hero.png" /><YSort layer={1} offset={-8} /></entity>`
- **THEN** the entity has `YSort { layer: 1.0, offset: -8.0 }`
