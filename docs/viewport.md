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

## Gizmo tools

- `bevy-ts/viewport_tools { mode?, space?, snap? }` sets the transform gizmo's mode (`translate`,
  `rotate`, `scale`), space (`world`, `local`) and snap. It answers
  `{ mode, space, snap, steps, origin }`; with no params it changes nothing.
- The steps are fixed, as Bevy's editor prototype sets them: translate 0.5, rotate 15 degrees,
  scale 0.1. An axis drag snaps the move along its axis.
- `bevy-ts/viewport_tools+watch` sends the tools on open and on every change. `origin` is
  `editor` for a change by the method, `viewport` for one by `W`, `E`, `R` or `X` in the viewport,
  `engine` before any. No key toggles snap; the keys with ⌃, ⌘ or ⌥ held are not tool keys.
- A change during a gizmo drag takes effect on release: the drag keeps the field it started on.
- The tools start as `translate`, `world`, snap off, and are kept for the process, across mode
  switches and reloads. The method answers in play too; the tools act in edit mode.

## Orientation gizmo

In edit mode the game viewport shows the world's axes in its top-left corner: a 96 px square
inset 8 px, `+X` red, `+Y` green and `+Z` blue as filled ends, the negative axes as hollow ends,
turned as the editor camera sees them.

- A click with no drag on an end turns the editor camera to look down that axis at its focus,
  keeping its distance. `+Y` looks down to the orbit's pitch limit (1.54 rad), so the next orbit
  does not jump.
- A press in the square never picks, changes the selection or starts a gizmo drag.
- Not drawn in play (a click there reaches the game), in asset previews or in `bevy-ts dev`'s own
  window. A small camera on render layer 32 renders it into a texture, only on the two frames after
  the editor camera turned (and every frame for its first 1.5 s, while Bevy compiles its pipelines);
  a quad in front of the editor camera shows it at any scale up to 3x.
  Its cost is in `docs/research/viewport-tools.md`.

## An engine that is always up

The studio keeps the project's engine running in edit mode while the project is open (operator
ruling 2026-10-20). A project whose TS fails on the first load runs with an empty world: `hello`,
the asset methods and previews answer, the error is on the log and `reload+watch`, and the next
good save applies as generation 1.

The studio starts that engine itself; a project adds nothing for it, no abyme process entry. For a
folder with `bevy-ts.json` and no game serving it, the studio runs
`npx --yes bevy-ts dev --viewport --edit <entry> --remote-port <first free port from 15702>` as a
kept process. The entry is the first of `src/game.tsx`, `src/game.ts`, `src/main.tsx` and
`src/main.ts` that exists. The studio ends the process when no page shows the project.

`--edit` boots the engine in edit mode, as Unity and Godot open a project: the TS loads and
registers, the scenes and the levels `app.level()` names load and show from the editor camera, and
no TS system, observer or `Startup` runs. `Time<Virtual>` is paused from the first frame. So what
edit mode shows is what the scene describes; a camera or light a `Startup` spawns appears on Play.
The first `viewport_mode { mode: "play" }` starts the game: its `Startup` runs once, then the
stages. `bevy-ts/step` before that starts it too and stays in edit; `pause { paused: false }` fails
with "the game has not started; switch to play". Triggers raised in edit mode are dropped, never
fired on Play. A Rust rebuild of a project engine comes back in edit mode. `--edit` with
`--headless` is refused.

## Play and Stop

