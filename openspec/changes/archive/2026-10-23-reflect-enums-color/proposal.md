## Why

TSX cannot pick an enum variant or set a colour on a Bevy component. `camera3d` takes only
`position` and `lookAt`, and `<Projection>` is written field by field into its `Perspective`
default, so no scene can be orthographic. A light's `color` is a layout-tier hole, so sun and
fill cannot be warmed. slice-2's orthographic 2.5D look needs both. The fix is general: every
reflected enum and every `Color` is a value TSX can write.

## What Changes

- TSX writes any reflected enum in Bevy's own shape: a unit variant as its name (`"Hidden"`), a
  variant with data as `{ Variant: ... }` (`<Projection value={{ Orthographic: { scale: 0.02,
  scaling_mode: { FixedVertical: { viewport_height: 12 } } } }} />`). Nested enums
  (`ScalingMode`) and `Option<T>` (`T | null`) too. A component that is itself an enum takes its
  whole value as `value`.
- A written value is the JSON over the default: keys left out take the field's or the variant's
  default, never the previous value. A newly chosen variant starts from its default, so
  `{ Orthographic: { scale: 2 } }` is enough.
- Bevy's `Color` is a full value in TSX: `"#rgb"`, `"#rrggbb[aa]"`, `[r, g, b, a?]` (sRGB), or
  Bevy's reflected shape (`{ Srgba: {...} }`, `{ LinearRgba: {...} }`, `{ Hsla: {...} }` and every
  other variant). `bevy-ts` exports `srgb()`, `linear()`, `hsl()` and `hex()` that build these,
  and a `ColorValue` type.
- One colour parser, `bevy_ts::color`, owns every colour in bevy-ts. Today four copies parse hex
  only: `ui.rs`, `intrinsics.rs`, `recipes.rs` and `bevy-ts-material`'s `layout.rs`. All four call
  the one parser, so `<material color>`, `<mesh color>`, UI colours, script-material `color`
  params and the `cuboid` recipe take every form.
- A layout-tier component's hole that reflection can write (a `Color`, an enum with data, an
  `Option`, a nested struct) is a TSX prop, written through reflection on insert and on change.
  It stays invisible to TS systems; columns do not change.
- `camera3d` takes an optional `projection` param in the same shape as `<Projection value>`.
- Generated types: `Color` fields type as `ColorValue`; layout-tier classes gain TSX-only props for
  their writable holes; enum-shaped reflect-tier components type `value`.

### Holes this closes (audit of the shipped `bevy-ts/bevy` types)

- `Color` holes: `DirectionalLight`, `PointLight`, `SpotLight`, `RectLight`, `AmbientLight`
  `.color`; `DistanceFog` `.color`, `.directional_light_color`; `FogVolume` `.fog_color`,
  `.light_tint`; `VolumetricFog` `.ambient_color`; `Vignette` `.color`; `Sprite` `.color`;
  `Text2dShadow`, `TextShadow`, `ImageNode`, `InlineImage` `.color`; `InfiniteGridSettings` axis
  and line colours.
- Enum-with-data holes: `Projection` (whole component, with `ScalingMode`), `DistanceFog`
  `.falloff`, `Camera` `.viewport` (`Option`), `PlaybackSettings` `.volume`, `.spatial_scale`,
  `RenderDebugOverlay` `.mode`, `ScreenSpaceAmbientOcclusion` `.quality_level`, `Window` `.mode`,
  `.position`; nested-struct holes such as `Bloom` `.prefilter` and `Lightmap` `.uv_rect`.
- Not closed, and why: `Handle<T>` holes keep their existing asset-path rule; opaque types
  (`usize`, `Range<f32>`, `Duration`, `TypeId`) and computed components (`ComputedNode`,
  `TextLayoutInfo`) are not authored. UI `Val` and `Node` fields belong to bevy-ui-coverage's
  `style` prop; they work through this writer too but that change owns their TSX surface.

## Capabilities

### New Capabilities

None.

### Modified Capabilities

- `native-components`: TSX writes enums, `Option`s and `Color`s by reflection; writable holes of
  layout-tier types become TSX props; generated types carry `ColorValue` and enum `value`.
- `ts-systems`: the `camera3d` recipe takes `projection`; recipe colours take every colour form.

## Impact

- Code: new `crates/bevy-ts/src/color.rs` and `crates/bevy-ts/src/reflect_value.rs`;
  `crates/bevy-ts-tsx/src/reflected.rs`, `resolve.rs`, `reconcile.rs`, `ui.rs`, `intrinsics.rs`;
  `crates/bevy-ts/src/recipes.rs`; `crates/bevy-ts-material/src/layout.rs`;
  `crates/bevy-ts-cli/src/types.rs`; `packages/bevy-ts` (`index.d.ts`, `bevy.d.ts` regenerated,
  colour helpers in the JS runtime).
- Shared seam with bevy-ui-coverage: `reflected.rs`'s `set()` (its `value` → `.0` newtype mapping
  stays). This change owns enum and `Color` writing and the colour parser.
- Docs: `docs/native.md` (enums and colours in TSX), `docs/materials.md` (colour forms).
- Protocol: no wire change is planned (TSX values do not cross the protocol). If the regenerated
  `protocol/bevy-ts.openrpc.json` differs at landing, bump `1.24.0` to the next free minor.
- No new dependency, infrastructure or service. Nothing here needs the operator's go: this is
  reflection work on existing paths, written only when a prop changes.
- Behaviour change: a partial object for a struct field now fills the rest from defaults instead
  of failing; no script in the repo relied on the failure.
