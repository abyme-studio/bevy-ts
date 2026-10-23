Each stage lands on its own: its code, its tests (every new element tested headless, keyboard paths in viewport mode offscreen), its `examples/ui-gallery` panel, its `docs/ui.md` section, and `bevy.d.ts` regenerated. Stages 3 and 4 start only after text-input lands; stage 4 also after mesh-picking. The operator's go for stages 3 and 4 is given (proposal, Impact).

## 0. Gallery

- [x] 0.1 `examples/ui-gallery` (no `example.json`, so it does not ship): `src/gallery.tsx` with today's `<node>`, `<text>`, `<image>`, `<button>` (and `<input>` once text-input lands), one panel per stage
- [x] 0.2 `crates/bevy-ts-cli/tests/ui_gallery.rs`: load the gallery headless; fail naming any intrinsic in `jsx-runtime.d.ts` `IntrinsicElements` that it does not mount

## 1. Style

- [x] 1.1 `layout_reflect.rs`: a one-field tuple struct's field is `value`; tuple structs with more fields keep `"0"`, `"1"`; regenerate `bevy.d.ts`; test `<ZIndex value={2} />`, `ref.get(ZIndex).value` and a TS system reading `TabIndex.value`
- [x] 1.2 Check that a level holding a `ZIndex` still loads and that no protocol answer changes; if one does, the next free minor at landing and `scripts/gen-protocol.sh`
- [x] 1.3 `ui.rs` `style`: `boxSizing`, `justifyItems`, `justifySelf`, `alignContent`, `direction`, `scrollbarWidth`, `overflowClipMargin`, `overflow: { x, y }`
- [x] 1.4 `ui.rs` `style` grid keys and `index.d.ts` helpers `fr`, `minmax`, `repeat`, `span` (plain data the Rust side parses)
- [x] 1.5 `radius` per corner; `border` per side
- [x] 1.6 `outline`, `shadow` (one or many), `transform` with `deg`; each its own `Part`
- [x] 1.7 Gradients: `linearGradient`, `radialGradient`, `conicGradient` helpers; `Part::Background` and `Part::Border` write the colour or the gradient component and remove the other (design 3)
- [x] 1.8 `jsx-runtime.d.ts`: types for every new prop and style key; type tests for a misspelt key
- [x] 1.9 Tests: one per prop family, each checking the component written and that changing one prop writes only its component
- [x] 1.10 Gallery panel and `docs/ui.md` "Style" section; `docs/native.md` on the `value` field

## 2. Text

- [x] 2.1 `<text>` props `font`, `weight`, `italic`, `justify`, `linebreak`, `lineHeight`, `letterSpacing`, `textShadow`, `textBackground`, `underline`, `strikethrough`
- [x] 2.2 `<span>`: a `UiKind` valid only under `<text>` or `<span>` (load error otherwise); `TextSpan` plus span props; a bound span writes only its `TextSpan`
- [x] 2.3 Tests: each prop's component; a bound span changes and its siblings' ticks do not move; `<span>` under `<node>` fails naming its location
- [x] 2.4 Gallery panel and `docs/ui.md` "Text" section

## 3. Events and state reads (after text-input; operator go for 3.4)

- [ ] 3.1 `events.rs`: observers for `PointerEnter`, `PointerLeave`, `PointerMove`, `PointerCancel`, the seven drag events and `PointerScroll`, entity-local; payload fields per reactive-ui
- [ ] 3.2 `<button onActivate>` (`Activate`, entity-local), `activateOnPress`
- [ ] 3.3 `disabled` on every UI intrinsic: insert or remove `InteractionDisabled`
- [ ] 3.4 `NodeRef.has(C)`: JS registers a `has` pair; `Subscription` gains presence; `frame_inner` compares `contains_id` with it; resolve any registered component by type path; never drop a `has` pair on absence (design 4)
- [ ] 3.5 `tsx_scale`: a row with 1,000 `has` pairs on a quiet frame against 1,000 `get` pairs; the one-HUD quiet frame stays at today's 0.8 µs; record in `docs/research/tsx-scale.md`
- [ ] 3.6 Tests: each pointer event headless by triggering Bevy's event; a child's click not heard by its parent; `Enter` on a focused button runs `onActivate` (viewport mode); a disabled button does not; `has(Pressed)` styles a button with one host call per change and none on quiet frames
- [ ] 3.7 Gallery panel and `docs/ui.md` "Events" and "Reading markers" sections

## 4. Focus and navigation (after text-input and mesh-picking)

