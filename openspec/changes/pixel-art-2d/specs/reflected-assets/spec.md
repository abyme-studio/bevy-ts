## ADDED Requirements

### Requirement: A project-wide default sampler
`bevy-ts.json` SHALL take `"imageSampler": "nearest"` or `"linear"`. `nearest` SHALL build Bevy's `ImagePlugin::default_nearest()`, so every image whose sampler is `"Default"` samples nearest; `linear`, or no key, SHALL keep Bevy's default. Another value SHALL fail at start naming `imageSampler` and the value. An image given its own sampler SHALL keep it. `bevy-ts build` SHALL write the key into `game.json`, and `bevy-ts play` SHALL apply it.

#### Scenario: Pixel art without per-image settings
- **WHEN** a project sets `"imageSampler": "nearest"` and a sprite loads `hero.png` with no settings
- **THEN** the image samples nearest, and an `Image.load(p, { sampler: { Descriptor: { mag_filter: "Linear" } } })` image still samples linear

#### Scenario: A built game
- **WHEN** that project is built and played
- **THEN** `game.json` holds `"imageSampler": "nearest"` and the sprite samples nearest in play
