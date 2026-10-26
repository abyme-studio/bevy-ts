## ADDED Requirements

### Requirement: `ResMut<Assets<Image>>` makes an image from TS bytes
A TS system that takes `images: ResMut<Assets<Image>>` SHALL get Bevy's image assets. `Image.new([width, height], format, data, options?)` SHALL record a 2D image in JS and cross nothing. `format` SHALL be one of `TextureFormat.R8Unorm`, `TextureFormat.Rg8Unorm`, `TextureFormat.Rgba8Unorm` and `TextureFormat.Rgba8UnormSrgb`; `data` SHALL be a `Uint8Array` of exactly `width × height × bytes per pixel` bytes. `images.add(image)` SHALL build Bevy's `Image::new` (dimension `D2`, `RenderAssetUsages::default()`, usage including `COPY_DST`) in one host call, the bytes passed as a view and copied once, add it to `Assets<Image>`, and return an `AssetRef<Image>` holding a strong handle, freed as script-assets' strong refs are. A wrong length, a zero or oversized side, or another format SHALL throw naming the image's size, format and the expected length, and add nothing. `options.sampler` SHALL take reflect-assets' `ImageSampler` shape, written by the same writer as `Image.load(path, { sampler })`.

#### Scenario: An image from bytes
- **WHEN** a system calls `images.add(Image.new([4, 2], TextureFormat.Rgba8UnormSrgb, bytes))` with 32 bytes
- **THEN** `Assets<Image>` holds a 4×2 `Rgba8UnormSrgb` image whose data equals `bytes`, and the ref's id is that image's id

#### Scenario: The wrong length
- **WHEN** a system adds a 4×2 `Rgba8Unorm` image with 31 bytes
- **THEN** the call throws naming `4×2`, `Rgba8Unorm` and `32 bytes`, and `Assets<Image>` is unchanged

#### Scenario: A sampler at creation
- **WHEN** a system adds an image with `{ sampler: { Descriptor: { mag_filter: "Nearest", min_filter: "Nearest" } } }`
- **THEN** the image's `sampler` is that descriptor, and `assets.get(ref).sampler` reads it back in the same shape

### Requirement: An image is rewritten in place
`images.getMut(ref)` SHALL return the live image, or `undefined` when it is gone. `setData(bytes)` SHALL copy a `Uint8Array` of the image's exact length into the image's own data in one host call, with no allocation. `writeRect(x, y, w, h, packedRows)` SHALL copy `w × h` pixels of tightly packed rows into the rect at `x`, `y` in one host call, with no allocation. A length that does not match, or a rect outside the image, SHALL throw naming the image's size and the rect or length, and leave the image as it was. Several writes to one image by one system in one frame SHALL send one `AssetEvent::Modified`. A rewrite SHALL keep the image's size, format and sampler, so Bevy keeps its GPU texture.

#### Scenario: The whole image
- **WHEN** a system calls `images.getMut(ref)!.setData(next)` on a 4×2 RGBA image
- **THEN** the image's data equals `next` and a `MessageReader<AssetEvent<Image>>` sees one `Modified` for its id that frame

#### Scenario: A rect
- **WHEN** a system calls `writeRect(1, 0, 2, 2, patch)` with 16 bytes on a 4×2 RGBA image of zeros
- **THEN** pixels (1,0), (2,0), (1,1) and (2,1) equal `patch` in row order and every other pixel is still zero

#### Scenario: A rect outside the image
- **WHEN** a system calls `writeRect(3, 0, 2, 1, patch)` on a 4-wide image
- **THEN** the call throws naming the rect and the image's size, and the data is unchanged

#### Scenario: Two writes, one event
- **WHEN** a system calls `setData` and then `writeRect` on one image in one run
- **THEN** a reader sees exactly one `Modified` for its id

### Requirement: An image is read and removed
`images.get(ref)` SHALL return the image read-only, or `undefined` when it is gone: `size` as `[width, height]`, `format` as a `TextureFormat`, and `readData(out?)` copying its bytes into `out` (or a new `Uint8Array`) in one host call. A loaded image of a format the four do not cover SHALL read its size and throw on `format` and `readData`, naming the format. `images.remove(ref)` SHALL be Bevy's `Assets::remove` and return whether it removed one; the ref then goes stale.

#### Scenario: Read back
- **WHEN** a system reads `images.get(ref)` of an image it added
- **THEN** `size`, `format` and `readData()` equal what it added

#### Scenario: Remove
- **WHEN** a system calls `images.remove(ref)`
- **THEN** it returns `true`, the image is gone from `Assets<Image>`, and a later `images.get(ref)` throws "stale asset ref"

### Requirement: An image ref goes where a `Handle<Image>` goes
A ref from `images.add` SHALL go in any `Handle<Image>` slot reflect-assets' writer reaches (`assets.add(StandardMaterial, { base_color_texture: ref })`, a TSX prop) and in a script material's texture prop. A rewrite SHALL reach what samples the image the next rendered frame.

#### Scenario: A StandardMaterial samples a rewritten image
- **WHEN** a headless render draws a quad with `StandardMaterial { base_color_texture: ref, unlit: true }`, and a later run calls `setData` with another colour
- **THEN** the frame after the rewrite reads the new colour at the quad's centre
