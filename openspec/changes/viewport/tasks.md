## 1. Spike: a Bevy frame in another process's layer (go or no-go before group 2)

- [ ] 1.1 Throwaway viewer: a Cocoa app with one layer-backed view that looks up a bootstrap name, sends `hello`, and sets `layer.contents` from `surface`/`frame` messages, reusing abyme's `ipc.h` framing; plus a `CADisplayLink` sending `tick`
- [ ] 1.2 In a scratch Bevy 0.20 app with no `WinitPlugin`: create three IOSurfaces, wrap each with `newTextureWithDescriptor:iosurface:plane:` on wgpu's `MTLDevice`, `texture_from_raw` (with a `drop_callback` that releases the IOSurface) and `create_texture_from_hal`; render a clear colour into one and see it in the viewer
- [ ] 1.3 Feed the slot to a handle-less `PrimaryWindow` by inserting its `ExtractedWindow` after `extract_windows`; render the cube through an unmodified `Camera3d`; record in design.md whether decision 2 holds or the `TextureView` fallback is taken
- [ ] 1.4 Announce from `Queue::on_submitted_work_done`, choose slots by last-announced and `IOSurfaceIsInUse`; run 60 s of a full-screen changing pattern at 120 Hz and record skipped frames and any tearing in captures
- [ ] 1.5 Measure key to pixels (median, p95) and engine CPU and GPU time against the same scene in a winit window; record in design.md. No-go on tearing or a median over two refreshes goes to the operator with fallback 1 (`CARemoteLayerClient`) before group 2

## 2. Crate, CLI and protocol

- [ ] 2.1 Create `bevy-ts-viewport` (macOS only) and `bevy-ts dev --viewport`, which starts with no window, the virtual primary window at 1280×720 at 2x, and the viewport runner; other platforms fail with a message
- [ ] 2.2 Add the 1.1 methods and channel notifications to `protocol/bevy-ts.openrpc.json` with `x-bevy-ts-crate`, regenerate Rust and TS types, bump to 1.1.0, add `viewport` to `bevy-ts/hello`
- [ ] 2.3 Implement the Mach channel: per-attach bootstrap check-in, `hello`/`bye`, dead-port detach, a receive thread pushing into a lock-free queue, and sending with a timeout; `viewport_attach` and `viewport_detach`

## 3. Shared surface

- [ ] 3.1 Move the spike's ring into the crate: allocation rounded up to 256, `surface` messages, slot choice, completion-driven `frame`, skip counting
- [ ] 3.2 Resize: coalesce per frame, crop within the ring, new ring numbering, release of the old ring after the new ring's first frame; `WindowResized` and `WindowScaleFactorChanged` on the virtual window
- [ ] 3.3 Runner: `tick`-driven updates with drop-while-busy, the 100 ms `hz` fallback, hidden play at 60 Hz without rendering, idle edit mode updating only on requests, reloads and input
- [ ] 3.4 `bevy-ts/viewport_stats`: frames, skipped, dropped ticks, GPU time, input-to-frame median and p95
- [ ] 3.5 Tests with the throwaway viewer as a harness: attach, resize storm, scale change, viewer killed mid-frame, re-attach replacing the first viewer

## 4. Input

- [ ] 4.1 Map `ViewportInput` to Bevy 0.20's winit-shaped messages (cursor, buttons, motion, wheel with units and phases, gestures, keys via a W3C `code` to `KeyCode` table from winit's, IME, focus); release keys on focus loss
- [ ] 4.2 `bevy-ts/viewport_input` over HTTP into the same queue through `BrpSender`; test `ButtonInput<KeyCode>` and a `Pointer<Click>` observer from it
- [ ] 4.3 Send `cursor`, `cursor_grab`, `cursor_warp`, `ime_area`, `ime_allowed` on change of the virtual window's fields
- [ ] 4.4 Gate `gilrs` gamepad input by focus unless `gamepad_unfocused`

## 5. Edit mode

- [ ] 5.1 `bevy-ts/viewport_mode`: pause through bridge, remember and deactivate game cameras, `EditorOnly` marker, layer 31, excluded from `entities+watch`, `tsx_tree`, TS queries; `bevy-ts/step` in edit mode
- [ ] 5.2 Editor camera: spawn at the game camera's pose and projection, fly, orbit, dolly, pinch, frame selection; `bevy-ts/viewport_camera`
- [ ] 5.3 Gizmos: grid, selection and hover bounds, light and camera icons, `TransformGizmoPlugin` on the primary selection, `W`/`E`/`R`/`X`/`Escape`; `ui` option for game UI over the editor view
- [ ] 5.4 Picking: editor pointer with game mouse input off, prefab-instance-root resolution from `TsxNode` source chains, ⌥ and ⇧ clicks, `bevy-ts/viewport_pick`; test that TSX `onClick` does not fire in edit mode
- [ ] 5.5 Selection sync: `viewport_select` by entities or by location (through tsx's lookup), `viewport_selection+watch` with origin, node id, source chain and primary flag; selection kept across reloads by node id
- [ ] 5.6 Check edit mode in windowed `bevy-ts dev` with the same tests

## 6. Edits

- [ ] 6.1 Routes per gizmo field from tsx provenance (`tsx`, `locked`, `transient`; `bsn` left as a stub for `levels`), sent with the selection; `locked` handles dimmed and inert
- [ ] 6.2 Live drag writes, `Escape` restore, one `ViewportEdit` on release on `viewport_edits+watch`
- [ ] 6.3 `viewport_edit_result` (`written` keeps; `rejected` restores and locks the field), the 10 s `unconfirmed` state, `viewport_revert`
- [ ] 6.4 Test the round trip with a scripted editor: drag enemy `b`, rewrite the literal in `game.tsx`, answer `written`, check no jump after the reload and `b` still selected; repeat with a computed prop and `rejected`

## 7. Assets

- [ ] 7.1 `bevy-ts/assets`: walk the asset root, kind and loader by extension, load state by path, glTF labelled sub-assets once loaded, loading nothing
- [ ] 7.2 `assets+watch` from `file_watcher` and `AssetEvent`, coalesced per frame
- [ ] 7.3 `asset_thumbnail`: offscreen camera on layer 30 with its own light, `Readback`, PNG, cache by path and mtime; test it leaves the ring and game cameras untouched

## 8. Demo and hand-off

- [ ] 8.1 With the throwaway viewer standing in for abyme: run `examples/arena` with `--viewport`, play, toggle edit, select enemy `b` by click and by location, drag it, answer as an editor, step, resize, and record stats in design.md
- [ ] 8.2 Check design.md's "What abyme must provide" against what the throwaway viewer needed, and send it to abyme's Bevy app change
