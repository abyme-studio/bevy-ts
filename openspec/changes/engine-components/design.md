## Context

`bevy-ts/schema` (crates/bevy-ts-remote/src/values.rs) returns stock `registry.schema` entries for reflected Rust components and builds its own for TS components, marking those with `x-bevy-ts`. Stock entries already carry `crateName`, but a crate name alone cannot tell Bevy's crates from a third-party one (`bevy_rapier3d` starts with `bevy_`), and nothing says which components the engine maintains. abyme's inspector (team bevy-inspector) needs both to hide engine-maintained components, offer Add component, and pick an icon. The field shape was agreed with spec@bevy-inspector on 2026-10-22.

## Goals / Non-Goals

**Goals:**
- One object on every schema entry that says origin, internal, addable and category.
- bevy-ts owns the internal list and the category table, in one file.
- A minor protocol bump that older studios ignore.

**Non-Goals:**
- Labels and descriptions. Bevy 0.20's schema has no docs; the studio splits CamelCase.
- Adding or removing a component in a level (spec@level-edit's LevelOps). `addable` only says an editor may offer it.
- New filters on `SchemaParams`. The studio filters client side.

## Decisions

### A new namespaced key, not top-level fields or a wider `x-bevy-ts`
`x-bevy-ts-editor` sits beside `x-bevy-ts`. Top-level keys belong to stock `registry.schema`, and Bevy may add its own. `x-bevy-ts` means "a TS component" and clients test for its presence; putting it on Rust entries would be a major bump. A new optional key is a minor one.

### Origin from the type's crate, with known lists
`TypeRegistration`'s `type_info().type_path_table().crate_name()` gives the crate. bevy-ts decides:
- `engine`: the crate is in `BEVY_CRATES`, the crate names of Bevy 0.20's workspace (`bevy`, `bevy_app`, `bevy_transform`, `bevy_render`, …), is `bevy_ts` or starts with `bevy_ts_` (bevy-ts's crates and ports, `bevy_ts_rapier3d` included).
- `game`: every TS component, and a type whose crate is the native crate named in `EngineIdentity` (its package name with `-` as `_`).
- `plugin`: anything else.

A test runs the engine with Bevy's `DefaultPlugins` and no ports and fails if any registered component has origin `plugin`, so a Bevy upgrade that adds a crate fails CI until `BEVY_CRATES` is updated. Bevy is pinned exactly (native-crate spec), so the list moves only with a Bevy bump.

Alternative: derive Bevy's crates from Cargo metadata at build time. Rejected: the prebuilt engine has no project Cargo metadata, and a fixed list is simpler to read.

### Internal is a list of type paths
`INTERNAL` in `editor.rs`: GlobalTransform, TransformTreeChanged, InheritedVisibility, ViewVisibility, VisibilityClass, Aabb, Children, TransformGizmoFocus, the render-world sync markers, frusta and visible-entity caches, UI computed nodes and text layout, and bevy-ts's own editor markers (`EditorOnly`, `PreviewOnly`, `ScriptHidden`, `TsSpawned`). The implementer completes it by listing Bevy 0.20's registered components in the cube example and marking every one that a system writes and no user sets. A path in `INTERNAL` that the registry does not know fails a test, so the list cannot rot.

Alternative: infer "maintained" from required components or from lacking `ReflectDefault`. Rejected: GlobalTransform has `ReflectDefault`, and many authored components lack it.

### Addable from reflection
Addable = `reflectTypes` contains `Component` and (`Default` or `FromWorld`), `componentInfo.mutable` is true, and not internal. TS: declared by the live generation. This only gates what Add component offers.

### Category from a table keyed by type path, then crate
`CATEGORIES` maps a type path first, then a crate, to a category: `bevy_transform` → transform, `bevy_light` and light types → light, `Camera*` → camera, `bevy_audio` → audio, `bevy_ui` and `bevy_text` → ui, `bevy_animation` → animation, `bevy_render`/`bevy_pbr`/`bevy_sprite`/`bevy_mesh` → render, `bevy_rapier3d` and `bevy_ts_rapier3d` → physics. Unlisted engine and plugin types are `other`; game types are `game`.

## Risks / Trade-offs

- [The internal list misses a component] → the studio shows it, as today. The list is one file anyone can extend in a patch release.
- [A Bevy upgrade renames or adds crates] → the `plugin`-in-a-bare-engine test fails until `BEVY_CRATES` is updated.
- [A plugin crate is categorised `other`] → generic icon only; adding its crate to `CATEGORIES` fixes it.

## Migration Plan

Protocol 1.11.0. Studios on 1.10.0 ignore the key; studios on 1.11.0 treat its absence as an older engine. No rollback concern: removing the key later would be a major bump.