Play runs the game; Stop throws the play world away and rebuilds it from source, as Unity re-reads
the scene. A Stop is any `viewport_mode { mode: "edit" }` from play, paused or not. The engine keeps
no copy of the world: it holds nothing unsaved (`level_write` writes before it answers; unsaved
TSX text is the studio's).

Stop re-applies the live generation's module set as a `restart` reload does, without compiling, so
a save applied during play is what it rebuilds, and a file broken since does not stop it. It resets:

- every entity TS spawned, every TSX node and tombstone: each scene mounts as if first;
- which `Startup` systems ran: `started` is false, and the next Play runs `Startup` again;
- `Local` storage, TS resources (back to their `insertResource` values), TS states and pending
  messages;
- the JS engine, with a fresh isolate;
- `Time<Virtual>`, `Time<Fixed>` and the script frame count, from 0 and paused.

It keeps the editor camera's pose, the gizmo tools, `ui`, `gamepadUnfocused`, the camera choice
(see "The game view"), and what Rust
spawned: a native crate resets its own state in `OnExit(EngineMode::Play)`, which runs first
(`docs/native.md`).

The levels open when Play was entered come back, each with its opener: one a client opened during
play closes, one it closed opens again, and `level+watch` streams both. A `ts` level the live
generation no longer declares stays closed. Each level is reconciled from disk before Stop answers.
A game that booted in play rather than `--edit` has no Play to go back to: its first Stop keeps the
levels that are open.

The selection follows by node id: a selected level or TSX entity is selected again with its new
entity, streamed with `origin: "engine"`; one the game spawned is dropped.

A Stop costs a restart. At 1,000 level instances it takes about 0.1 s, at 10,000 about a second;
`docs/research/stop-cost.md` has the numbers.

### A Play that can't run

`viewport_mode { mode: "play" }` fails, leaving the engine in edit with nothing run, when no
generation has been applied yet (`no-generation`) or the newest compile failed (`compile-errors`),
as Unity refuses Play with compiler errors. The JSON-RPC error has code -32000, a message the
studio shows as is (`src/game.tsx:3: Unexpected token`, the first error's file and line), and
`data: { reason }`. A Play while a compile runs plays the live generation.

### The game view

`viewport_mode { camera }` picks what the viewport shows: `auto` (the default), `editor` or `game`.
The engine keeps the choice until a call sets another, across Play and Stop.

In edit mode `auto` and `editor` show the editor camera with its tools. `game` shows what Play would
draw at that frame, as Unity's Game view does: every game camera whose own active flag is on, in its
`order`, with the game's UI as Play draws it whatever `ui` is. The editor camera, the gizmo's
overlay camera, the grid, outlines, icons, handles and orientation gizmo are off. The world stays
paused and runs no game code; `bevy-ts/step` renders the stepped frame through the game cameras. No
click selects, no handle drags, no edit key acts, and `viewport_pick` answers -32602 "Picking needs
the editor camera.". The editor camera keeps its pose and the selection is kept, both there again
when the editor view comes back. Switching between `editor` and `game` is not a Stop and changes
nothing in the world; the swap applies on the next frame, so no frame shows both views or neither.

`auto` and `editor` act the same today. `auto` stays a value of its own, answered as `auto`, so it
can mean more once there are two panes (editor and game side by side).

Play always shows the game cameras. `camera: "editor"` in a call that is in, or enters, Play is
refused with -32602 "The editor camera is not available in Play." and changes nothing; the watch
sends `cause: "failed"`. A Stop brings back the choice in force before Play.

`gameCameras` in the answer counts the game cameras Play would turn on: not the editor's or a
preview's, alive, their own active flag on. In Play it counts the ones on. Under `--edit` before the
first Play it counts only cameras levels and scenes spawned, since no `Startup` has run. With none,
the game view clears to the game's `ClearColor` (Bevy clears a window no camera draws), and the
studio says "No cameras rendering".

### `viewport_mode+watch`

Sends the mode as `viewport_mode` answers it (`mode`, `ui`, `gamepadUnfocused`, `paused`,
`started`, `camera`, `gameCameras`) plus `cause`, on open and on every change:

- `engine`: on open, and to a subscriber that comes back;
- `client`: a `viewport_mode`, `pause` or `step` call changed it;
- `restart`: a `restart` reload changed it;
- `failed`: a Play or a camera choice was refused; the mode is unchanged;
- `cameras`: only `gameCameras` changed: a reload, a level edit or a game system added or removed a
  game camera.

A `step` whose only change is the game camera count (a `Startup` that spawns the first camera, say)
is streamed as `cameras`, not `client`.

A Stop sends one message, after its rebuild. A new engine process starts a new stream: the studio
subscribes again after it reconnects.

## For abyme's `bevy-studio`

When `hello.protocol` is 1.8 or later, the studio's glTF view can show the engine's preview
instead of three.js:

1. `asset_preview { path }` until it answers a `preview` id.
2. `viewport_attach { w, h, scale, hz, view: { preview } }` and show its service in a hole of its
   own, ticking it and forwarding pointer, wheel, pinch and key input as for the game's hole.
3. `asset_preview_control` for the animation list the answer gave; `viewport_detach { viewport }`
   when the pane closes.

On a 1.7 engine `asset_preview` is method-not-found: keep three.js.

`viewport_mode` answers `started` from protocol 1.18: whether the game's `Startup` has run in this
world, false until the first play or step of an `--edit` engine. An answer without it is an older
engine: treat the game as started. An older engine also rejects `--edit` as an unknown option, so
the studio adds the flag only when the pinned `BEVY_TS` is 1.18 or later, and otherwise switches to
edit once `hello` answers, as before.

Play and Stop need protocol 1.24: `viewport_mode+watch` and a refused Play's error. On an older
engine the watch is method-not-found and Stop resumes where play stopped. The studio's one-time
switch to edit (operator ruling 2026-10-20) is a Stop: a game the operator started outside abyme
and already played is rebuilt from source when the studio finds it. abyme opens a project in
edit, so that is intended.

The game view needs protocol 1.25: `camera` and `gameCameras` on `viewport_mode` and its watch,
and the `cameras` cause. An older engine rejects `camera` as an unknown param, so the studio checks
`hello.protocol` before it shows the game-camera toggle.

The toolbar needs protocol 1.17 (`viewport_tools`). The studio does not fall back: on an older
engine it shows "bevy-ts too old".
