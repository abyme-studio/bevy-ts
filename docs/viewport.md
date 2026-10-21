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

## The asset list and thumbnails

`bevy-ts/assets { prefix?, kind? }` lists every file under the asset root without loading any.
Each entry has `path`, `kind`, `loader`, `size`, `modified`, `loadState` and `thumbnail`; a loaded
glTF also has its `labels`. `assets+watch` sends the whole list once, then each file added,
modified, removed or changing load state.

| kind | extensions | `thumbnail` | what it draws |
|---|---|---|---|
| `image` | png, jpg, ktx2, hdr, … | true | the image scaled to fit, on clear, at once |
| `mesh` | gltf, glb, obj | true | the model, rendered offscreen |
| `scene` | `.bsn` | true in a game that reads levels | the level, rendered offscreen |
| `scene` | `.scn.ron` | false | — |
| `audio`, `font`, `shader`, `other` | | false | — |

`bevy-ts/asset_thumbnail { path, size }` (`size` at most 512) answers `{ png, pending }`. An image
answers at once. A mesh or a level answers `pending: true` until it is drawn; ask again each frame
or so. Anything with `thumbnail: false`, or that fails to load or compose, answers `png: null` with
no `pending`. Answers are cached by path, modification time and size.

A mesh or level is drawn on render layer 30 by a camera of its own, from three-quarters, framed
to its bounds, then read back and despawned. Nothing on the viewport's ring or the game's cameras
changes. One is drawn at a time: the others asked for wait, loaded, until it is read back, so no
picture holds another's subject. Whatever a glTF's scene spawns under a thumbnail is marked
`EditorOnly` and put on layer 30 the moment it is attached.

A level is composed as loading it would: includes resolved, scene components expanded through
their TS `scene()`. Only what draws is spawned: `Transform`, `Visibility`, `<mesh>`, `<material>`
(script materials too), `Mesh3d`/`MeshMaterial3d`, lights and glTF scene roots. Every entity carries
`EditorOnly` (so `ScriptHidden`) from its first insert, so no TS query or observer, no physics and
no reconciler sees it, and it is absent from `tsx_tree` and `entities+watch`. A level whose look
comes from a TS system at runtime shows its authored look.

What a level thumbnail costs (asset-catalog task 3.4: release, Apple M5 Max under a load average
of about 22 from other builds, 128 px, three rounds each; `thumbnail-bench`):

| level | entities | compose and spawn | frames to PNG | frame while rendering, median / max | idle frame |
|---|---|---|---|---|---|
| `levels/arena.bsn` | 10 | 0.5–1.2 ms | 10 | 3.0–3.6 / 3.6–9.2 ms | 2.7 ms |
| generated, 100 enemies | 200 | 3.4–5.3 ms | 10 | 2.9–3.4 / 6.2–6.9 ms | 2.7 ms |

Composing runs once, in the frame the level's files finish loading; that frame is the max. The
other frames cost what a glTF thumbnail does.

## Picking

`bevy-ts/viewport_pick { x, y }` (edit mode) answers every hit at that point, nearest first, each
with `entity`, `depth`, its world `point` `[x, y, z]` and the entity a click there `selects`, and a
`ground` `[x, y, z]`: where the ray meets the plane y = 0 in front of the camera, or null when it
does not (pointing at the sky). A model dropped on empty floor goes at `ground`.

## An engine that is always up

The studio keeps the project's engine running in edit mode while the project is open (operator
ruling 2026-10-20). A project whose TS fails on the first load runs with an empty world: `hello`,
the asset methods and previews answer, the error is on the log and `reload+watch`, and the next
good save applies as generation 1.

The studio starts that engine itself; a project adds nothing for it, no abyme process entry. For a
folder with `bevy-ts.json` and no game serving it, the studio runs
`npx --yes bevy-ts dev --viewport <entry> --remote-port <first free port from 15702>` as a kept
process. The entry is the first of `src/game.tsx`, `src/game.ts`, `src/main.tsx` and `src/main.ts`
that exists. The studio switches to edit mode once `hello` answers, and ends the process when no page
shows the project.

## For abyme's `bevy-studio`

When `hello.protocol` is 1.8 or later, the studio's glTF view can show the engine's preview
instead of three.js:

1. `asset_preview { path }` until it answers a `preview` id.
2. `viewport_attach { w, h, scale, hz, view: { preview } }` and show its service in a hole of its
   own, ticking it and forwarding pointer, wheel, pinch and key input as for the game's hole.
3. `asset_preview_control` for the animation list the answer gave; `viewport_detach { viewport }`
   when the pane closes.

On a 1.7 engine `asset_preview` is method-not-found: keep three.js.
