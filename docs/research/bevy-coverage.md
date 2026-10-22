# Bevy coverage: what a TS script reaches today

Audit of Bevy 0.20's core crates against bevy-ts on main (`542a005`), 2026-10-23, for the
operator's ask: nothing in core Bevy without an idiomatic TS translation that is not a
hand-written hack. Research by research@bevy-coverage-audit, for lead@bevy-ts.

Status words:

- **idiomatic**: a Bevy-shaped TS API (types, params, TSX) does it.
- **partial**: idiomatic for part; the missing part is named.
- **hack**: reached only through `spawnNative` recipes, `custom()` ops or JSON blobs.
- **missing**: no way from TS without writing Rust.

Owner says who closes the gap: an in-flight change (its team), or a new change proposed at the
end (`new: <name>`). Tests are relative to `crates/`. "no test" means the claim rests on code
reading.

The one-line answer: data is nearly covered once the in-flight changes land (reflect-coverage
does most of it); behaviour is not. Schedules, conditions, observers on Rust events, Rust
messages, input devices, time control, assets, gizmos, audio, diagnostics and 2D have no TS
path, and a writable exported param, which most of them need, is refused by design today.

## ECS

| Capability | Today | Evidence / test | Owner |
|---|---|---|---|
| TS components (layout columns) | idiomatic | `bevy-ts-cli/tests/systems.rs` `ts_writes_reach_rust_and_only_written_rows_are_changed` | — |
| Rust layout-tier components as query data | idiomatic | `bevy-ts-cli/tests/native_components.rs` `a_ts_write_is_seen_by_rust_the_same_frame…` | — |
| Layout-tier holes (`Color`, enums with data, `Option`) | partial: TSX writes after reflect-enums-color; systems never see them | `bevy.d.ts` "Holes" comments | reflect-enums-color (TSX), reflect-coverage stage 3 (systems) |
| Reflect-tier components in TSX, `With`/`Without` | idiomatic | `native_components.rs` `a_reflect_tier_component_is_a_tsx_element`; filters: no test | — |
| Reflect-tier components from `Commands` (`Name`, `Camera3d`, `Disabled`) | missing: `idOf` throws, `runtime/core.js:2041` | no test | reflect-coverage stage 2 |
| Reflect-tier components as query data, `GlobalTransform` | missing (`NotReflect` in `index.d.ts:220`) | `systems.rs` `load_errors_name_what_is_wrong` (refusal) | reflect-coverage stage 3 |
| `bevy.d.ts` reflect-tier types carry fields | missing: bare `Reflected`, so `<Camera3d …>` props do not type-check | `bevy.d.ts:7` | reflect-coverage stage 1 (generated types) |
| TS resources (`Res`, `ResMut`) | idiomatic | `surface.rs` `a_score_resource_is_written_in_place_and_kept_across_reloads` | — |
| Rust resources other than `Time` | missing: `Res` resolves `Time` or TS names only (`bevy-ts/src/systems.rs:447`); `native.d.ts` types them, so they fail at load | no test | new: `rust-resources` (with reflect-coverage stage 3) |
| Filters `With`, `Without`, `Changed`, `Added` | idiomatic | `systems.rs` `changed_and_added_filters_visit_only_matching_rows` | — |
| Filters `Or`, `Spawned`, `Has`, `AnyOf`, `Ref` | missing | `bevy-ts-cli/src/reader.rs:1147` | new: `ecs-queries` |
| `Option`, `Entity`, `get`, `single`, `each`, iteration | idiomatic | `systems.rs` `option_entity_single_get_and_iteration` | — |
| `iter_many`, `Single`, `Populated`, `RemovedComponents`, `SystemChangeTick` | missing | `reader.rs:1116` | new: `ecs-queries` |
| `ParamSet` | not needed: overlapping queries share one column | `overlapping_queries_alias_one_column` | — |
| Hierarchy: `setParent`, `ChildOf` column | idiomatic | `systems.rs` `commands_spawn_parent_overflow_and_discard` | — |
| Hierarchy: walk down (`Children`), sibling order | missing: `Children` is reflect tier | — | reflect-coverage stage 3 (read), new: `ecs-queries` (helpers) |
| Custom relationships | missing | — | new: `ecs-queries` |
| Commands `spawn`, `insert`, `remove`, `despawn`, `trigger`, `write` | idiomatic | `systems.rs` as above; `surface.rs` trigger and message tests; `remove`: no test | — |
| `spawn(<element/>)` returns no entity; `entity(e).insert(<X/>)` mounts a child, not on `e` | partial | `core.js:1893`, `core.js:1801` | new: `ecs-queries` |
| `spawnBatch`, `insertIfNew`, `retain`, `clear`, `cloneAndSpawn` | missing | — | new: `ecs-queries` |
| 256 spawns per system run | partial (a limit Bevy does not have) | `bevy-ts/src/commands.rs:42` | new: `ecs-queries` (lift or document) |
| `spawnNative` recipes `camera3d`, `directionalLight`, `cuboid` | hack | `bevy-ts/src/recipes.rs:79-109`; examples `cube`, `arena` use `camera3d` | reflect-coverage stage 2 removes them |
| `custom(kind, payload)` | hack, kept for plugin opcodes | `bevy-ts/tests/plugin_api.rs` | — (a plugin seam, not a gameplay path) |
| TS messages | idiomatic (TS queues, not Bevy `Messages<T>`) | `surface.rs` `two_readers_see_the_same_messages_once_and_they_expire` | — |
| Reading Rust messages (`KeyboardInput`, `AssetEvent`, `WindowResized`, `CollisionEvent`) | missing; ports forward by hand (`plugin_api::write_message`) | `bevy-ts/src/systems.rs:470` | new: `rust-events` |
| Writing Rust messages (`GamepadRumbleRequest`, `RequestRedraw`) | missing | — | new: `rust-events` |
| Observers: TS `Event`, `EntityEvent` | idiomatic; bubbling itself untested | `surface.rs` `an_observer_triggered_from_ts_runs_before_the_next_system` | new: `ecs-queries` (bubble test) |
| Lifecycle `Add`, `Insert`, `Remove` | idiomatic, deferred to the next stage boundary | `surface.rs` `a_lifecycle_observer_on_a_rust_insert_runs_at_the_next_stage`; Insert/Remove: no test | — |
| Lifecycle `Discard`, `Despawn` | missing | `bevy-ts/src/observers.rs:56` | new: `ecs-queries` |
| Entity-scoped observers from code (`entity(e).observe`) | missing; TSX `onPointer*` only | `core.js:760` | new: `rust-events` |
| Observers on Rust events (`Pointer*`, `WorldInstanceReady`, `FocusGained`) | partial: picking via TSX props only | `tsx_reactive.rs` `three_clicks_in_one_frame…` | bevy-ui-coverage (UI picking and focus props); new: `rust-events` (from code, any Rust event) |
| Component hooks | missing; observers cover the TS need | — | new: `ecs-queries` (decide: hooks or immediate observers) |
| Required components on TS components | missing | — | new: `ecs-queries` |
| States, `NextState`, `OnEnter`/`OnExit`, `inState` | idiomatic (TS state machines) | `surface.rs` `entering_playing_runs_exit_then_enter_and_unlocks_systems` | — |
| `SubStates`, `ComputedStates`, `PreviousState`, `OnTransition`, `state_changed` | missing | — | new: `ecs-schedules` |
| State-scoped entities (`DespawnOnExit`, `DisableOnExit`…) | missing: reflect tier, and TS states are not Bevy states | — | new: `ecs-schedules` |
| Schedules `Startup`, `PreUpdate`, `Update`, `FixedUpdate`, `PostUpdate` | idiomatic | `surface.rs` `startup_runs_once_per_identity`; `systems.rs` `chain_after_before_sets_and_stages_order_systems` | — |
| Schedules `First`, `Last`, `PreStartup`, `PostStartup`, `FixedFirst`/`PreUpdate`/`PostUpdate`/`Last`, custom | missing (reachable only through a slot around a Rust set) | `bevy-ts/tests/slots.rs` | new: `ecs-schedules` |
| Sets and ordering, Rust anchors | idiomatic | `systems.rs` as above | — |
| Conditions `inState`, `resourceExists`, `onMessage`, `not` | idiomatic, evaluated in Rust | `surface.rs` `resource_exists_and_not_conditions`, `a_skipped_system_costs_no_js_call` | — |
| Conditions `or`, `and` (explicit), `runOnce`, `resourceChanged`/`Added`/`Removed`/`Equals`, `anyWithComponent`, `anyComponentRemoved`, `conditionChanged`, a TS predicate | missing (several `runIf` AND together) | `core.js:1105` | new: `ecs-schedules` |
| One-shot systems (`registerSystem`, `runSystem`) | missing | — | new: `ecs-schedules` |
| Exclusive world access | missing; not wanted (JS stays batched) | `index.d.ts:426` | — (decline) |
| `Disabled` entities | partial: filter only; whether TS queries honour `DefaultQueryFilters` is untested | — | reflect-coverage stage 2 (insert); new: `ecs-queries` (test) |

