# Mesh picking opt-in and a headless pointer

research@mesh-picking, 2026-10-23. Sources: bevy-ts at 1c44e93, Bevy 0.20 sources in the cargo
registry, agent-town `openspec/research/slice-1-dependencies.md` (1f685b6).

Recommendation: `"meshPicking": true` in `bevy-ts.json` (default `false`, a bool, baked into
`game.json` for play) adds `MeshPickingPlugin`; `dev --headless` serves `bevy-ts/viewport_input`
for `pointer` events as a custom Bevy pointer on the offscreen image, from a cross-platform module of
`bevy-ts-viewport`. No protocol bump, no new dependency, zero default cost.

## 1. A bevy-ts.json switch

Fits Bevy. Bevy keeps `MeshPickingPlugin` out of `DefaultPlugins`
(`bevy_internal-0.20.0/src/default_plugins.rs:103` adds only `DefaultPickingPlugins`), and bevy-ts
already decided "a game that wants mesh picking adds it"
(`openspec/changes/archive/2026-10-20-viewport/design.md:206,252`). Today only a native crate can
(a project-engine build); a script-only game cannot. A config switch closes that.

No new Cargo feature is needed. The engine already compiles the backend: `bevy-ts-cli/Cargo.toml:13`
enables `3d`, which pulls `picking`, which pulls `mesh_picking` (`bevy-0.20.0/Cargo.toml:2725-2731`,
`2954-2959`). So the switch is a runtime plugin choice, not `features` (`project.rs:15`, which means
Bevy Cargo features and forces a project engine, `engine.rs:286-287`).

Existing pattern to follow: `levels.prefabEdits` (`project.rs:77-83`), a runtime choice `run_dev`
reads from `bevy-ts.json` (`cli.rs:282-285`). There is no opt-in-plugin pattern yet; this is the first.

Key `meshPicking`, bool, default `false`. Named after Bevy's plugin. Not `"picking": ["mesh"]`: sprite
and UI picking are already in `DefaultPickingPlugins`, so `mesh` would be the only member.

Play does not read `bevy-ts.json`; it reads `game/game.json` (`play.rs:22-31`, `106-113`). So
`bevy-ts build` writes the flag into `GameFile` (`build.rs:355`), and `base_app` (`cli.rs:197`) adds
the plugin from either source. script-networking does the same for `net` (agent-town note, §1).

Add one guard: when an entity with `Mesh3d` gets an `onPointer*` handler and `MeshPickingSettings`
does not exist, warn once naming `"meshPicking": true`. The place is `bevy-ts-tsx/src/picking.rs`
`set` (line 62). Without it, a click that never fires stays silent, which is how agent-town found it.

Edit mode is safe with it on: entering edit turns off mouse pointer input (`bevy-ts-viewport/src/edit.rs:359-361`),
and edit picking casts its own rays (`edit.rs:479-485`).

## 2. MeshPickingSettings

A bool is enough for now. `require_markers` only helps if scripts can put `MeshPickingCamera` and
`Pickable` on entities; `ray_cast_visibility` defaults to `VisibleInView`, which is what a click wants
(`bevy_picking-0.20.0/src/mesh_picking/mod.rs:40-63`). Both are a reflected resource; expose them later
if a game asks, as `meshPicking: { requireMarkers, visibility }` (a bool still parses as on/off).

## 3. A pointer under --headless

What `viewport_input` needs from `ViewportPlugin` (`bevy-ts-viewport/src/methods.rs:39-48`, `lib.rs:71-107`):

- `SharedRes`, the Mach channel queue it pushes onto; `methods::drain` and `input::apply` in `First`.
- A `VirtualWindow` with `PrimaryWindow`: `input::apply` writes `CursorMoved`/`MouseButtonInput` for
  that window (`input.rs:94-118`).
- The whole crate is `cfg(target_os = "macos")` (`lib.rs:7-40`). Headless also runs on Linux.

Reusing it headless does not work. Bevy's mouse pointer gets a `Window` target, and headless cameras
render into the `Offscreen` image (`bevy-ts-cli/src/headless.rs:56-67`). `RayMap::repopulate` only casts
where the pointer's target is the camera's target (`bevy_picking-0.20.0/src/backend.rs:300-327`). So a
window pointer would never hit.

Smallest way: a custom pointer, as Bevy's own `examples/ui/render_ui_to_texture.rs:20,169-225` does.
Spawn one `(PointerId::Custom(uuid), PointerLocation)`, and turn each `pointer` event into
`PointerInput` messages (`Move`, `Press`, `Release`) whose `Location.target` is
`NormalizedRenderTarget::Image(offscreen)`, before `PickingSystems::Input`. Coordinates: the offscreen
is 960×540 at scale 1 (`headless.rs:13`), so points are pixels. Bevy then makes `PointerClick` from
press and release, and tsx's observers fire as in a window.

Where it lives: a module of `bevy-ts-viewport` built on every platform (say `offscreen.rs`,
`OffscreenPointerPlugin { image }`), added by `base_app` under `Display::Headless` (`cli.rs:217-218`) when
the remote protocol is on. It registers `bevy-ts/viewport_input` with the viewport crate's name,
which the contract check demands (`bevy-ts-remote/src/lib.rs:72-78`). The method keeps one owner.
It handles `pointer` and refuses other event types with a named error.

Where a headless test injects a click today: nowhere through input. Rust tests trigger
`PointerClick` on the entity by hand, skipping picking (`bevy-ts-cli/tests/arena_walk.rs:246-254`,
`tsx_reactive.rs:273-280`). The only real click path is the macOS viewport test, which adds
`MeshPickingPlugin` itself (`bevy-ts-viewport/tests/viewport.rs:174-176`). No HTTP click exists for
`--headless`, and none at all on Linux.

## 4. Protocol

No schema change, so no version bump (stays `1.23.0`, `bevy-ts-remote/src/protocol.rs:5`).
`viewport_input`'s params and result are unchanged. Two spec sentences change, in
`openspec/specs/shared-surface/spec.md`: line 105 and the design note that `_input` exists only under
`--viewport` (`design.md:204`). `hello.viewport` stays `false` headless; it means `viewport_attach`
is served. `meshPicking` is project config, not protocol.

If the team prefers a method of its own (`bevy-ts/pointer`), that is an additions-only bump to
1.24.0 and a second owner of pointer input. I would not.

## 5. Ask-first list

- New dependency: none. The backend is compiled in already (§1).
- Performance: off by default, so the plugin and its `update_hits` system
  (`mesh_picking/mod.rs:68-73`) are absent: zero cost for every existing game. On, it casts one ray per
  pointer per active camera per frame, only while the pointer is over that camera
  (`backend.rs:300-327`); headless with no test pointer casts nothing. The custom pointer system is
  headless-only and idle with no input.
- New architectural pattern: mild. It is the first engine plugin toggled from `bevy-ts.json`, and the
  first `bevy-ts.json` field baked into `game.json`, unless script-networking's `net` lands first.
  Worth one line to the operator if networking has not landed.

## Confidence

Sure, from code: the plugin is absent, the feature is compiled, the viewport input path needs a
macOS window, today's tests trigger clicks by hand. Likely but untested here: the custom pointer on an
`Image` target produces `PointerClick` through mesh picking headless. Bevy's example does exactly this
for UI on a texture; a first test in the change should prove it for a mesh.
