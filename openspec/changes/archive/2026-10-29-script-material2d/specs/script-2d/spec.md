## MODIFIED Requirements

### Requirement: `<material2d>` is a `ColorMaterial`
`<material2d>` without `kind` SHALL insert `MeshMaterial2d<ColorMaterial>`. It SHALL take `color` (every colour form bevy-ts parses), `alpha` (0 to 1, applied with `Color::with_alpha`), `alphaMode` (`"opaque"`, `"blend"` or `{ mask: n }`, Bevy's `AlphaMode2d`), `texture` (an asset path, loaded as an `Image`) and `uvTransform` (`{ translation?, rotation?, scale? }`, built into an `Affine2`). Unset, `alphaMode` SHALL follow Bevy's `ColorMaterial::from(Color)` rule: `"blend"` when the final alpha is below 1, else `"opaque"`. An unknown prop SHALL fail the load naming it. Equal props SHALL share one material asset. Props SHALL NOT be bindable, as on `<material>`. `<material2d kind>` SHALL instead be a script 2D material (script-material2d), taking the kind's params, instance params and textures.

#### Scenario: A see-through textured material
- **WHEN** `<entity><mesh2d shape="rectangle" size={[64, 32]} /><material2d color="#f80" alpha={0.5} texture="tiles/brick.png" /></entity>` mounts
- **THEN** its `ColorMaterial` has colour `#f80` at alpha 0.5, `AlphaMode2d::Blend` and a texture handle for `tiles/brick.png`

#### Scenario: Equal materials share an asset
- **WHEN** two entities have `<material2d color="#0af" />`
- **THEN** both `MeshMaterial2d` handles are the same asset

#### Scenario: With a kind
- **WHEN** an entity has `<mesh2d shape="rectangle" />` and `<material2d kind={Planet} seed={3} />`
- **THEN** it holds a `MeshScriptMaterial2d` and no `MeshMaterial2d<ColorMaterial>`