## Transform, time, input, window

| Capability | Today | Evidence / test | Owner |
|---|---|---|---|
| `Transform` columns, `Vec3`/`Quat` helpers | idiomatic | `systems.rs` `a_vector_method_stores_the_tick_once` | — |
| `Transform.lookingAt` | missing | — | reflect-coverage stage 2 |
| `GlobalTransform` read; `TransformHelper` | missing; TS `PostUpdate` also runs before propagation | `bevy-ts/src/lib.rs:153` | reflect-coverage stage 3 (read); new: `ecs-schedules` (a slot after propagate) |
| `Res<Time>` | idiomatic, read-only, a built value not a column (docs say column) | `surface.rs` `time_is_a_read_only_view`; fixed step: no test | new: `script-time` |
| `Time<Real>`, `Time<Virtual>` pause and speed, `Time<Fixed>` timestep | missing | — | new: `script-time` |
| `Timer`, `Stopwatch`, `on_timer`, delayed commands | missing (hand-rolled `Local`) | — | new: `script-time` |
| Keyboard (`ButtonInput<KeyCode>`), `InputFocus`, `KeyCode` | missing on main | — | text-input |
| Mouse buttons, motion, scroll, cursor position | missing | — | new: `script-input-devices` |
| Touch, gestures | missing | — | new: `script-input-devices` |
| Gamepads (buttons, axes, sticks, rumble) | missing | — | new: `script-input-devices` |
| `input_just_pressed` conditions | missing | — | new: `script-input-devices` |
| Actions and bindings | not core Bevy (0.20 has none); a leafwing port is planned | `docs/research/native-ports.md` | — (port, not core) |
| Tab and directional navigation, focus events | missing | — | bevy-ui-coverage stage 4 |
| `Window`, `CursorOptions` | partial: flags and enums are columns; size, title, position are holes | `bevy.d.ts` | reflect-coverage |
| Window messages (`WindowResized`, `CursorMoved`, `FileDragAndDrop`) | missing | — | new: `rust-events` |
| `CursorIcon` | missing (reflect, custom cursor needs an image handle) | — | reflect-coverage |

