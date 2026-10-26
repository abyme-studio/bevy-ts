# Changelog

Changes a user of bevy-ts must act on, newest first.

## Unreleased

- Protocol 1.32.0: a TS component field may be `{ kind: "string" }` (`label: string`) or
  `{ kind: "list", of }` (`scores: u32[]`), heap fields. A reader that switches on a field type's
  `kind` handles both; a component with one has an 8-byte header before its first field, already in
  the offsets and `size` the layout gives.
- **Breaking:** a mesh faded by `MaterialAlpha` holds `MeshMaterial3d<FadeMaterial>`
  (`ExtendedMaterial<StandardMaterial, Fade>`) while faded, not `MeshMaterial3d<StandardMaterial>`.
  Code that reads a faded mesh's `StandardMaterial` finds none; `bevy_ts_material::standard_of`
  returns its original. An opaque original no longer becomes `Blend`: the copy keeps its mode and
  dithers, and its shadow fades with it (`docs/materials.md`).