- [ ] 4.1 Widen mesh-picking's switch, not copy it (design 6): `base_app`'s `mesh_picking: bool` becomes one `Switches` (`mesh_picking`, `tab_navigation`, `directional_navigation`); `project.rs` reads `tabNavigation` and `directionalNavigation` with the same named-error bool deserializer; `build.rs` writes each `true` one into `game.json` and `play.rs` reads them; `base_app` adds `TabNavigationPlugin` and `DirectionalNavigationPlugin` after the project's plugins unless already added; not engine hash inputs; the `bevy-ts.json` comment in `project.rs` lists them
- [ ] 4.1a The once-per-process warning when a `tabIndex` or `tabGroup` prop mounts with tab navigation off (not for a `<tab>`'s own `TabIndex`), beside mesh-picking's, added only when `TabNavigationPlugin` is not added after the project's plugins
- [ ] 4.1b `examples/ui-gallery/bevy-ts.json`: `"tabNavigation": true`, `"directionalNavigation": true`
- [ ] 4.2 `tabIndex`, `tabGroup`, `autoFocus` on every UI intrinsic
- [ ] 4.3 `onFocusGained` and `onFocusLost`, entity-local
- [ ] 4.4 text-input's `input.rs`: `InputFocusVisible` (`get`) and `DirectionalNavigation` (`navigate`, throwing naming `"directionalNavigation": true` when there is no `DirectionalNavigationMap`), `CompassOctant` TS enum, `.d.ts` and `.js`
- [ ] 4.5 Tests in viewport mode, switches on: Tab through three buttons with focus events; a modal tab group traps focus; arrow keys with `DirectionalNavigation` over `AutoDirectionalNavigation` buttons; play, stop, play then one Tab moves one step; Tab while editing does not move game focus
- [ ] 4.6 Test headless: with both switches the plugins load and Tab does nothing with no primary window
- [ ] 4.6a Switch tests, mirroring mesh-picking's: off by default (no plugin, no `DirectionalNavigationMap`, Tab moves nothing in viewport mode); a non-bool fails `dev` naming the field and `bevy-ts.json`; build writes each `true` switch into `game.json` and the played app has its plugin; an old `game.json` plays with both off; one warning for two `tabIndex` buttons with tab navigation off, none when a native plugin added it; `navigate` throws with directional navigation off
- [ ] 4.7 Gallery panel and `docs/ui.md` "Focus" section (the switches, and that Tab is off by default); `docs/native.md` for the two params; mesh-picking's `docs/picking.md` lists the two switches beside `meshPicking`

## 5. Value widgets

- [ ] 5.1 `UiKind`s `checkbox`, `radioGroup`, `radio`, `slider`, `sliderThumb`, `scrollbar`, `scrollbarThumb` (no `Node`; design 11), `tabList`, `tab`, `listBox`, `listItem`; `scrollArea` on `<node>`
- [ ] 5.2 Controlled and `initial*` per ui-widgets: bindings write `Checked`, `SliderValue`, `SelectedTab` (by key), `Selected`; `initial*` writes on mount and attaches Bevy's self-update; the radio-group mix is a load error
- [ ] 5.3 `onValueChange` observers for `ValueChange<bool>`, `<f32>`, `<Entity>`, `<Option<Entity>>`, entity-local; `key` from the tree for entity values
- [ ] 5.4 `jsx-runtime.d.ts`: props per widget, `onValueChange` payload typed per widget, controlled/initial conflicts as type errors
- [ ] 5.5 Tests per widget headless (pointer by triggered events): controlled checkbox with and without a handler, initial checkbox kept across reload, radio group gives `key`, a child checkbox not heard by the group, slider drag `isFinal` false then true, scrollbar drag writes `ScrollPosition`, `scrollArea` wheel scroll, tab list by key, list box selection; keyboard on each in viewport mode
- [ ] 5.6 Gallery panel and `docs/ui.md` "Widgets" section

## 6. Overlays

- [ ] 6.1 `UiKind`s `menuButton`, `menuPopup` (inserts `MenuFocusState::Opening(First)` on mount), `menuItem`, `popover`, `dialog` (`modal`), `modalDialogBarrier`, `dialogDragHandle`
- [ ] 6.2 `onMenuEvent` and `onRequestClose`: observers that take the event from the entity or a descendant, nearest handler wins, stop propagation (design 5); `onActivate` on `<menuItem>`
- [ ] 6.3 Tests: menu open, navigate, activate, close-all (viewport mode for keys); popover placement against its parent; modal barrier click and `Escape` raise `onRequestClose` with the barrier as `source` (advance time past Bevy's 0.1 s modal focus); a dialog dragged by its handle
- [ ] 6.4 Gallery panel and `docs/ui.md` "Overlays" section

## 7. Cameras and images

- [ ] 7.1 `camera` on a root UI intrinsic (`UiTargetCamera` once the ref mounts); `<viewport camera>` (`ViewportNode`)
- [ ] 7.2 `<image>` `flipX`, `flipY`, `rect`, `atlas`, `mode` with `sliced` and `tiled` helpers
- [ ] 7.3 Tests: UI on a second camera; a viewport node's camera; each image option's `ImageNode` field
- [ ] 7.4 Gallery panel and `docs/ui.md` "Cameras and images" section

## 8. Close

- [ ] 8.1 `openspec validate bevy-ui-coverage --strict` passes
- [ ] 8.2 `docs/ui.md` says UI materials and `bevy_feathers` are not available, and why