## Assets and scenes

| Capability | Today | Evidence / test | Owner |
|---|---|---|---|
| Handles as asset paths in TSX and levels | idiomatic | `bevy-ts-tsx/src/reflected.rs:19` | — |
| Hot reload of files | idiomatic (dev only, by design) | levels: `bevy-ts-levels/tests/levels.rs` `an_included_file_reloads_every_includer_copy`; textures and glTF: no test | — |
| `AssetServer.load`, load state, `isLoadedWithDependencies` in systems | missing | — | new: `script-assets` |
| `AssetEvent`, `AssetLoadFailedEvent` | missing | — | new: `rust-events` |
| `Assets<T>` add and mutate (meshes, images, materials, atlas layouts) from TS | missing | — | new: `script-assets` |
| Processing, meta files, custom loaders | Rust-side by nature | — | — (stays Rust) |
| `.bsn` levels, TSX scenes, `SceneComponent` prefabs | idiomatic (bevy-ts's own `.bsn` loader; Bevy 0.20 has none) | `bevy-ts-levels/tests/levels.rs` | — |
| glTF scenes (`WorldAssetRoot`) | partial: TSX cannot place a newtype component today | — | script-gltf |
| `DynamicWorld` save and load (save games) | missing | — | new: `world-saves` |

## Render, PBR, core pipeline

| Capability | Today | Evidence / test | Owner |
|---|---|---|---|
| Cameras: `Camera`, `Camera3d`, `Hdr`, `RenderTarget` | partial: `camera3d` recipe (hack) or TSX `<Camera3d/>` (untested); no look-at | `examples/cube/src/game.ts:12` | reflect-coverage (recipes go, `.of`), game-camera-edit (editor side) |
| `Projection`, `ScalingMode`, `Viewport` | missing → TSX after reflect-enums-color | — | reflect-enums-color |
| World to viewport | missing on main | — | world-to-screen |
| Viewport to world (cursor ray), `MeshRayCast` | missing | — | new: `camera-rays` |
| Lights (`Directional`, `Point`, `Spot`, `Rect`) | idiomatic for numbers; `color` a hole | `examples/arena/src/game.tsx:49` | reflect-enums-color (colour) |
| `AmbientLight` | partial | — | ambient-light-script |
| Environment maps, skybox, light probes, fog volumes | partial: numbers are columns, image handles via the path rule | — | reflect-coverage |
| `<material>` `StandardMaterial`: color, metallic, roughness, emissive, unlit | idiomatic | `bevy-ts-tsx/src/intrinsics.rs` | — |
| `alpha`, `alphaMode` | missing on main | — | material-alpha |
| The other `StandardMaterial` fields (textures, normal map, double-sided, cull, transmission, clearcoat, uv transform, depth bias) | missing | — | new: `standard-material-fields` |
| Script materials (`.wgsl` + TS params) | idiomatic | `docs/materials.md` | — |
| Script material on a glTF scene | missing on main | — | scene-material |
| Post-process settings (`Bloom`, `Tonemapping`, `DepthOfField`, AA…) | idiomatic for numbers; colours and nested structs are holes | `bevy.d.ts` | reflect-enums-color |
| `RenderLayers`, `Visibility`, `NoFrustumCulling`, shadows markers | partial: `Visibility` a column (no test); `RenderLayers` reflect tier | — | reflect-coverage |
| `<mesh>` primitives | partial: cuboid, sphere, plane | `intrinsics.rs:27` | mesh-primitives |
| Custom meshes from TS (`Mesh::new`, attributes, indices) | missing | — | new: `script-assets` |
| Wireframes | partial: markers via TSX | — | reflect-coverage |

## 2D, text, UI, picking

| Capability | Today | Evidence / test | Owner |
|---|---|---|---|
| `Camera2d`, `Sprite`, `Anchor` | partial: `Sprite.image` is a handle hole; no 2D camera recipe | `bevy.d.ts` | reflect-coverage (handles, colour) |
| `Mesh2d` + `ColorMaterial`, 2D primitives | missing (mesh-primitives scopes out `Mesh2d` and 2D cameras) | — | new: `script-2d` |
| Texture atlases (`TextureAtlasLayout::from_grid`), sprite sheet animation | missing | — | new: `script-2d` (with `script-assets`) |
| `Text2d`, `TilemapChunk` | missing / reflect only | — | new: `script-2d` |
| UI nodes, style, text, images, buttons | partial (see bevy-ui-coverage's list) | `bevy-ts-tsx/tests` | bevy-ui-coverage |
| `<input>` | missing on main | — | text-input |
| UI widgets (checkbox, slider, menus, dialogs) | missing | — | bevy-ui-coverage stages 5–6 |
| UI materials (`MaterialNode`) | missing | — | bevy-ui-coverage stage 8 (ask-first, not asked) |
| Picking events on entities | partial: five `onPointer*` props, no bubbling | `tsx_reactive.rs` | bevy-ui-coverage stage 3 (all events), mesh-picking (3D backend) |
| `Pickable`, `PickingInteraction`, `Hovered` | partial: columns | — | — |

## Gizmos, animation, audio, glTF, diagnostics, remote, reflect

| Capability | Today | Evidence / test | Owner |
|---|---|---|---|
| Immediate `Gizmos` (lines, shapes, text) | missing | — | new: `script-gizmos` |
| Gizmo config, retained `GizmoAsset`, `Show*Gizmo` markers | partial: markers via TSX | — | new: `script-gizmos` |
| Play a glTF clip with transitions | missing on main | — | script-gltf (`PlayAnimation`) |
| `AnimationPlayer` control from systems (seek, pause, speed, weights, `is_finished`), graphs, animation events, TS-built clips | missing; script-gltf scopes it out | — | new: `animation-control` |
| `AudioPlayer`, `PlaybackSettings`, `SpatialListener` | partial: `PlaybackSettings` is a column; `AudioPlayer` absent from the types; no decoder crate in `Cargo.lock` (no `lewton`), so likely nothing plays | `bevy.d.ts`; `Cargo.lock` | new: `script-audio` |
| `AudioSink` control, `GlobalVolume` | missing | — | new: `script-audio` |
| glTF asset (`Gltf`: named scenes, nodes, animations), extras | partial: extras are reflect components | — | script-gltf; new: `script-assets` (the `Gltf` asset) |
| `DiagnosticsStore` (FPS, frame time, entity count), `FrameCount` | missing | — | new: `script-diagnostics` |
| BRP (studio, protocol) | idiomatic for tools; scripts need none | `bevy-ts-remote` | — |
| Custom BRP methods from TS | missing; low demand | — | — (decline for now) |
| Reflect shapes (enums, colour, lists, maps, `Duration`), hole CI walker | partial | — | reflect-enums-color, reflect-coverage |

## Docs and specs that claim more than the code

- `openspec/specs/ts-systems/spec.md:134` says a Rust resource with a native layout is reachable
  as `Res<Time>` is. Only `Time` is special-cased. `rust-resources` owns the fix.
- `docs/native.md:89` and the same spec call `Time` a column. It is a value built per run.
  `script-time` owns the fix.
- `openspec/specs/native-components/spec.md:41-50` says shipped types carry JSON shapes;
  `bevy.d.ts` declares reflect-tier types bare. reflect-coverage stage 1 owns the fix.
- No doc says `Commands` cannot insert a reflect-tier component; it is a runtime error. Closed
  by reflect-coverage stage 2.

## The enabler most new changes need

Exported params (`#[bevy_ts_native::export]`) are read-only by rule: "A param that writes
(`ResMut`, `Query<&mut T>`) does not compile" (`docs/native.md`). That rule is why gizmos,
audio sinks, time control, asset loading, animation control and rumble have no TS path. Bevy's
own unit for each is a writing param (`Gizmos`, `ResMut<Time<Virtual>>`, `AssetServer` +
`ResMut<Assets<T>>`, `Query<&mut AnimationPlayer>`).

Recommendation: a `writable-native-params` change first. An exported param may write; its calls
run inside the TS system's run, which is already exclusive on the main thread, so a written
column is marked changed and the borrow ends with the call. It is an extension of an existing
pattern, but it changes how Rust and JS share the world, so it is on the operator's ask-first
list (architecture, performance). Every new change below except `ecs-*`, `rust-events`,
`script-2d` and `world-saves` builds on it.

## New changes, one per uncovered category

Recommended order: the enabler, then ECS (everything else uses it), then the rest by demand.

1. `writable-native-params`: exported params may take `ResMut`, `Query<&mut T>` and `Commands`;
   change ticks and borrow rules; a bench against today's read-only call.
2. `ecs-schedules`: every main and fixed schedule; `PreStartup`/`PostStartup`; a slot after
   transform propagation; conditions `or`/`and`/`runOnce`/`resource*`/`anyWithComponent`/
   `conditionChanged` evaluated in Rust, plus a TS predicate; one-shot systems; `SubStates`,
   `ComputedStates`, `PreviousState`, `OnTransition`; TS states as real Bevy states so
   `DespawnOnExit` and friends work.
3. `ecs-queries`: `Or`, `Has`, `Spawned`, `AnyOf`, `Ref` filters and data; `Single`,
   `Populated`, `iterMany`, `RemovedComponents`; `Discard`/`Despawn` lifecycle; required
   components and custom relationships on TS components; `Children` helpers; `spawnBatch` and
   the other `EntityCommands`; `spawn(<element/>)` returns its entity; the 256-spawn limit; tests
   for bubbling, `Disabled` and despawn recursion.
4. `rust-events`: `MessageReader<M>`/`MessageWriter<M>` of any reflected Rust message (input,
   window, asset, state transition, port messages without hand forwarding); `addObserver` and
   `entity(e).observe` on any reflected Rust `Event`/`EntityEvent`, with bubbling.
5. `rust-resources`: `Res`/`ResMut` of every Rust resource: layout tier zero-copy, reflect tier
   through reflect-coverage's reader and writer; the spec fix. Settle with reflect-coverage
   whether its stage 3 already takes this (it names "`Res` data" for reflect tier only).
6. `script-time`: `Time<Real>`, `Time<Virtual>` (pause, speed), `Time<Fixed>` (timestep,
   overstep); `Timer` and `Stopwatch` as TS values with Bevy's methods; `onTimer` conditions;
   delayed commands.
7. `script-input-devices`: extends text-input's `bevy-ts/input` with `Mouse`, `MouseMotion`,
   `MouseScroll`, cursor position, `Touches`, gestures, `Gamepads` (buttons, axes, sticks,
   rumble), and `inputJustPressed` conditions.
8. `script-assets`: an `Assets` param: `load` (typed handles), load state, `add` for meshes,
   images, materials and atlas layouts built in TS, the `Gltf` asset's named parts.
9. `script-gizmos`: the `Gizmos` param with Bevy's methods, batched per frame; config groups;
   retained `GizmoAsset`.
10. `script-audio`: the decoder features, `<AudioPlayer>`, `AudioSink` control from systems,
    spatial audio, `GlobalVolume`.
11. `script-diagnostics`: `DiagnosticsStore` reads (FPS, frame time, entity count) and
    `FrameCount`.
12. `script-2d`: `Camera2d` example, `Mesh2d` + `ColorMaterial` (`<mesh>` 2D shapes,
    `<material2d>` or `<material>` on a 2D mesh), sprite sheets, `Text2d`, tilemap chunks.
13. `standard-material-fields`: every `StandardMaterial` field on `<material>`, textures as
    asset paths. A follow-up for the material-alpha team.
14. `animation-control`: `AnimationPlayer` and graph control from systems, animation events,
    TS-built clips. A follow-up for the script-gltf team.
15. `camera-rays`: `viewportToWorld` and `MeshRayCast` on world-to-screen's `Cameras` param. A
    follow-up for the world-to-screen team.
16. `world-saves`: `DynamicWorld` save and load of TS and Rust components. Lowest demand.

Declined: exclusive world access from TS (it breaks the batched bridge), custom BRP methods from
TS, Bevy action bindings (not core; the leafwing port plan covers them), asset processing and
custom loaders (Rust by nature).

## Sources

- bevy-ts `main` at `542a005`: `packages/bevy-ts/*.d.ts`, `runtime/core.js`,
  `crates/bevy-ts/src`, `crates/bevy-ts-cli` tests, `docs/native.md`, `openspec/specs`.
- In-flight proposals in their worktrees: bevy-ui-coverage, text-input, reflect-enums-color,
  reflect-coverage, mesh-primitives, mesh-picking, script-gltf (scene-animation),
  scene-material, material-alpha, world-to-screen, game-camera-edit, script-networking.
- Bevy 0.20.0 sources in the cargo registry: `bevy_ecs`, `bevy_state`, `bevy_app`,
  `bevy_time`, `bevy_input`, `bevy_asset`, `bevy_scene`, `bevy_world_serialization`,
  `bevy_camera`, `bevy_pbr`, `bevy_sprite`, `bevy_gizmos`, `bevy_animation`, `bevy_audio`,
  `bevy_diagnostic`, `bevy_picking`, `bevy_mesh`, `bevy_image`; and its `Cargo.toml` features
  against bevy-ts's (`3d`, `bevy_ui_render`, `bevy_remote`, `file_watcher`; no `audio`).

Confidence: high for "missing" rows (each checked against the code); medium for "idiomatic"
rows without a test. Not checked: whether TS queries skip `Disabled` entities, whether Rust
required components come with a TS insert, whether `Changed`/`Added` work on sparse-set
components (the rows path reads table ticks only, `bevy-ts/src/systems.rs:697`), and whether
`<Camera3d/>` in TSX renders with no recipe.
