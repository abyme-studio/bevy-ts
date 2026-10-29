# Changelog

Changes a user of bevy-ts must act on, newest first.

## Unreleased

- `pass()` from `bevy-ts/material` and `<pass kind>` on a `Camera2d` run a fullscreen WGSL fragment
  over the camera's image, with bindable params and engine-written `time`, `texel`, `size` and
  `view`; at most four kinds at once (docs/materials.md, Fullscreen passes).
- World text places a frame's new glyphs in the atlas sorted by their raster, not in the order
  labels were laid out. The same labels spawned in one frame in another order draw the same pixels;
  before, letter edges could differ under MSAA. Glyphs first used in different frames still go in
  frame order: for the same pixels every run, spawn labels in one frame (`ReadyHold` alone does not).
- `<bitmapText font>` draws text in an XNA SpriteFont (JSON beside its PNG), one sprite per glyph,
  laid out as MonoGame does (docs/2d.md).
- `<SpriteClip first last fps mode />` steps a sprite's atlas index (`Repeat`, `Once`, `PingPong`)
  on virtual time (docs/2d.md).
- `<PixelCamera zoom ease />` zooms a `Camera2d` by whole numbers, eased, and draws its view from
  whole screen pixels. It owns the projection's `scale` (docs/2d.md).
- `<YSort layer offset />` draws a 2D entity by its base y within a layer, and `<ScreenSize />`
  keeps one size on screen at every zoom. Each owns what it writes, `Transform` z and scale: a
  script's write is overwritten (docs/2d.md).
- `bevy-ts.json` takes `assetSources`, folders outside the project loaded as `name://path`
  (docs/assets.md). A build names them in `game.json` and copies none unless `--copy-sources`; a
  built game whose source folder is missing exits 1 at start naming it (docs/build.md).
- `bevy-ts.json` takes `"imageSampler": "nearest"`: every image on the default sampler samples
  nearest, in dev and in a built game (docs/assets.md). Bevy's text atlas follows it.
- `bevy-ts/ready`'s `ReadyHold` (bevy-ts's own, not Bevy's): a script holds a run's ready frames
  until its data comes (`hold`, `release`, `isHeld`). A shot and the fixed step wait it out; one
  never released fails the shot naming it. A game whose data comes over the network holds from
  `Startup` until the data is applied, so its shots stop depending on load.
- The stall watchdog's `Update stopped:` line is also logged at error level, and ends with what a
  waiting shot waits on. A shot that gives up names everything it waited on.
- **Breaking:** `BEVY_TS_SCREENSHOT` no longer shoots anyway. When it cannot be ready after 60 s
  with no progress, or Bevy renders, reads back or writes nothing in time, it saves no file, logs
  an error naming the path and the reason, and the run exits with code 1. A script that took any
  PNG as success sees the failure. The 60 s, for it and for `bevy-ts/screenshot`, now restarts on
  progress (a pipeline finished, fewer assets loading), so a cold build is waited out.
- **Breaking:** `bevy-ts play`, `bevy-ts dev` and `bevy-ts skeleton` exit with the code of the game's `AppExit`: a
  game that exits with `AppExit::Error` sets it. They used to exit 0.
- A shot is written to a temp file beside its path and renamed onto it: a reader polling the path
  never reads part of a PNG.

- `bevy_ts::commands::apply` takes a fifth argument, `label`: what queued the ops, named in a
  delayed queue's warnings. A Rust caller passes its own context (`"my plugin"`).
- Protocol 1.38.0: a TS component's `x-bevy-ts` may carry `relationship` (`{ role, counterpart,
  linkedSpawn?, allowSelfReferential? }`). A relationship target's layout has no fields and
  `get_components` reads it as an array of entities; `mutate_components` refuses a target and
  re-inserts a source. A reader that walks every TS component's fields sees none on a target.
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
