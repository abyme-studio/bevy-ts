## ADDED Requirements

### Requirement: A texture prop takes an asset path or an image ref
A script material's texture prop SHALL take an asset path (loaded by the `AssetServer`, as before) or an `AssetRef<Image>` (`{ $asset: n, type }`), resolved through reflect-assets' `ScriptAssets::handle_for`. The prop check SHALL accept a ref's shape and its type without a world; any other value, or a ref of another asset type, SHALL fail naming the kind, the texture and `takes an asset path or an image ref` (or both types). A stale ref SHALL fail at the element with "stale asset ref". Equal props, the same ref included, SHALL share one instance. The instance SHALL hold the image's handle, so the image stays while a material uses it after the script drops its ref.

#### Scenario: A ref as a texture
- **WHEN** an element writes `<material kind={Town} owners={tex} />` with `tex` from `images.add`
- **THEN** the instance's texture is the image at `tex`'s id, and a headless render samples its bytes

#### Scenario: A ref of another type
- **WHEN** an element passes a mesh ref as a texture prop
- **THEN** the load fails at the element naming the kind, the texture, `Mesh` and `Image`

#### Scenario: A rewritten texture
- **WHEN** a system calls `setData` on the image a script material samples
- **THEN** the instance is unchanged and the next rendered frame samples the new bytes
