## Context

`bridge` runs the game in a window of its own and serves the bevy-ts protocol (with `bevy-ts/pause` and `bevy-ts/step`); `tsx` keeps, for every reconciled entity, its key path, source chain and per-prop provenance (`controlled`, `initial`, `bound`, `default`), and serves them as `bevy-ts/tsx_*`. The operator's rulings of 2026-10-19 set the rest:

- The game draws inside abyme's macOS window from the start, in a native hole behind the page, as abyme's browser view does. The engine renders to an offscreen target shared across processes as a GPU surface (IOSurface), abyme's app draws it, and input is forwarded over the protocol. macOS and this Mac first; streamed frames to remote devices later.
- The edit viewport is the engine itself in edit mode: paused simulation, an editor camera, engine-drawn gizmos, picking.
- TSX defines prefabs; levels and per-instance overrides are `.bsn` files the viewport writes (the `levels` change). A literal TSX prop inside a prefab may be rewritten in its TSX; computed and bound props are read-only.
- `bevy-ts dev` keeps a windowed mode outside abyme.

Prior art, read 2026-10-19:

- **abyme's own browser view** (`crates/browser-host/PROTOCOL.md`, `crates/app/src/browser.m`, change `browser-native`). The exact shape this change reuses. A host process checks in one Mach bootstrap service per view; the viewer looks it up and sends `hello` with a reply port; messages are JSON, each with at most one Mach port. The host owns a ring of three IOSurfaces per surface, sends each once (`surface`, from `IOSurfaceCreateMachPort`), then `frame` with a slot; the app sets the IOSurface as its view's `layer.contents`, so the window server composites it at device pixels. Input goes the other way on the same channel. Measured there: a 1.3 ms Metal blit per frame at 2560×1600 (CEF's surface is only valid in its callback, so it copies; Bevy will not need to), key to pixels 6 ms median plus one refresh. Both processes must be in the operator's GUI bootstrap domain.
- **Godot 4.5's embedded game window on macOS** (PR #105884, 2025-05; Game workspace PR #97257, 2024-11). The game makes a `CAMetalLayer`, puts it in a private `CAContext`, and sends the 32-bit context id to the editor over its debugger socket (`game_view:set_context_id`); the editor shows it with a private `CALayerHost`. No IOSurface is exchanged and there is no frame sync between the processes: the game keeps its own vsync on its layer, told which display the editor is on. Input is Godot's `InputEvent`s serialised over the same debugger socket (`embed:event`), coordinates divided by the screen scale; joypads are read in the game. Resize goes editor to game (`embed:window_size`), applied in a `CATransaction` with actions off; the game's own window calls (size, mode, position) error out while embedded. The Game workspace suspends (`SceneTree.set_suspend`), steps (`scene:next_frame`), overrides the camera and picks inside the game process (`runtime_node_select.cpp`), and maps a pick back by the debugger's remote object ids, not scene paths. Taken: input over the protocol, picking and camera override in the game process, joypads read in the game, display id from the viewer. Not taken: the private API (below) and debugger object ids as the mapping (bevy-ts has source chains).
- **wgpu 30** (Bevy 0.20's). No public IOSurface API, and `wgpu::ExternalTexture` is for YUV video only (#7732). The supported path is the hal: make an `MTLTexture` with `newTextureWithDescriptor:iosurface:plane:` on wgpu's own `MTLDevice` (`Device::as_hal::<Metal>`), wrap it with `wgpu_hal::metal::Device::texture_from_raw` (objc2 types since 29; 30 adds a `drop_callback`, wgpu PR #9634, which keeps the IOSurface alive as long as wgpu holds the texture), then `wgpu::Device::create_texture_from_hal::<Metal>`. Read in `wgpu-hal-30.0.1/src/metal/device.rs`. Not yet run.
- **Bevy 0.20.** `RenderTarget::TextureView(ManualTextureViewHandle)` with the `ManualTextureViews` resource renders a camera into an external view (bevy_openxr uses it for XR swapchains). The render world's `ExtractedWindows` only holds windows with a `RawHandleWrapper`, and `prepare_windows` skips any extracted window with no surface data, leaving its `swap_chain_texture_view` as set. `bevy_picking`'s mouse input maps `WindowEvent::CursorMoved` to a pointer on that window and matches cameras by normalized render target. `bevy_gizmos` 0.20 ships `TransformGizmoPlugin` (translate, rotate, scale handles on a `TransformGizmoFocus` entity), driven by `ButtonInput<MouseButton>` and the `PrimaryWindow`'s cursor. Headless apps drop `WinitPlugin` and run `ScheduleRunnerPlugin` or step `SubApps` by hand (`externally_driven_headless_renderer` example). `bevy_remote` accepts requests from any transport through `BrpSender`.
- **macOS layer hosting.** `CAContext`/`CALayerHost` (Godot's, Chromium's, Atlas's) are private: absent from the SDK headers, looked up at run time by Chromium. `CARemoteLayerServer`/`CARemoteLayerClient` (macOS 10.7) are public: the host gives a server port, the client process makes a `CARemoteLayerClient` and sets its layer, and the host shows it with `+[CALayer layerWithRemoteClientId:]`. Little used and unmeasured. Private API matters only for Mac App Store distribution; notarisation does not review API use. It does not matter here, because the primary design needs neither.
- **Cross-process sync.** `IOSurfaceIsInUse` is the one system-wide "in use" test (the window server holds a use count while compositing a surface); per-process use counts do not cross. `MTLSharedEvent` handles can cross processes over XPC, and Chromium and Dawn fence IOSurface access with them, but a `CALayer`'s `contents` cannot wait on an event, so the producer must finish before it hands a slot over.

## Goals / Non-Goals

**Goals:**
- The game renders straight into memory abyme's window composites, with no copy and no encode, at the hole's display rate, and with key-to-pixels no worse than a window of its own.
- Game code does not know whether it is in a window or a hole: a primary `Window`, `ButtonInput`, cursor position and picking behave the same.
- Edit mode is the same engine and the same world, paused, seen from an editor camera, with selection and transform drags that become source edits through the routes the rulings set.
- One schema file covers HTTP methods and the channel's messages.
- The abyme half can be specced from this document alone.

**Non-Goals:**
- abyme's half: the hole, drawing the surface, capturing input, the outliner, inspector and asset browser UI.
- Streamed frames for remote devices (later: the same IOSurfaces fed to VideoToolbox), Windows, Linux, iOS.
- More than one viewport per engine (a Scene view and a Game view side by side), multi-selection drags, snapping, undo inside the engine.
- The `.bsn` format, loader and writer (`levels`), and TSX source rewriting (the editor's, tsx decision 10).
- HDR/EDR output; the ring is 8-bit sRGB.

## Decisions

### 1. The engine owns a ring of three IOSurfaces and renders into them directly
In viewport mode the render world creates three IOSurfaces (`BGRA`, `kIOSurfaceColorSpace` sRGB, rounded-up pixel size, decision 5), wraps each as an `MTLTexture` on wgpu's own device and then as a `wgpu::Texture` (prior art, wgpu 30), and renders the frame's final pass into the current slot. Each surface crosses once as a Mach port. Rendering into the surface itself, not blitting into it, is possible because bevy-ts owns the renderer, unlike CEF.

- *Beat: CAContext + CALayerHost (Godot's way).* Private API, no frame-level control or stats, and the game's `CAMetalLayer` presents on its own schedule. It is fallback 2.
- *Beat: CARemoteLayerClient around the engine's `CAMetalLayer`.* Public and Godot-like, but rarely used, unmeasured, needs a `CAMetalLayer` surface in a process with no window, and gives frames nothing a later streamer can encode. It is fallback 1 if decision 1 fails its spike.
- *Beat: global IOSurface ids (`kIOSurfaceIsGlobal`, Syphon's way).* Deprecated, and any process can read them.
- *Beat: `RenderTarget::Image` and a GPU copy into a shared surface.* An extra copy for nothing; kept only if wrapping a foreign texture fails (decision 2's fallback uses it).
- The same IOSurfaces are what VideoToolbox encodes with no copy, so streaming later adds an encoder, not a second render path.

### 2. A virtual primary window whose swap chain is the ring
The engine spawns a `Window` with `PrimaryWindow` and no `RawHandleWrapper`. A render-world system in `ExtractSchedule`, after `extract_windows`, inserts an `ExtractedWindow` for it with `swap_chain_texture_view` set to the current slot's view and its format; `prepare_windows` skips it (no surface data) and `present` has no `SurfaceTexture` to present. Cameras with `RenderTarget::Window(Primary)` therefore render into the slot with no change to game code; picking's window input, `Window::cursor_position`, `bevy_ui` and the transform gizmo all work against it.

- *Beat: retarget every camera to `RenderTarget::TextureView`.* Game cameras would need rewriting, picking would need pointers whose target is the texture view (its mouse input maps only window events), and `TransformGizmoPlugin` reads the primary window's cursor, so it would need forking. It is the fallback if Bevy's window extraction cannot be fed this way (task 1.3 decides), at the cost of a custom pointer and a forked gizmo.
- *Beat: a `CAMetalLayer` nobody shows, as a real wgpu surface.* Its drawables are recycled IOSurfaces the engine cannot hand out stably.

### 3. Hand-over: completion before announcement, `IsInUse` before reuse
The render app registers `Queue::on_submitted_work_done` after the frame's submit; the callback (on wgpu's poll, in the render thread) sends `frame` for that slot. So the viewer never sets a surface the GPU is still writing, and no fence has to cross processes. Before rendering, the engine picks a slot that is neither the last announced (the viewer may be showing it) nor `IOSurfaceIsInUse` (the window server may be compositing it); with none free it skips the render (stats count it) and the simulation still advances. Three slots cover one being written, one shown and one in the compositor.

- *Beat: `MTLSharedEvent` across processes.* `CALayer.contents` cannot wait on it, so the viewer would have to wait itself before setting contents, which is the same latency as waiting in the engine plus a second channel (XPC) for the handle.
- *Beat: a release message from the viewer per slot.* A round trip per frame, and it still would not know when the window server let go; `IOSurfaceIsInUse` does.
- Cost: the frame is announced when the GPU finishes rather than when it is submitted, which is when it could first be shown anyway.

### 4. The channel is abyme's Mach service shape; the protocol is one schema
`bevy-ts/viewport_attach` (HTTP, from abymed or a test) creates a viewport and checks in `dev.bevy-ts.viewport.<pid>.<n>`; the viewer looks it up, sends `hello` with a reply port, and both sides speak the browser host's framing (a JSON payload plus at most one port). The payloads are JSON-RPC notifications whose methods and params are defined in `protocol/bevy-ts.openrpc.json`: `surface`, `frame` (each with a `layer`, absent meaning 0), `cursor`… from the engine; `input` (one `ViewportInput` event per message), `tick`, `resize`, `visible`, `focus` from the viewer. A thread owned by `bevy-ts-viewport` receives on the port and pushes into a lock-free queue the main world drains in `First`; `bevy-ts/viewport_input` over HTTP goes into the same queue through `BrpSender`.

- *Beat: input as HTTP requests to `bevy_remote`.* Each pointer move would be an HTTP request answered in `RemoteLast`, a frame late; per-event HTTP at 120 Hz is wasteful. HTTP stays for tests and remote clients.
- *Beat: a socket per viewport for input, Mach only for surfaces.* Two channels to keep alive and order against each other, where the browser host already proved one.
- *Beat: XPC.* Needs a launchd-registered service or an anonymous listener handed over by a third party; the bootstrap look-up already works for unrelated processes in one GUI session.

### 5. Resize: rounded-up ring, crop by `contentsRect`
The ring is allocated at the pixel size rounded up to a multiple of 256 per dimension; the virtual window and camera viewports take the exact size, and each `frame` carries the `w`, `h` rendered, which the viewer turns into the layer's `contentsRect`. A pane edge dragged across 1000 px makes at most four rings rather than one per frame. A new ring gets the next `ring` number, its surfaces are sent before its first frame, and the old ring is freed after that frame is announced. Resizes coalesce per frame.

- *Beat: a new ring on every size.* Three IOSurface allocations per frame of a live resize.
- *Beat: render at the old size and let the layer scale.* Blurry for the whole resize and a jump at the end.

### 6. The viewer's vsync drives the frame
With no window and no winit, `bevy-ts-viewport` installs its own runner. While attached and visible it runs `App::update` once per `tick` the viewer sends from the hole's display link (`targetTs` lets `Time` and future frame pacing see the deadline), dropping ticks that arrive mid-update. Before the first tick or after 100 ms without one it paces itself at the attach's `hz`. Hidden in play it updates at 60 Hz without rendering (game time keeps running, as a background tab's page in abyme's browser does); hidden or idle in edit it updates only on a request, a reload or input, as `WinitSettings::desktop_app` does. Bevy's pipelined rendering stays on.

- *Beat: the engine's own display link.* A process with no window has no `NSView` or `NSScreen` link to ask; `CVDisplayLink` is deprecated in macOS 15 and does not know which display the hole moved to.
- *Beat: a fixed timer.* Beats against the display's refresh, so frames judder and latency varies by up to a frame.
- *Beat: Godot's split (each side its own vsync).* There a `CAMetalLayer` presents itself; here the engine has no layer to present to.

### 7. Input becomes winit's messages, with gamepads read locally
Viewport input events are applied before `PreUpdate` as the exact messages `bevy_winit` writes (spec viewport-input), so `bevy_input`, picking, UI and game code take one path. Points become logical pixels; keys use W3C `code` strings mapped to `KeyCode` as winit maps them; IME follows `Ime`. Gamepads are read in the engine by `gilrs` (HID is not per-window), gated by focus. Cursor icon, grab, warp and IME area go back to the viewer, which owns the real cursor.

- *Beat: a bevy-ts input API next to Bevy's.* Every game and plugin would need two code paths.

### 8. Edit mode is a world state, not a second app
`viewport_mode: edit` pauses through bridge's pause (bevy-ts stages and `Time<Virtual>`), flips game cameras' `is_active` off (remembered in a resource), activates an `EditorOnly` camera on layers `0..=30` plus 31 targeting the primary window, adds `TransformGizmoFocus` to the primary selection, swaps the stock mouse pointer for an editor pointer, and turns on the grid and outlines. Play reverses it. `bevy-ts/step` runs frames through the same path. The editor camera is a Rust controller (fly, orbit, dolly, frame), not TS, so it works when TS fails to load.

- *Beat: a second Bevy `App` or sub-app for the editor.* Two worlds to keep in step, which is the problem the rulings removed by making the editor the engine.
- *Beat: the editor camera overriding the game camera's transform (Godot's camera override).* Leaves the game camera moved when play resumes unless restored, and the game's camera components (post-processing, UI target) leak into the editor view. A separate camera starting at its pose gives the same first view.

### 9. Picking selects the prefab instance; selection carries the source
Picking uses `bevy_picking`'s mesh, sprite and UI backends unchanged, with an editor pointer (`PointerId::Custom`) fed from viewport input while game pointer input is off, so game `Pointer<…>` observers and TSX `onClick` handlers stay silent in edit mode. A click resolves to the nearest ancestor that is a prefab instance root (Unity's and Godot's first-click rule), ⌥ to the hit entity. Each selection item carries what tsx serves for the entity (node id, source chain) and, once `levels` exists, its level instance, so the editor maps the pick to source without a second call. Godot maps by debugger object ids and leaves the editor to look the object up; bevy-ts already has source chains.

### 10. Edits are routed by provenance; whoever owns the format writes it
A drag writes the world live (the game is paused, so nothing fights it) and on release emits one `ViewportEdit` with a route:

| Route | When | Who writes | Before `levels` |
|---|---|---|---|
| `bsn` | a level placed the entity | bevy-ts, through `levels`' writer | never produced |
| `tsx` | `TsxNode`, provenance `controlled`, `initial` or `default` | the editor, after it parses and finds a literal (or adds the prop for `default`) | the only persistent route |
| `locked` | provenance `bound` | nobody; the drag never starts | same |
| `transient` | no source describes the entity | nobody; ECS state only | same |

bevy-ts writes `.bsn` because it owns that format's loader and type registry (one implementation of the format, in the engine). The editor writes TSX because only a parser with the file open can tell a literal from a computed expression and keep formatting (tsx decision 10, Triplex). A `rejected` answer reverts the live value and locks the field for the session.

- *Beat: bevy-ts rewrites TSX itself.* It would need a formatting-preserving TS writer in the engine, and the tsx change ruled it the editor's.
- *Beat: the editor writes `.bsn` too.* abyme would need a second BSN serialiser in TS that must track bevy-ts's registry.
- *Beat: decide writability in the engine.* The engine cannot see whether `at={[3, 0, -4]}` or `at={spawnPoint(2)}` reached the field; the editor's parser can.

Consequence worth the operator's eye: for a TSX prefab instance at a scene root, the field the gizmo moves (`Transform.translation` in `enemy.tsx`) is usually a prop passed through (`translation={at}`), which is computed at that element. Whether the editor follows `at` up the chain to the literal at `<Enemy at={[3, 0, -4]} />` is the editor's call; if it does not, placement is persistent only once `levels` makes it a `.bsn` override. Drag-to-place persists reliably only with `levels`.

### 11. Assets are listed from disk and Bevy's asset server, thumbnails rendered offscreen
`bevy-ts/assets` walks the asset root and asks `AssetServer` for the loader by extension and the load state by path, loading nothing. `assets+watch` combines Bevy's `file_watcher` events with `AssetEvent`s. Thumbnails render on a dedicated `RenderTarget::Image` camera on layer 30 with its own light, read back with a `Readback`, encoded to PNG, cached by path and mtime; they never touch the ring.

## Data flow

```
abyme.app (viewer)                          bevy-ts --viewport (engine)
  CADisplayLink ── tick ───────────────────▶ runner: App::update
  NSEvent ── pointer/key/ime/wheel ────────▶ queue ─▶ First: winit-shaped messages
                                             Update … PostUpdate (paused in edit)
                                             render: ExtractedWindow(primary) = slot k
                                             submit ─▶ on_submitted_work_done
  layer.contents = surface[k] ◀── frame{k} ─ (render thread)
  contentsRect = w,h / ring size
  window server composites at vsync
abymed ── HTTP bevy-ts/viewport_attach, _mode, _select, assets … ─▶ bevy_remote
editor ◀─ SSE viewport_selection+watch, viewport_edits+watch ───── bevy_remote
editor ── viewport_edit_result ───────────────────────────────────▶
```

## What abyme must provide

For abyme's own change, in its Bevy app (`crates/app`, Tauri 2) and page; bevy-ts provides everything below the protocol.

1. **The hole.** A layer-backed `NSView` behind the WKWebView, placed by the page as `desktop.browser` places a browser (frame in points, the page's covers over it, held or not), with the web view's hit test giving points in the hole to it. One per game. `contentsGravity` top-left, `contentsScale` the window's backing scale, opaque.
2. **Starting and attaching.** abymed starts `bevy-ts dev --viewport <entry>` as a placed process, calls `bevy-ts/viewport_attach` with the hole's size, scale, display refresh and display id, and gives the app the `service`. The app `bootstrap_look_up`s it and sends `hello` with a reply port; on engine restart (a new pid, a new name) it attaches again.
3. **Drawing.** On `surface`, `IOSurfaceLookupFromMachPort` and keep it by `layer` (absent = 0), `ring` and `slot`; on `frame`, set the view's `layer.contents` to that surface and `contentsRect` to `w/ring_w`, `h/ring_h` inside a `CATransaction` with actions disabled; drop a ring's surfaces when the first frame of a newer ring arrives.
4. **Pacing.** A `CADisplayLink` from the hole's view (`NSView.displayLink(target:selector:)`, macOS 14+) sending `tick` (`ts`, `targetTs`) each vsync while the hole is visible; `visible` on show and hide; `resize` (points and scale) on layout and on backing-scale change; the display's refresh in `resize` when the window moves display.
5. **Input capture.** While the hole has focus, every `NSEvent` the view receives as one `input` message each: pointer moves, buttons, click counts, raw deltas, enter and leave; scroll with precise deltas, phase and momentum; magnify, rotate and smart-magnify; keys with the W3C `code` (winit's macOS scancode table), the logical key, text and repeat; IME through `NSTextInputClient` as `preedit`/`commit`; `focus` on gain and loss. Which keys stay abyme's (its chords, the menu's) follows the browser view's `ownsKeys` rule.
6. **Feedback.** Apply `cursor` (NSCursor), `cursor_grab` (`CGAssociateMouseAndMouseCursorPosition` and hiding for `locked`, confinement for `confined`, and a key that releases it, as Godot's Alt+Esc), `cursor_warp`, `ime_area` (`firstRectForCharacterRange:`), `ime_allowed`.
7. **Edit-mode UI in the page, over the protocol only.** Play, edit, pause and step controls (`viewport_mode`, `pause`, `step`); an outliner from `tsx_tree+watch`; an inspector from `get_components` and `mutate_components`; selection both ways (`viewport_select`, `viewport_selection+watch`); an asset browser from `assets+watch` and `asset_thumbnail`. Gizmos, outlines and the grid are the engine's, drawn in the surface; the page draws nothing over the hole for them.
8. **Source edits.** Consume `viewport_edits+watch`; for a `tsx` route, open the chain's files, find the prop at a location in the chain, rewrite it if it is a literal (or add it for `default`), save, and answer `viewport_edit_result`; keep the undo stack. `bsn` edits need only display.
9. **A proof script** as `scripts/app-browser.js` is for the browser: a measured launch that attaches the cube example, clicks, drags, resizes, toggles edit and checks frames and stats.

## Depends on bridge and tsx

- **D1 (bridge).** `bevy-ts/pause` and `bevy-ts/step` as specced; `bevy-ts dev` able to start with no window (the CLI flag is this change's, the windowless app setup must not assume `WinitPlugin`).
- **D2 (bridge).** Feature-crate protocol methods in the one schema file (B7), including notifications used on the channel.
- **D3 (tsx).** `TsxNode` source chain and per-field provenance readable from Rust by entity, and `tsx_find`'s location lookup callable from Rust, for routes and `viewport_select`.
- **D4 (tsx).** Binding site location and expression text per `bound` field, for the `locked` route.
- **D5 (levels, later).** A level-instance component naming file and instance, and a writer for one field override; until then no `bsn` route.

## Risks / Trade-offs

- [wgpu's hal interop is `unsafe` and moved between 28, 29 and 30] → One module owns it, behind `cfg(target_os = "macos")`; task 1.2 spikes it first. If wrapping fails, render to `RenderTarget::Image` and blit into the IOSurface (the browser host's 1.3 ms at 2560×1600).
- [Feeding `ExtractedWindows` from outside `extract_windows` may break on a Bevy minor] → Task 1.3 spikes it; fallback is decision 2's retargeting with a custom pointer and a forked transform gizmo.
- [`IOSurfaceIsInUse` may stay true longer than a frame for layer contents] → Measured in the spike; if it starves the ring, go to four slots before anything else.
- [Mach bootstrap names need both processes in the operator's GUI domain] → abyme's process host already starts the browser host there; the engine is started the same way. A launch over SSH cannot attach; windowed mode remains.
- [Live-resize allocations] → Rounded-up ring (decision 5).
- [The viewer's tick stops (abyme hangs)] → 100 ms fallback timer.
- [Mouse lock in a hole] → abyme owns the real cursor; a release key is in abyme's list.
- [Private API not used] → Nothing to gate. If decision 1 fails, the fallback order is CARemoteLayerClient (public), then CALayerHost (private: fine for Developer ID builds, not for the Mac App Store).
- [Drag-to-place on TSX instances is usually not a literal at the moved element] → Decision 10: persistent placement needs `levels`; until then the edit is `rejected` or the editor follows props up the chain.
- [Edit-mode pointer swap may miss a plugin that reads `ButtonInput<MouseButton>` directly] → Paused game systems do not run; native plugins that react to raw mouse input in edit mode are listed as a known gap.

## Not possible, and the fallback

- **Rendering into another process's window or view, publicly, with frame control.** Not possible: macOS has no public cross-process `CAMetalLayer`, and `CALayerHost` is private. Fallback: the IOSurface ring (this design), with `CARemoteLayerClient` as the public alternative.
- **Waiting on a GPU fence in `CALayer.contents`.** Not possible. Fallback: announce after completion (decision 3).
- **Sharing surfaces with a sandboxed or other-session process.** Not possible with bootstrap names. Fallback: none needed for abyme (not sandboxed); a remote device gets streamed frames later.
- **Remote devices seeing the IOSurface.** Not possible. Fallback: the later streaming change (VideoToolbox from the same surfaces); until then the phone gets the page's outliner and inspector without the picture, as abyme's browser view does.
- **A virtual window on wgpu's own surface path.** Not possible (no native handle). Fallback: decision 2's injected `ExtractedWindow`.

## Migration Plan

None: a new crate, a CLI flag, protocol 1.1 (additions only). abyme pins major 1 and uses 1.1 methods when `hello` says `viewport: true`.

## Operator rulings

- 2026-10-19: the editor follows a passed-through prop one level up to a literal call-site argument;
  otherwise the edit goes to `.bsn`. Play mode hidden keeps simulating at 60 Hz. Game UI over the
  edit view is off by default, with a toggle (`ui: false`).

- 2026-10-19: the game draws inside abyme's macOS window from the start, in a native hole, as
  abyme's browser view puts a WKWebView behind its page. The engine renders to an offscreen target
  shared across processes as a GPU surface (IOSurface on macOS); abyme's app draws it in the hole;
  input is forwarded over the protocol. macOS, this Mac first; streamed frames for remote devices
  later. The edit viewport is the engine itself in edit mode. Viewport edits write `.bsn` levels and
  per-instance overrides; a literal TSX prop inside a prefab may be rewritten in its TSX; computed
  and bound props are read-only. bevy-ts keeps a windowed mode for `bevy-ts dev` outside abyme.

## Spike results (2026-10-19, Apple M5 Max, macOS 26.4)

- **Decision 1 holds.** Three `BGRA` IOSurfaces wrapped with `newTextureWithDescriptor:iosurface:plane:` on wgpu's own `MTLDevice`, `texture_from_raw` with a `drop_callback` holding the surface, and `create_texture_from_hal` render Bevy's frame with no copy. A Metal texture may be smaller than its IOSurface, so a slot is wrapped at the window's exact pixel size over the rounded-up surface (rewrapped on resize, no new IOSurfaces). This replaces cropping by camera viewport: Bevy 0.20's upscaling pass only scissors to the camera viewport, so a larger target would have stretched the picture.
- **Decision 2 holds.** An exclusive system in `ExtractSchedule`, before `extract_cameras`, inserts the virtual window's `ExtractedWindow` (`Bgra8UnormSrgb`, no `SurfaceData`); `prepare_windows` and `present` skip it and an unmodified `Camera3d` on the primary window renders into the slot. No fallback taken.
- **Decision 3 holds.** `on_submitted_work_done`, with a thread that waits on `Device::poll` so the callback fires at completion rather than the next submit. A slot is excluded while last announced, submitted and unannounced, or `IOSurfaceIsInUse`.
- **60 s at a 120 Hz target** (release, full-screen clear colour changing every frame, the corner pixels of every announced slot read back on `frame`): 4934 updates, 4930 frames, 4 skipped, 0 mixed frames. The loop reached 82 Hz, not 120: the test paces with a sleep after each update; the tick-driven runner (task 3.3) replaces it.
- **Deviation:** the throwaway viewer is windowless and in-process (tests run headless on a shared Mac; nothing may open a window): it speaks the same Mach channel, looks surfaces up from their ports and reads pixels back instead of setting `layer.contents`. Tearing is checked by readback rather than captures, and the window server never holds a slot, so `IOSurfaceIsInUse` starvation is unmeasured. The same-scene comparison against a winit window was not run for the same reason.
- **Key to pixels (task 1.5)**, release, cube scene at 2560×1440, updates paced at 120 Hz as ticks would pace them, keys pressed at spread phases of the frame, measured from the key leaving the viewer to the `frame` announcing the changed picture: median 14.0 ms, p95 37.0 ms, max 59.6 ms over 200 presses; GPU 1.2 ms a frame (`viewport_stats` agreed: median 13.9 ms). The median meets the spec (two refreshes, 16.7 ms); it is about one wait for the next update plus Bevy's pipelined rendering's frame. The tail (a few presses at 3 to 7 frames) is not explained yet; worth a look with the real tick runner and abyme's display link before tuning.

## Implementation notes

- The contract is 1.2.0, not 1.1: tsx took 1.1.0 for its methods first; viewport's are the next additions-only bump. Protocol 1.2 field names are camelCase like the rest of the contract (`targetTs`, `gamepadUnfocused`, `droppedTicks`, `pixelW`). Channel messages are methods with `x-bevy-ts-channel` (`viewer` or `engine`, the sender); the generators list them apart (`CHANNEL` in Rust, `ChannelMessages` in TS), never as HTTP methods. `hello.viewport` is whether `bevy-ts/viewport_attach` is served.
- `bevy-ts dev` (windowed) adds the viewport plugin's edit-mode side only; `viewport_attach`, `_detach`, `_resize` and `_input` exist only under `--viewport`.
- Idle edit mode wakes on any channel message or every 100 ms, which is how protocol requests and reloads are served without a hook into `bevy_remote`'s queue.
- `MeshPickingPlugin` is not in Bevy's `DefaultPlugins`; a game that wants mesh picking adds it, in a window or a hole.

## Edit mode as built

- Picking is a ray cast from the editor camera (`MeshRayCast`, plus light and camera icons as spheres), not an editor pointer in `bevy_picking`: any pointer's events reach game observers, so the stock mouse pointer is turned off (`PointerInputSettings::is_mouse_enabled`) and no picking pointer exists in edit mode at all. Game `PointerClick` observers and TSX `onPointer*` handlers stay silent. UI and sprite hits are not cast yet.
- Bevy 0.20's `TransformGizmoPlugin` draws its handles on its own layer 15 with an overlay camera; the overlay camera is marked editor-only, ordered after the editor camera (-10, -9) and active only in edit mode. Grid, outlines and icons are `EditorGizmos` on layer 31. The editor camera sees layers 0 to 29 and 31, not the thumbnail layer 30.
- `ui: false` keeps game UI off by leaving the default UI camera an inactive game camera; `ui: true` makes the editor camera `IsDefaultUiCamera`.
- A locked field's handles are inert by taking the gizmo's focus away while that field's mode is on (Bevy's gizmo has no per-handle dimming).
- The prefab instance root is the highest ancestor with the hit's innermost call site and chain depth; `viewport_select` by location goes through the served `bevy-ts/tsx_find`. Selection items carry the node id as tsx's 16-digit hex.
- Pointer events' `mods` count as held modifiers for ⌥ and ⇧ clicks, since a viewer may not forward modifier key events.

## Assets as built

- The root is Bevy's file reader root (`BEVY_ASSET_ROOT`, else `CARGO_MANIFEST_DIR`, else the executable's folder, joined with `assets`), overridable with the `AssetRoot` resource. `bevy-ts dev` does not yet point Bevy at the project's `assets/`; that is bridge's to set, and the listing follows whatever Bevy reads.
- `assets+watch` polls the root every 250 ms while someone watches, diffing size, mtime and load state; Bevy's `file_watcher` feature is not on in the engine build, and polling needs no new feature.
- `asset_thumbnail` answers images at once (decoded by Bevy, scaled on the CPU). Meshes and glTF scenes render offscreen on layer 30 over a few frames and read back; until then the answer is `pending: true` and the client asks again (`bevy_remote` methods cannot wait across frames). Fonts get no thumbnail yet (`png: null`). PNGs are written with stored deflate blocks and base64 by hand, so no image or base64 crate was added.

## Demo walk (task 8.1, 2026-10-20)

`cargo test --release -p bevy-ts-cli --test viewport_walk -- --ignored --nocapture` runs `bevy-ts dev <game.tsx> --viewport` as a process of its own; the test is the viewer (the Mach channel across processes, ticks at 120 Hz from a thread as a display link would, IOSurfaces looked up from their ports) and the editor (HTTP, rewriting `game.tsx`). There is no `examples/arena` yet (tsx task 8.1), so the walk writes a two-enemy TSX project (`b` at a literal, `c` at `spawn(2)`). Release build, Apple M5 Max, load average 50–75 from other builds on the machine:

```
1 started with no window: hello says viewport: true (5.8 s)
2 play: 3 surfaces of 1792x1280, 83 to 111 frames in 1 s at 120 Hz ticks, 2 skipped, gpu 0.7 ms
2b play input to frame over 40 events: median 15.0 ms, p95 70.9 ms
3 edit: paused; selected b by game.tsx:8, route tsx
4 clicked b: selected, origin viewport
5 dragged b: x 3 -> 5.474, one edit, route tsx at game.tsx:8
6 editor wrote the literal: reload in 39 to 52 ms, b at x 5.474 (no jump), still selected
7 stepped one frame in edit
8 resized to 900x500 points: frames at 1800x1000 in 112 to 407 ms (a new ring: 2048x1024)
```

The frame rate and the latency tail follow the machine's load; on a quiet machine the spike measured 14.0 ms median key to pixels. The first frame after start takes several seconds in a debug build (pipelines compile); the walk waits for ten frames before measuring.

## What abyme must provide: as built

Checked against abyme's `bevy-studio` change (its design, decision 2), which already speaks this channel. Differences from the list above:

- The contract is protocol 1.5.0 (tsx, systems-api and native took 1.1 to 1.4); abyme checks `hello.viewport`, not a version.
- Multi-word fields are camelCase like the rest of the contract: `tick` is `{ ts, targetTs }`, not `target_ts`. The engine does not read `targetTs` yet, so either spelling works today.
- `visible` and `focus` work as notifications of their own or as `input` events (`{ "type": "focus", "on": … }`); `resize` takes `display` and `hz`.
- `surface` and `frame` carry no `layer` yet (absent means 0). A message the engine does not know (`command`, `menu`) is ignored.
- `asset_thumbnail` answers `pending: true` for meshes and glTF until the offscreen render has been read back; ask again.
- `viewport_select` by `file`, `line` (and `column`) selects every instance root the location makes. Selection items carry tsx's 16-hex node id.
- A game wanting mesh picking in play adds `MeshPickingPlugin`; edit mode's picking needs nothing from the game.

## Open Questions

