## Why

The studio's inspector lists every component on an entity, the engine's own included: GlobalTransform, InheritedVisibility, ViewVisibility, VisibilityClass, Aabb and more. The bevy editor study (section 6, "An inspector for people") asks it to hide what the engine maintains, offer Add component, and show an icon per component. Only bevy-ts can say which components are the engine's own: `bevy-ts/schema` sends schemas, but nothing in them says who owns a component, whether anyone authors it, or whether it can be added.

## What Changes

- Every entry of `bevy-ts/schema`, TS and Rust, gains an `x-bevy-ts-editor` object: `origin`, `internal`, `addable` and `category`.
- `origin` is `engine` (Bevy's crates and bevy-ts's), `game` (TS components and the project's native crate) or `plugin` (any other crate).
- `internal` marks components the engine maintains and nobody authors. bevy-ts owns that list.
- `addable` says Add component may offer it.
- `category` is one of `transform`, `render`, `light`, `camera`, `physics`, `audio`, `ui`, `animation`, `game`, `other`, for the studio's icon.
- Protocol 1.10.0 → 1.11.0, a minor bump: one new optional key, nothing removed. `x-bevy-ts` is unchanged and still marks TS components only.

The shape was agreed with spec@bevy-inspector, who writes abyme's half. Absent object means an older engine: the studio shows everything, offers no Add component and uses a generic icon.

Not in this change: labels and descriptions (the studio splits CamelCase; Bevy 0.20's schema has no docs), and adding or removing components in a level (spec@level-edit's LevelOps).

## Capabilities

### New Capabilities

None.

### Modified Capabilities

- `remote-protocol`: `bevy-ts/schema` entries carry `x-bevy-ts-editor`, saying a component's origin, whether it is internal or addable, and its category.

## Impact

- `protocol/bevy-ts.openrpc.json`: a new `EditorInfo` schema and the `x-bevy-ts-editor` property on `ComponentSchema`; version 1.11.0. Regenerated Rust (`crates/bevy-ts-remote/src/protocol.rs`) and TS (`packages/bevy-ts/protocol.d.ts`).
- `crates/bevy-ts-remote`: a new `editor.rs` holding the internal list, the Bevy crate list and the category table; `values.rs` adds the object to every schema.
- `crates/bevy-ts-cli`: nothing new. The native crate's name already reaches the engine in `EngineIdentity`.
- abyme's inspector (team bevy-inspector) reads the new key. It lands separately and works against 1.10.0 engines.
- No new dependency, no runtime cost outside `bevy-ts/schema` calls.
