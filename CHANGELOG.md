# Changelog

Changes a user of bevy-ts must act on, newest first.

## Unreleased

- **Breaking:** a mesh faded by `MaterialAlpha` holds `MeshMaterial3d<FadeMaterial>`
  (`ExtendedMaterial<StandardMaterial, Fade>`) while faded, not `MeshMaterial3d<StandardMaterial>`.
  Code that reads a faded mesh's `StandardMaterial` finds none; `bevy_ts_material::standard_of`
  returns its original. An opaque original no longer becomes `Blend`: the copy keeps its mode and
  dithers, and its shadow fades with it (`docs/materials.md`).
