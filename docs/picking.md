# Picking

`onPointerClick`, `onPointerPress`, `onPointerRelease`, `onPointerOver` and `onPointerOut` on an
`<entity>` or a UI element fire when Bevy's picking hits that entity. The code is
`crates/bevy-ts-tsx/src/picking.rs`; the switch and the warning are in `crates/bevy-ts-cli`, the
headless pointer in `crates/bevy-ts-viewport/src/offscreen.rs`.

## What picks

- **UI and sprites** pick out of the box: Bevy's default plugins add both backends.
- **3D meshes** pick only with Bevy's `MeshPickingPlugin`, which Bevy leaves out of its defaults.
  Turn it on in `bevy-ts.json`:

  ```json
  { "meshPicking": true }
  ```

  It is a bool, default `false`. `bevy-ts dev` adds the plugin with its default settings, under a
  window, `--viewport` and `--headless` alike, with the prebuilt engine: no project engine, no
  `cargo`. `bevy-ts build` writes it into `game/game.json`, and the built game plays with it. A
  `game.json` without it plays with mesh picking off. A native crate may add `MeshPickingPlugin`
  itself, with settings of its own; then the switch adds nothing.

## When a click cannot fire

With mesh picking off, the first entity that has both a `Mesh3d` and a pointer handler logs one
warning per process, naming the entity and `"meshPicking": true`. It shows in the terminal and in
`bevy-ts/log+watch`. No warning when mesh picking is on, however it was added.

## Clicking headless

Under `bevy-ts dev --headless`, on every platform, `bevy-ts/viewport_input` takes `pointer` events.
They drive one pointer over the offscreen image the cameras render into (960×540), with `x`, `y`
in its pixels. `move` and `enter` move it, `down` and `up` press and release `left`, `right` or
`middle` (`back` and `forward` are ignored), and `leave` cancels it.

```json
{ "jsonrpc": "2.0", "id": 1, "method": "bevy-ts/viewport_input", "params": { "events": [
  { "type": "pointer", "kind": "move", "x": 480, "y": 270 },
  { "type": "pointer", "kind": "down", "x": 480, "y": 270, "button": "left" },
  { "type": "pointer", "kind": "up", "x": 480, "y": 270, "button": "left" }
] } }
```

It answers `{ "queued": 3 }`. The engine applies one event a frame, so the move's hover is in place
before the press: a click takes three frames. Any other event type refuses the whole request with
an error naming the type, and nothing of it applies; so does a `viewport` other than 0. The other
viewport methods are not served headless, and `bevy-ts/hello` still reports `viewport: false`.
`crates/bevy-ts-cli/tests/headless_pointer.rs` clicks a cube this way.
