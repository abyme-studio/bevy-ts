# Picking

`onPointerClick`, `onPointerPress`, `onPointerRelease`, `onPointerOver` and `onPointerOut` on an
`<entity>` or a UI element fire when Bevy's picking hits that entity. The code is
`crates/bevy-ts-tsx/src/picking.rs`; the switch and the warning are in `crates/bevy-ts-cli`, the
headless pointer in `crates/bevy-ts-viewport/src/offscreen.rs`.

## Bubbling

A picking event bubbles up `ChildOf`, as Bevy's do: a handler runs for a hit on its own entity and
on any descendant. So `onPointerClick` on the entity carrying `WorldAssetRoot` fires for a click on
any mesh of the glTF scene spawned under it, and one on a `<node>` fires for a click on a button
inside it. The event says which:

- `entity`: the entity whose handler runs;
- `target`: the entity the pointer hit, `entity` itself or a descendant.

```tsx
<entity onPointerClick={({ entity, target }) => select(entity)}>
  <WorldAssetRoot value="models/Fox.glb#Scene0" />
</entity>
```

A handler cannot stop the bubbling: handlers run in the frame's one call into JS, after Bevy has
walked the ancestors. A handler that wants only its own hits compares `target` with `entity`.
Nested handlers both run, the inner one first.

Hover bubbles too. `PointerOver` and `PointerOut` reach every ancestor, and Bevy sends Out before
Over. So when the pointer moves from a panel onto a child inside it, the panel's handlers get
`onPointerOut` (`target` the panel) and then `onPointerOver` (`target` the child), in the same
call; moving between two children gives the panel the same pair again. A handler that sets hover
state (`hovered = true` on Over, `false` on Out) ends right. One that counts, plays a sound or
starts a tween fires on every crossing: filter with `target === entity`, or keep the handlers on
the leaves. Bevy's events for "the pointer entered this subtree" are `PointerEnter` and
`PointerLeave`, which skip ancestors the previous hover shared; bevy-ts has no `onPointerEnter` or
`onPointerLeave` yet.

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
`bevy-ts/log+watch`. No warning when mesh picking is on, however it was added. A handler on a
parent of the meshes (a glTF scene's root, say) is not warned about: only an entity with both.

## Clicking headless

Under `bevy-ts dev --headless`, on every platform, `bevy-ts/viewport_input` takes `pointer` events.
They drive one pointer over the offscreen image the cameras render into (960×540), with `x`, `y`
in its pixels. `move` and `enter` move it, `down` and `up` press and release `left`, `right` or
`middle` (`back` and `forward` are ignored), and `leave` cancels it and takes it off the image.
A `down` or `up` away from the pointer moves it there first, a frame before the press.

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

Headless has a primary window with no native window behind it, the image's size: Bevy 0.20's
picking casts no ray without a `PrimaryWindow`, even for a pointer on an image. Nothing renders
for it. It goes once Bevy fixes `Location::is_in_viewport`.
