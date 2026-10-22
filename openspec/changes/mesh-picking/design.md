## Context

Research: `openspec/research/mesh-picking.md` (f3eba13). In short:

- Bevy adds `DefaultPickingPlugins` (sprite, UI) but not `MeshPickingPlugin`. bevy-ts follows it: "a game that wants mesh picking adds it" (archived `viewport` design). Only a native crate can today.
- The backend is compiled in: `bevy-ts-cli` enables `3d`, which pulls `picking` and `mesh_picking`.
- `bevy-ts.json` already carries one runtime choice, `levels.prefabEdits`, read by `run_dev`. Play reads only `game/game.json`.
- `bevy-ts/viewport_input` is registered by `bevy-ts-viewport`'s `ViewportPlugin`, which is macOS-only and writes window input for a `VirtualWindow`. Headless cameras render into the `Offscreen` image (`headless.rs`), and `bevy_picking` casts only where a pointer's target is the camera's target, so a window pointer never hits there.
- Tests today trigger `PointerClick` by hand; the only real click path is the macOS viewport test.

## Goals / Non-Goals

**Goals:**
- A script-only game turns mesh picking on in one line, in dev and in a built game.
- A headless test, on macOS and Linux, clicks a mesh through real picking.
- A click that cannot fire says why.

**Non-Goals:**
- `MeshPickingSettings` fields (`requireMarkers`, ray-cast visibility). Later, as `meshPicking: { ... }`, with a bool still meaning on/off.
- Keys, wheel, touch or gamepads headless. Only `pointer`.
- A pointer for `bevy-ts play --headless`: play serves no protocol.
- Any protocol change.

## Decisions

1. **A runtime plugin, not a Cargo feature.** `meshPicking` is a field of `ProjectConfig`, next to `levels`. `features` means Bevy Cargo features and forces a project engine; this needs neither. Named after Bevy's plugin. Not `"picking": ["mesh"]`: sprite and UI picking are already on, so `mesh` would be the only member.

2. **`base_app` adds it, from either source.** `base_app` gains one argument, `mesh_picking: bool`, and adds `MeshPickingPlugin` when it is set. `run_dev` reads the project once (it already does for `prefabEdits`) and passes the flag; `play::app` passes `game.json`'s. `skeleton` passes `false`. One place adds the plugin, so dev and play cannot drift.

3. **Baked into `game.json`.** `GameFile` gains `mesh_picking: bool`, serialised as `meshPicking`, `#[serde(default, skip_serializing_if = "is_false")]`: a game built without it writes nothing, and an older `game.json` parses. Not an engine hash input: it is not Rust.

4. **The headless pointer lives in `bevy-ts-viewport`, built everywhere.** A new module `offscreen` (not behind `cfg(target_os = "macos")`) holds `OffscreenPointerPlugin { image: Handle<Image> }`. It registers `bevy-ts/viewport_input` under the crate's own name (`CRATE`), so the contract check still sees one owner, and the method is never registered twice: `ViewportPlugin` is never added headless. `CRATE` moves out of the macOS-only `methods` module so both can use it. `run_dev` adds the plugin under `Display::Headless`, after `BevyTsRemotePlugin`, with `Offscreen`'s handle.
   - Alternative: a method of its own (`bevy-ts/pointer`). Rejected: a second owner of pointer input and a protocol bump.
   - Alternative: reuse `ViewportPlugin` with a virtual window. Rejected: macOS-only, and a window pointer misses an image-target camera.

5. **A custom pointer, as Bevy's `render_ui_to_texture` example.** At startup the plugin spawns `(PointerId::Custom(<fixed uuid>), PointerLocation::default())`. The method validates every event first (pointer only, `viewport` none or 0), then queues them in a resource. A system in `PreUpdate`, before `PickingSystems::Input`, pops one event per frame and writes a `PointerInput` with `Location { target: NormalizedRenderTarget::Image(offscreen, scale 1), position: (x, y) }`:
   - `move`, `enter` → `PointerAction::Move { delta }` from the last position;
   - `down` / `up` → `Press` / `Release` of `Primary`, `Secondary`, `Middle`; `back`, `forward` ignored;
   - `leave` → `PointerAction::Cancel`.
   One per frame because Bevy's hover map comes from the previous frame's hits; a press in the same frame as the move that brought the pointer over the mesh would hit nothing. `bevy_picking` then makes `PointerClick` from press and release, and tsx's observers fire as in a window.

6. **The warning lives in `bevy-ts-cli`.** It needs `MeshPickingSettings`, which only crates with `mesh_picking` see; `bevy-ts-tsx` does not enable it, and adding the feature there for a warning is not worth it. `base_app` adds one `PostUpdate` system, run only while `MeshPickingSettings` is absent, over `(Entity, With<Mesh3d>, With<TsxHandlers>)` filtered on `Added` of either, warning once per process (`Local<bool>`). Checking the resource, not the flag, keeps a native crate that adds the plugin itself free of the warning.
   - Alternative: warn in `picking::set`. Rejected: the handler can be set before `Mesh3d` is inserted, so it would miss cases.

## Risks / Trade-offs

- [The custom pointer on an `Image` target may not reach `PointerClick` through mesh picking] → Task 1 is that test, before anything else. If it fails, build stops and reports.
- [One event per frame slows long input batches] → Only tests use it; a click is three frames.
- [The warning system runs every frame while mesh picking is off] → It reads only rows just added, and stops logging after one warning; cost is negligible next to existing systems.
- [A second `bevy-ts.json` field read at runtime, and the first baked into `game.json`] → It is the pattern script-networking's `net` will also use; both go through `GameFile`.

## Migration Plan

None. Off by default; existing projects and built games are unchanged. A game built before this change plays with mesh picking off, as now.

## Open Questions

None.
