# The viewport and asset previews

`bevy-ts dev --viewport` (macOS) runs the engine with no window of its own. It renders into
IOSurfaces another process shows (abyme's Bevy studio), paced by that process's display, and takes
its input over the same Mach channel. The contract is `protocol/bevy-ts.openrpc.json`; the code is
`crates/bevy-ts-viewport`.

## Viewports

One engine serves several viewports at once. Each has its own Mach service, ring of three
surfaces, pacing, input queue and counters.

- `bevy-ts/viewport_attach { w, h, scale, hz?, view? }` makes a viewport for `view`: `"game"` (the
  default) or `{ preview: <id> }` from `asset_preview`. It answers `{ viewport, service, protocol }`.
  The game viewport's id is always 0.
- Attaching a view that already has a viewport replaces that one; other views are untouched.
- `viewport_detach`, `viewport_resize`, `viewport_input`, `viewport_camera` and `viewport_stats`
  take an optional `viewport`; without one they mean the game viewport, so a 1.7 client is
  unchanged.
- The virtual primary window, edit mode, picking and edits belong to the game viewport alone.
- A viewport renders only while attached and visible. The leading viewer's ticks pace the engine:
  the game's while it is visible, else the first visible preview's. A preview never adds updates
  to a visible game; it renders in the game's frame.

## Asset previews

`bevy-ts/asset_preview { path }` loads a mesh (`.gltf`, `.glb`, `.obj`) into a scene of its own and
answers `{ preview, kind, animations, pending }`. While the asset loads it answers
`pending: true`; ask again. Asking for the same path again answers the same preview.

- **Isolation.** A preview takes a render layer from 20 to 29 (ten at once; an eleventh closes the
  least recently attached) and sits under a root far from the game's origin. Its scene, a
  three-point light rig on its layer only, its camera and its window carry `PreviewOnly`, which
  requires `ScriptHidden`: no TS query matches them, and they are absent from `bevy-ts/entities`,
  `entities+watch`, `tsx_tree`, `.bsn` writing and edit-mode picking. The editor camera does not
  see layers 20 to 29.
- **Look.** The camera copies the game's main camera's tonemapping, exposure and environment map
  when there is one, so a model looks as it does in the game.
- **Input**, handled in Rust on the preview viewport's channel: a drag orbits, the wheel or a
  pinch zooms, `f` reframes. `viewport_camera { viewport }` reads or sets the pose, relative to the
  preview's root; `frame: true` reframes.
- **Animations.** `asset_preview_control { preview, animation?, play?, speed? }` plays, pauses or
  switches the glTF's animations by name (`Animation<i>` when unnamed).
- **Lifetime.** `asset_preview_close { preview }` despawns it. A preview with no viewport attached
  closes after 30 seconds, so a pane that closes and reopens does not reload.

## An engine that is always up

The studio keeps the project's engine running in edit mode while the project is open (operator
ruling 2026-10-20). A project whose TS fails on the first load runs with an empty world: `hello`,
the asset methods and previews answer, the error is on the log and `reload+watch`, and the next
good save applies as generation 1.

## For abyme's `bevy-studio`

When `hello.protocol` is 1.8 or later, the studio's glTF view can show the engine's preview
instead of three.js:

1. `asset_preview { path }` until it answers a `preview` id.
2. `viewport_attach { w, h, scale, hz, view: { preview } }` and show its service in a hole of its
   own, ticking it and forwarding pointer, wheel, pinch and key input as for the game's hole.
3. `asset_preview_control` for the animation list the answer gave; `viewport_detach { viewport }`
   when the pane closes.

On a 1.7 engine `asset_preview` is method-not-found: keep three.js.
