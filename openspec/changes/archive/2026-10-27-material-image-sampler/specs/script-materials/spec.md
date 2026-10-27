## ADDED Requirements

### Requirement: A texture samples with its image's sampler
A script material's texture SHALL bind its image's own sampler at `<name>_sampler`, as Bevy prepares it into the image's `GpuImage`: an `ImageSampler::Descriptor` as given, `ImageSampler::Default` as the `ImagePlugin`'s default sampler. An unset texture SHALL bind the white fallback image with the fallback's sampler. No sampler SHALL be shared by every texture regardless of its image. When an image's sampler changes (`assets.set(img, { sampler })`), every script material holding the image SHALL bind the new sampler within a frame of the change, with no new material instance. A rewrite that keeps the sampler (`setData`, `writeRect`) SHALL re-prepare no material after the first. A material prepared in the frame its image changes SHALL bind the image's new sampler.

#### Scenario: A clamping image is not wrapped
- **WHEN** a script material samples a 2x1 image (left texel red, right blue) made by `Image.new` with `{ sampler: { Descriptor: { address_mode_u: "ClampToEdge", address_mode_v: "ClampToEdge" } } }` at `u = 1.01`
- **THEN** a headless render draws the right texel's blue, with no red

#### Scenario: A repeating image wraps
- **WHEN** the same image has `address_mode_u: "Repeat"`, Linear filters, and is sampled at `u = 1.01`
- **THEN** the pixel blends the right texel with the left, so it has both red and blue

#### Scenario: The default sampler is the ImagePlugin's
- **WHEN** the image is made with no sampler and the app uses `ImagePlugin`'s default (ClampToEdge)
- **THEN** sampling at `u = 1.01` draws blue, as `ClampToEdge` does

#### Scenario: A changed sampler rebinds
- **WHEN** a system calls `assets.set(img, { sampler: { Descriptor: { address_mode_u: "ClampToEdge", ... } } })` on a repeating image a script material samples
- **THEN** within frames the pixel turns from the red and blue blend to blue, and `Assets<ScriptMaterial>` holds the same instances
