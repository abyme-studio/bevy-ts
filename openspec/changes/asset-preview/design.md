## Context

The viewport (archived change `viewport`, specs `shared-surface`, `edit-mode`, `asset-list`):

- **One viewport per engine.** The engine renders one camera into a ring of three IOSurfaces sent to one viewer over a Mach service. `viewport_attach` makes that service, and a second attach replaces the first.
- **`layer` stacks images in one hole.** It names stacked surfaces within that viewport (0 is the picture), reserved for overlays.
- **Thumbnails are stills.** `asset_thumbnail` already loads a mesh or glTF and renders it offscreen on render layer 30, with a fixed camera and light, then reads one frame back as a PNG (`crates/bevy-ts-viewport/src/assets.rs`).
- **Editor-only entities.** Editor entities carry `EditorOnly`, render only on layer 31, and are hidden from TS queries, the protocol and `.bsn` writing (`edit-mode`).

abyme's studio shows glTF files with three.js (`bevy-studio` task 5.2). The operator wants the engine to preview instead, with three.js kept until this lands.

## Goals / Non-Goals

**Goals:**
- A live preview of a mesh or glTF scene rendered by the game's own renderer, orbitable, beside the running game.
- The same preview with no game running.
- No change for clients that attach one game viewport.

**Non-Goals:**
- Previews of materials on a probe sphere, of audio, or of fonts beyond the thumbnail.
- Editing assets in the preview.
- Removing three.js from abyme.

## Decisions

### 1. Several viewports, keyed by id; `layer` keeps its meaning
`viewport_attach` takes `view`: `"game"` or `{ preview: <id> }`, defaulting to `"game"`. It answers `{ viewport, service, protocol }`.

- **What a viewport is:** its own Mach service (`dev.bevy-ts.viewport.<pid>.<n>`), ring of three IOSurfaces, pacing by its viewer's `tick`, input queue and camera target.
- **Attach rules:** attaching a view that already has a viewport replaces that one, today's rule now per view. Other views are untouched.
- **Ids on every method:** `viewport_detach`, `viewport_resize`, `viewport_input`, `viewport_camera` and `viewport_stats` take `viewport`. Without one they mean the game viewport.
- **What the game viewport alone keeps:** the virtual primary window, edit mode, picking and edits.
- **Rendering:** each viewport renders only while attached and visible, and each counts its own skipped frames.

- *Beat: previews as `layer` 1, 2, ... of the game viewport.* A layer is stacked inside one hole at the game's size, so a preview would sit on top of the game, not in a pane of its own with its own size, pacing and input.
- *Beat: a second engine process per preview.* That means a second V8 and Bevy, and a second copy of every loaded asset, for a pane that shows one model.

### 2. A preview is an isolated scene on a layer of its own
`asset_preview { path }` answers `{ preview, kind, animations }` once the asset is loaded. While loading it answers `pending: true`, and the client asks again, as with thumbnails. The scene is built like this:

- **Its own render layer:** a render layer from the preview pool, layers 20 to 29, so ten previews at once. An eleventh closes the least recently attached.
- **Its own world corner:** a root far from the game's origin, so physics and spatial queries see nothing. A preview has no colliders.
- **Its entities:** the asset's scene, a camera targeting the viewport's ring, and a three-point light rig. All of them carry `PreviewOnly`, which joins `EditorOnly`'s exclusions: no TS query matches them, and they are absent from `tsx_tree`, `entities+watch` and `.bsn` writing.

The camera frames the scene's bounds on load. The viewport's input orbits (drag), zooms (wheel and pinch) and reframes (`f`), handled in Rust, so no JS runs for it.

`asset_preview_control { preview, play?, animation?, speed? }` plays a glTF animation through an `AnimationPlayer` on the preview root. `asset_preview_close` despawns the scene and frees its layer. Detaching a preview's viewport keeps the scene for 30 seconds, so a pane that closes and reopens does not reload.

Tonemapping, exposure and environment lighting come from the game's main camera when a game runs, so the preview matches the game. In preview mode they are the defaults.

- *Beat: a separate `World` or sub-app per preview.* It would be clean isolation, but assets live in the main world's `Assets` and would load twice. The render-layer and marker isolation that thumbnails and edit mode already use is enough.

### 3. Preview mode: the engine with no game
`bevy-ts preview --assets <dir>` runs the prebuilt engine in viewport mode with no entry module:

- **What it loads:** no TS generation, and the asset root is `<dir>`. Hot reload of assets still works through Bevy's file watcher.
- **What it serves:** the same protocol port as `dev`, answering `hello` with `mode: "preview"` and `viewport: true`. Methods that need a game (`reload`, `tsx_*`, `level_*`, `viewport_mode`, picking, edits) answer method-not-found.
- **Who runs it:** abyme starts it as a process. One per asset root is enough, and it idles at no GPU cost with no viewport attached.

- *Beat: require a running game to preview.* The asset view is most useful before there is a game, when a modeller drops files in.
- *Beat: three.js only, no engine preview.* It is what we have, and it does not show the game's renderer.

### 4. Protocol 1.7.0
New methods `asset_preview`, `asset_preview_control` and `asset_preview_close`. The new fields are `view` and `viewport` on viewport methods and `mode` on `hello`, all additions only. A 1.6 client attaching without `view` gets the game viewport as before.

## Risks / Trade-offs

- [The preview's draw runs inside the game's frame, so a GPU-bound game slows by its cost; three.js competes for the GPU outside that frame] → It renders only while attached and visible, paced by its viewer, at the pane's size. Task 4.2 measures the game's frame time and memory with an engine preview against three.js on the same model before landing, and the operator rules on the numbers.
- [Preview mode costs a whole engine process where three.js reuses the open web view] → It starts only when a preview is asked for with no game running, and task 4.2 measures its idle memory.
- [Ten preview layers] → An eleventh closes the least recently attached; the studio shows one or two at a time.
- [A preview's lights or environment leaking into the game] → Lights render only on the preview's layer (`RenderLayers` on lights); task 2.4 tests that the game's frames are byte-identical with a preview open.
- [Assets loaded for a preview stay in memory while the game also uses them] → They are the same handles, so nothing is loaded twice; closing a preview drops its handles.

## Migration Plan

None. Additions only, and clients without `view` are unchanged. Protocol 1.7.0.

## Open Questions

- Whether a preview of a material (`.wgsl` or a script material) on a probe sphere belongs here or in `script-materials`.
