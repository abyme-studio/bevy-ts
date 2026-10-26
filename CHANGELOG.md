# Changelog

Changes a user of bevy-ts must act on, newest first.

## Unreleased

- Protocol 1.36.0: `mutate_components` takes `.field[i]` and `.field[i].member` on a list field,
  writing that item in place; past the end fails naming the list's length. It used to refuse them.
- Protocol 1.35.0: a system's `schedule.kind` (and the `Stage` enum) may be `"predicate"`, a
  TS predicate condition passed to `runIf`. A reader that switches on the stage handles it.
- `resourceExists(R)` is false after `removeResource(R)`, and a system taking `Res<R>` is
  skipped, as in Bevy. Before, both went on as if `R` were there.
- Protocol 1.34.0: a list field's item type `of` may be a vector, `entity`, `enum` or
  `{ kind: "struct", fields: [{ name, type }] }` (`path: Vec3[]`, `hits: { at: Vec3, by: Entity }[]`).
  A reader that switches on a field type's `kind` handles `struct`; its value is an object of every
  member. A Rust system reads such a list as a `Vec<T>` of the item's layout: `Vec4` and `Quat`
  items as `[f32; 4]`, an object as a `#[repr(C)]` struct of its members.
- A list field on a `Mut` row, `ResMut` or `Local` is now a plain array a system edits in place
  (`push`, `[i] =`, `sort`); the edits reach Rust at the end of the run. It used to be frozen, and
  `push` threw a `TypeError`. A read row's list is still frozen.
- Protocol 1.33.0: a TS component field may be `{ kind: "string" }` (`label: string`) or
  `{ kind: "list", of }` (`scores: u32[]`), heap fields. A reader that switches on a field type's
  `kind` handles both; a component with one has an 8-byte header before its first field, already in
  the offsets and `size` the layout gives.
- **Breaking:** a mesh faded by `MaterialAlpha` holds `MeshMaterial3d<FadeMaterial>`
  (`ExtendedMaterial<StandardMaterial, Fade>`) while faded, not `MeshMaterial3d<StandardMaterial>`.
  Code that reads a faded mesh's `StandardMaterial` finds none; `bevy_ts_material::standard_of`
  returns its original. An opaque original no longer becomes `Blend`: the copy keeps its mode and
  dithers, and its shadow fades with it (`docs/materials.md`).
