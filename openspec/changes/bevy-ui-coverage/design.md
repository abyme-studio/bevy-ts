## Context

Research (`docs/research/bevy-ui-coverage.md`, faa5aff) read Bevy 0.20's UI crates and bevy-ts at 1c44e93. Facts this design rests on, rechecked in the source:

- `ui.rs` maps each prop to one component (`Part`). `background` writes `BackgroundColor` and removes it when the prop goes; `border` is `BorderColor::all`; `radius` is `BorderRadius::all` on `Node`.
- Picking props are entity-local observers that queue `(handler id, json)` on `TsxTree.reactive.events` and skip events whose `original_event_target()` is not the entity (`picking.rs`). text-input renames it `events.rs` and adds keyboard and text events there.
- The per-frame check (`host.rs` `frame_inner`) returns at once when there are no subscriptions, events or sites; otherwise it reads each subscribed pair's change tick with `get_change_ticks_by_id`. A pair whose component is absent counts as moved, is delivered as `null` and is dropped. Quiet frame with one HUD subscription: 0.8 µs (`docs/research/tsx-scale.md`).
- `component()` in `host.rs` resolves only components with a native layout. Markers such as `Pressed` are reflect tier in `bevy.d.ts`.
- A one-field tuple struct's layout field is named `"0"` (`layout_reflect.rs` `fields`); a fieldless enum's is `value`.
- `ValueChange<T>` and `Activate` do not propagate. Bevy triggers them on the widget: `Activate` on the button or menu item, `ValueChange<bool>` on the checkbox (and on a radio), `ValueChange<f32>` on the slider, `ValueChange<Entity>` on the radio group and the list box, `ValueChange<Option<Entity>>` on the tab list.
- `MenuEvent` and `RequestClose` auto-propagate and are meant for an owner. A menu's owner is a plain entity, not a widget, that opens and closes the popup on `MenuEvent`. `RequestClose` is triggered on the `ModalDialogBarrier` (barrier click, Escape), and the dialog sits inside the barrier, so the dialog never hears it.
- `FocusGained` and `FocusLost` auto-propagate.
- Bevy ships `checkbox_self_update`, `slider_self_update`, `radio_self_update` and `tablist_self_update`; none for list boxes.
- `ScrollbarThumb` has no `Node`; its border and radius are fields of `ScrollbarThumb`.
- `TabNavigationPlugin` adds a `Startup` system that observes the `PrimaryWindow` entity, and an observer that focuses a clicked `TabIndex` entity. It adds no resource. `DirectionalNavigationPlugin` only adds two resources, `DirectionalNavigationMap` and `AutoNavigationConfig`; `DirectionalNavigation` is a system param with `navigate(CompassOctant)` that reads the map. Bevy keeps both out of `DefaultPlugins`.
- Menus and modal dialogs in `bevy_ui_widgets` use the `TabNavigation` system param, which is queries only (`menu.rs`, `modal.rs`). They move focus without `TabNavigationPlugin`.
- mesh-picking (`openspec/changes/mesh-picking`) adds the first engine plugin switched from `bevy-ts.json`: `"meshPicking": true`, a bool, default `false`; not an engine hash input; baked into `game.json` by `bevy-ts build`; added in one place, `base_app` in `cli.rs`, after the project's plugins, unless a native plugin added it; and a once-per-process warning when a handler needs it and it is off.
- Viewport mode spawns its virtual `PrimaryWindow` inside `ViewportPlugin::build` (`crates/bevy-ts-viewport/src/lib.rs`), windowed mode gets winit's from `WindowPlugin::build`, and `--headless` has none (`headless.rs`, `primary_window: None`).

## Goals / Non-Goals

**Goals:**
- Every Bevy 0.20 UI element in research's stages 1 to 7 reachable from TSX, typed, with Bevy's names and Bevy's controlled widget model.
- Each stage lands on its own, tested headless, shown in `examples/ui-gallery`.
- Quiet frames stay as cheap as today.

**Non-Goals:**
- UI materials (stage 8) and `bevy_feathers` (proposal says why).
- Web DOM imitation: no `className`, no CSS strings, no `onChange`, no `value` attribute where Bevy has another name.
- Handlers that stop propagation. Handlers run in the next host call, after Bevy's propagation is done.
- Anything text-input owns.

## Decisions

1. One intrinsic per widget (settled). Each is a `<node>` with the widget's components, a new `UiKind` in `ui.rs`, named as the component in lower camel case. `scrollArea` is a prop of `<node>`, not a tag, since `ScrollArea` only adds wheel scrolling to a node. Alternative: component elements (`<node><Checkbox /></node>`). Rejected by research: state and events need props anyway, and several widget components lack `ReflectDefault`.

