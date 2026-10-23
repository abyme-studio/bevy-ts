## Context

`crates/bevy-ts-cli/src/headless.rs` `screenshot_on_request` spawns `Screenshot::image(offscreen)` (or `primary_window()`) with `save_to_disk` at frames 60 and 75 when `BEVY_TS_SCREENSHOT` is set. Nothing reports whether the file was written; `save_to_disk` only logs.

Facts from Bevy 0.20 that shape this:

- `bevy_remote`: an instant method answers in the frame it is called. A watching method runs every frame until it returns `Some`. The HTTP transport streams only when the method name contains `+watch` (`http.rs`: `let watch = request.method.contains("+watch")`); otherwise it waits for one result on a channel of size 1 and sends it as a plain response.
- `screenshot.rs`: `trigger_screenshots` (Update) inserts `Captured` and triggers `ScreenshotCaptured`; `save_to_disk` writes synchronously inside that observer on native; `clear_screenshots` (First) despawns `Captured` entities. The remote handlers run in `RemoteLast`, after `Last`, so the handler sees `Captured` the frame the file is written.
- `prepare_screenshots` needs a window's `SurfaceData` (a private wgpu surface). The viewport's virtual window has an `ExtractedWindow` whose swap chain is a ring slot, but no `SurfaceData`, so Bevy warns "Unknown window for screenshot" and never captures it.
- `bevy-ts play` serves no protocol (cli.rs adds `BevyTsRemotePlugin` only under `dev`).

## Goals / Non-Goals

**Goals:** one owner for screenshots; a BRP method that answers when the file exists or says why not; headless, window and viewport; a headless test at a chosen frame.

**Non-Goals:** formats other than PNG; returning image bytes over the protocol (`asset_thumbnail` does that for assets); a screenshot API for TS scripts; capturing one camera rather than the run's view.

## Decisions

1. **Owner: `crates/bevy-ts-remote/src/screenshot.rs`.** It holds `ScreenshotTarget` (a resource holding Bevy's `RenderTarget`; absent means the primary window), `capture(commands, path) -> Entity` (spawns `Screenshot(target)` observed by `save_to_disk(path)` and a `Shot { path, asked, since }` marker), the result check, and the method. bevy-ts-cli and bevy-ts-viewport already depend on bevy-ts-remote. Alternative: bevy-ts-cli, where `Offscreen` lives. Rejected: the viewport crate cannot depend on the cli.

2. **Deferred answer on Bevy's watching registration.** `add_deferred_method` registers `RemoteMethodSystemId::Watching` under `bevy-ts/screenshot` and checks the contract with `watch: false`. The handler keys its pending shot by params (as `Subscribers` does): first call validates and calls `capture`; later calls return `None` until `Captured` is on the entity, then `Some(result)` once. Alternatives: polling with `pending: true` like `asset_thumbnail` (the client loops; the lead asked for an answer once written); blocking the instant handler (deadlocks: rendering runs after it).

3. **Written or not.** Before capture: `path` absolute, `.png`, parent a directory; else `INVALID_PARAMS` naming the path. The file's modified time is recorded. When `Captured` appears: the file exists and is newer, answer `{ path, width, height, frame }` (size from the `ScreenshotCaptured` image via a second observer on the same entity, which only records size); else `INTERNAL_ERROR` "Bevy captured but did not write <path>; the engine log has its error". No capture within 5 s wall time: despawn the entity, error "no frame rendered within 5 s". Wall time, not frames, because a viewport with no viewer renders no frames.

4. **Headless names its target.** `HeadlessPlugin` inserts `ScreenshotTarget(RenderTarget::Image(offscreen))`. No other code picks a target.

5. **Viewport: the viewport crate completes the capture.** In its render-world ring code, when a `Screenshot` targets a `VirtualWindow` or `ViewportWindow` and is `Capturing`, it copies that frame's slot texture to a buffer after the frame renders, maps it, and sends `(entity, Image)` to the main world, which inserts `Captured` and triggers Bevy's `ScreenshotCaptured` on the entity. `save_to_disk` then writes it as for any target, and `clear_screenshots` despawns it. Alternatives: patching Bevy so `prepare_screenshots` uses `ExtractedWindow`'s size and format (the right upstream fix, but a fork is a new dependency; file it upstream, drop ours when it lands); re-targeting cameras to an image for one frame (changes what renders). This is a workaround for a Bevy gap, marked so in code as `offscreen.rs` marks its own.

6. **`BEVY_TS_SCREENSHOT` stays, thin.** `screenshot_on_request` calls `capture` once at frame 60. Frame 75 and `<path>.2.png` go: nothing reads the second file, and a later shot is the method's. Removing the variable was rejected: `play --headless` (the `arena-shot` process) has no protocol.

## Risks / Trade-offs

- [Two identical requests in flight share one shot] → both get the same answer; harmless.
- [Viewport readback copies a slot the window server may be showing] → read only the slot of this frame, before it is announced; ring has three slots.
- [Bevy changes `+watch` detection] → a test posts the method over HTTP and asserts one plain response.
- [Protocol version clash with a concurrent change] → the second to land takes the next minor.

## Migration Plan

Scripts that read `<path>.2.png` switch to `bevy-ts/screenshot`. In-repo: `glow-walk.mjs`, `ui-walk`. Rollback: revert the commit; the protocol is additive.

## Open Questions

None.