2. A one-field tuple struct's field is named `value` (settled), in `layout_reflect.rs`, so it holds for TSX elements, `ref.get` and TS system columns alike: `<ZIndex value={2} />`, `ref.get(Hovered).value`, `q.each((z) => z.value)`. One owner: the layout generator, as it already names a fieldless enum's field `value`. Props instead of the field only where the component is UI-only and common (`tabIndex`, `camera`). Alternative: per-component props everywhere. Rejected: it duplicates the component's name.

3. Gradients live in `background` and `border` (settled). `Part::Background` writes `BackgroundColor` for a colour or `BackgroundGradient` for a gradient value and removes the other; `Part::Border` the same with `BorderColor` and `BorderGradient`. The helpers (`linearGradient`, `radialGradient`, `conicGradient`) return tagged plain data. Alternative: a `backgroundGradient` prop. Rejected: two props would both own the background.

4. Q4: `NodeRef.has(C)` compares presence, not `RemovedComponents`. A `has` read registers a pair with a `present: bool` beside its tick. The per-frame check tests `contains_id(id) != present`, one archetype lookup, the same order of cost as the tick read a `get` pair already does. No `RemovedComponents` reader: that would read every removal of `C` in the world each frame and keep a cursor per component. A `has` pair is never dropped when the component is absent; absence is its value. `has` resolves any registered component by type path through the type registry, since markers have no native layout. Cost: a project with no `has` read pays nothing (no pair, and `frame_inner`'s early return holds); each `has` pair costs what a `get` pair costs. `tsx_scale` gains a row: 1,000 `has` pairs, quiet frame, measured against 1,000 `get` pairs. Alternative: no `has`, and widgets style through `onPointerPress` writing a signal. Rejected: it misses keyboard activation and `InteractionDisabled`, which only the markers carry. Ask-first: it touches the per-frame check.

5. Q5: where a widget event is heard. Every event prop is entity-local, as picking is: its handler hears events Bevy triggers on its own entity. That covers `Activate`, every `ValueChange`, `FocusGained` and `FocusLost`, since Bevy triggers each on the widget, and the prop goes on that widget's intrinsic (`onValueChange` on `<radioGroup>`, not on `<radio>`). Two events exist to reach an owner: `MenuEvent` and `RequestClose`. Their props (`onMenuEvent`, `onRequestClose`) go on any UI intrinsic and hear the event when it comes up from a descendant or the entity itself; the nearest entity on the path with the prop takes it, and its observer stops propagation, as a Bevy owner observer does. So `onRequestClose` goes on `<modalDialogBarrier>` or above, and `onMenuEvent` on the menu's container. Alternative: bubble every event. Rejected: picking's no-bubble rule is in the spec, and a bubbled `ValueChange` would reach an outer widget's handler with the wrong type.

6. Q6: navigation is opt-in, through mesh-picking's switch (operator, 2026-10-22). `bevy-ts.json` takes `tabNavigation` and `directionalNavigation`, bools, default `false`, one per Bevy plugin, as `meshPicking` is `MeshPickingPlugin`. They go through the path `meshPicking` already has, which stage 4 widens rather than copies: `base_app`'s `mesh_picking: bool` becomes one `Switches` value (`mesh_picking`, `tab_navigation`, `directional_navigation`) read from `bevy-ts.json` under `dev` and from `game.json` when playing; each field uses the same bool deserializer whose error names the field; `bevy-ts build` writes each that is `true` into `game.json`; none is an engine hash input. `base_app` adds each plugin after the project's plugins, unless one is already added. Off is the default because Bevy keeps both out of `DefaultPlugins`, and a game may bind Tab or the arrows to play. Off, the props still write their components: `tabIndex` without the plugin is inert, as in Bevy. With tab navigation off, the engine warns once per process when a `tabIndex` or `tabGroup` prop mounts, naming `"tabNavigation": true`, as mesh-picking warns for a mesh handler. It keys on the prop, not on `TabIndex`, since Bevy's `<tab>` writes its own roving `TabIndex` (`tabs.rs`) and tabs work by pointer without the plugin; it checks `is_plugin_added::<TabNavigationPlugin>()` after the project's plugins, so a native crate that adds it is not warned. With directional navigation off, `navigate` throws naming `"directionalNavigation": true`, since there is no map to read. `InputFocusVisible` needs neither. Menus and modals (stage 6) need neither: they use the `TabNavigation` param. `examples/ui-gallery` switches both on. On, Tab works in viewport mode: the plugin's `Startup` system finds the `PrimaryWindow`, and viewport mode spawns it in `ViewportPlugin::build`, before any schedule runs. Windowed mode has winit's window from `WindowPlugin::build`. `--headless` has no window: the plugin observes nothing, and Tab does nothing, as typing does nothing there (text-input). Alternative: both plugins in every engine. Rejected by the operator: Tab may mean something else in a game. Alternative: a `plugins: [...]` list of Bevy plugin names. Rejected: a second pattern beside `meshPicking`.

7. Controlled and `initial*` (settled rule). A widget's state prop is controlled (`checked`, `value`, `selected`) or initial (`initialChecked`, `initialValue`, `initialSelected`); both is a type error and a load error. Controlled: the binding writes the state component (`Checked`, `SliderValue`, `SelectedTab`); the handler writes game state; Bevy changes nothing itself. Initial: the state is written on mount only and Bevy's `*_self_update` observer is attached, so the widget updates itself. A `<radioGroup>` is uncontrolled when none of its `<radio>`s has `checked`, and then gets `radio_self_update`; a group mixing the two is a load error. `<listBox>` is controlled only: Bevy ships no list self-update, and bevy-ts does not write one.

8. `ValueChange<Entity>` and `ValueChange<Option<Entity>>` give `{ entity, value, key, isFinal }` (settled): `value` the chosen child's entity, as Bevy names it, and `key` that child's TSX `key` (its explicit key or `type#n`), read from the tree in Rust; `null` for a child TSX did not make. Controlled `selected` on `<tabList>` takes a key, which Rust maps to the child entity.

9. Payloads. `onValueChange` on `<checkbox>`, `<radio>` and `<slider>` gives `{ entity, value, isFinal }`. `onActivate`: `{ entity }`. `onMenuEvent`: `{ entity, source, action }` with `action` `"toggle"`, `"open-first"`, `"open-last"`, `"close-all"` or `"focus-root"`. `onRequestClose`: `{ entity, source }`. `onFocusGained`: `{ entity, cause }`. Drag, drop and scroll extend the picking payload with Bevy's fields (`delta`, `distance`, `dropped`, `unit`, `x`, `y`). All go through the one queue and the frame's one host call.

10. Overlays follow Bevy's structure. `<menuButton>` and `<menuPopup>` sit in a container whose `onMenuEvent` opens and closes the popup, usually through `<Show>`. `<menuPopup>` inserts `MenuFocusState::Opening(First)` on mount so focus lands in it, which keeps it open. `<popover>` is positioned against its parent and takes `positions` and `windowMargin`, Bevy's fields. `<dialog>` is `Dialog`, `<dialog modal>` adds `ModalDialog`; `<modalDialogBarrier>` is the screen cover around it.

11. `<scrollbarThumb>` has no `Node`. Its `background`, `border` (colour), `radius` and `style.border` (widths) map to `BackgroundColor`, `BorderColor` and `ScrollbarThumb`'s own fields. Its intrinsic owns no `Node` part, and `check` rejects other `style` keys.

12. Text spans. `<span>` is valid only as a child of `<text>` or `<span>`; it is a child entity with `TextSpan`, `TextFont`, `TextColor` and the decoration props, as Bevy builds rich text. Bound span content writes that span's `TextSpan` only.

13. Tests run with no screen. Each element is tested in a `--headless` engine: its components and computed layout, and its pointer behaviour by triggering Bevy's pointer events on the entity, as Bevy's own widget tests do (`radio.rs`, `tabs.rs`). Keyboard paths (Tab, arrows, Enter or Space on a widget, Escape on a barrier) need a primary window, so those tests run in viewport mode through `bevy-ts/viewport_input`, offscreen, as text-input's do.

14. `examples/ui-gallery` shows every element, one panel per stage, growing with each stage. It has no `example.json`, so it does not ship in the npm package, as `rapier` does not. A test loads it headless and fails when an intrinsic in `jsx-runtime.d.ts` `IntrinsicElements` does not appear in it.

## Risks / Trade-offs

- [The `value` rename breaks a script that wrote `"0"`] → none in the repo; docs and the stage 1 commit say so. Level files store Bevy reflection paths, not TS field names; a stage 1 test loads a level with a `ZIndex` to prove it.
- [`has` pairs grow the per-frame check] → one archetype lookup per pair; the `tsx_scale` row gates it.
- [Tab in edit mode moves game UI focus] → edit mode's keys are the editor's; a stage 4 viewport test checks Tab while editing.
- [A script sets `tabIndex` and forgets the switch] → the once-per-process warning names `"tabNavigation": true`; `navigate` throws naming `"directionalNavigation": true`.
- [mesh-picking has not landed when stage 4 starts] → stage 4 waits for it, as it waits for text-input.
- [Bevy's modal focus waits 0.1 s (`modal.rs` delayed command)] → tests advance time past it.
- [`MenuEvent` stopped by our observer before a Bevy observer higher up] → Bevy has none above the owner; the menu test opens, navigates and closes.
- [A `<radioGroup>` mixing controlled and initial radios] → load error naming the group.

## Migration Plan

Additive except the `value` rename (stage 1). Each stage lands alone, stages 3 and 4 after text-input, stage 4 also after mesh-picking. Protocol: none expected; if a protocol answer carries the renamed field, the next free minor at landing and `scripts/gen-protocol.sh`. Rollback is per stage.

## Open Questions

None.
