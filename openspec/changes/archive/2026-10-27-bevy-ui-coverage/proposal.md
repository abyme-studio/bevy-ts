## Why

Game UI in bevy-ts is bevy_ui through TSX (operator, 2026-10-23), but TSX reaches four intrinsics and a slice of `Node`. A script cannot lay out a grid, draw a shadow or gradient, style a span of text, read `Pressed`, move focus with Tab, or use any `bevy_ui_widgets` widget beyond `<button>`. Research (`docs/research/bevy-ui-coverage.md`, faa5aff) mapped all of Bevy 0.20's UI against TSX; this change closes every gap in its stages 1 to 7, with Bevy's names and model.

## What Changes

Seven stages and two small ones (3a, 3b), each landable on its own. Stages 3 and 4 wait for text-input (`openspec/changes/text-input`) to land: they build on its `events.rs`, `<input>`, `bevy_ts::keys` and `bevy-ts/input`. Stage 4 also waits for mesh-picking (`openspec/changes/mesh-picking`): it reuses that change's `bevy-ts.json` switch.

1. Style. `style` takes grid (`gridTemplateColumns`, `gridRow`, ... with `fr`, `minmax`, `repeat`, `span`) and the missing `Node` fields. `radius` takes corners, `border` takes sides. New props on every UI intrinsic: `outline`, `shadow`, `transform`. Gradients go inside `background` and `border` (`linearGradient`, `radialGradient`, `conicGradient`); one prop owns the background. A one-field tuple struct's field is named `value` in TS (`<ZIndex value={2} />`, `ref.get(Hovered).value`). **BREAKING** for any script that wrote the field as `"0"`; none in the repo does.
2. Text. `<span>` children of `<text>` (`TextSpan`), `font`, `weight`, `italic`, `justify`, `linebreak`, `lineHeight`, `letterSpacing`, `textShadow`, `underline`, `strikethrough`, `textBackground`.
3. Events and state reads. `onActivate` on `<button>` (keyboard activation), `activateOnPress`, `disabled` on every UI intrinsic (`InteractionDisabled`), the remaining picking events (`onPointerEnter`, `Leave`, `Move`, `Cancel`, `DragStart`, `Drag`, `DragEnd`, `DragEnter`, `DragOver`, `DragLeave`, `DragDrop`, `Scroll`), and `NodeRef.has(C)` for marker reads in bindings.
3a. Text halo. `textHalo={{ width, color }}` on `<text>` outlines its glyphs, spans included, so a label reads over pale and dark ground. bevy-ts draws it in Bevy's UI extraction, as Bevy draws `TextShadow`, at 8 or 16 offsets, with no extra layout. Asked by agent town's map labels.
3b. Placement. A node moved every frame binds `transform` (`UiTransform`), which does no taffy layout, instead of `style` `left`/`top`, which rebuilds `Node`. No new prop: a test, benches and docs prove and teach the path. Asked by agent town's map labels.
4. Focus and navigation. `tabIndex`, `tabGroup`, `autoFocus` on every UI intrinsic, `onFocusGained`, `onFocusLost`; `InputFocusVisible` and `DirectionalNavigation` in `bevy-ts/input`. `"tabNavigation": true` and `"directionalNavigation": true` in `bevy-ts.json` add Bevy's `TabNavigationPlugin` and `DirectionalNavigationPlugin`. Both are off by default, as in Bevy: Tab may mean something else in a game. They are switched on the way `meshPicking` is, through the same path: no second pattern.
5. Value widgets. `<checkbox>`, `<radioGroup>`, `<radio>`, `<slider>`, `<sliderThumb>`, `<scrollbar>`, `<scrollbarThumb>`, `scrollArea`, `<tabList>`, `<tab>`, `<listBox>`, `<listItem>`. State props are controlled (`checked`) or `initial*` (`initialChecked`, which attaches Bevy's `*_self_update` observer). `onValueChange` gives `{ value, isFinal }`; for radio, tabs and list box `value` is the chosen child's `key`, beside its `entity`.
6. Overlays. `<menuButton>`, `<menuPopup>`, `<menuItem>`, `onMenuEvent`, `<popover>`, `<dialog>` (with `modal`), `<modalDialogBarrier>`, `<dialogDragHandle>`, `onRequestClose`.
7. Cameras and images. `camera` on a root UI node (`UiTargetCamera`), `<viewport camera>` (`ViewportNode`), and `<image>`'s `flipX`, `flipY`, `rect`, `atlas` and `mode` (`stretch`, `sliced`, `tiled`).

Every stage tests each element headless and adds it to `examples/ui-gallery`, a new example that shows every UI element. Docs extend text-input's `docs/ui.md`.

Not in this change:
- Stage 8, UI materials (`MaterialNode` from bevy-ts-material). A new material target and render pipeline; ask-first, not yet asked.
- `bevy_feathers`. Its own docs say it is styled for the Bevy editor and not for game UI; enabling it is a new dependency.
- `InlineImage`, ghost nodes, the debug overlay, `LayoutConfig`, `FixedNode`: low demand or experimental; component elements reach them.
- What text-input owns: `<input>`, its events, `bevy_ts::keys`, `Keys`, `InputFocus`, `KeyCode`.

## Capabilities

### New Capabilities
- `ui-widgets`: the `bevy_ui_widgets` intrinsics (value widgets and overlays), their state props, events and the rule for where a widget event is heard; focus props and focus events.

### Modified Capabilities
- `reactive-ui`: style, text (with `textHalo`), image and camera props on the UI intrinsics; placement through `transform` without layout; the remaining picking events; `NodeRef.has(C)`; `onActivate` and `disabled`.
- `jsx-runtime`: the new intrinsics join the list.
- `native-components`: a one-field tuple struct's field is `value` in TS.
- `script-input` (text-input's): `InputFocusVisible` and `DirectionalNavigation`; the `tabNavigation` and `directionalNavigation` switches.
- `shipped-game`: `game.json` may also hold `tabNavigation` and `directionalNavigation`.

## Impact

- `crates/bevy-ts-tsx`: `ui.rs` (props, new `UiKind`s, `TextHalo`), a render-world `extract_text_halos` (Bevy's `bevy_ui_render` feature, already on in the CLI), text-input's `events.rs` (new observers, the ancestor rule for `MenuEvent` and `RequestClose`), `host.rs` and `tree.rs` (`has` subscriptions), `reconcile.rs` if `<span>` needs its own child kind.
- `crates/bevy-ts`: `layout_reflect.rs` (field `value`).
- `crates/bevy-ts-cli`: `types.rs` and the shipped `bevy.d.ts`; text-input's `input.rs` (two params); mesh-picking's switch path (`project.rs`, `base_app` in `cli.rs`, `build.rs`, `play.rs`) takes two more switches; tests.
- `packages/bevy-ts`: `jsx-runtime.d.ts`, `jsx-runtime.js`, `index.d.ts` (`NodeRef.has`, helpers `fr`, `minmax`, `repeat`, `span`, `deg`, gradient and image-mode helpers), `bevy.d.ts` regenerated.
- `examples/ui-gallery` (new, not shipped in the npm package). Its `bevy-ts.json` switches both navigation plugins on.
- Docs: `docs/ui.md` sections per stage; `docs/native.md` for the `value` field and the two params; mesh-picking's `docs/picking.md` and the `bevy-ts.json` comment in `project.rs` list the two switches.
- Protocol: none expected. If the `value` rename shows in a protocol answer, the next free minor at landing.
- Performance: none with the switches off (the default). On, Bevy's own cost: one observer on the window for Tab, and two resources for directional navigation. `textHalo` costs 8 glyph quads per glyph per frame (16 above 2 px), with no extra layout: 3,840 quads at 40 12-glyph labels, 96,000 at 1,000; frame time measured in stage 3a. Placement through `transform` takes work away (no taffy layout for moved nodes).
- Operator go: given by lead@bevy-ts on 2026-10-22 for stage 3 (`has(C)` presence checks in the per-frame subscription check) and stage 4 (navigation plugins, opt-in from `bevy-ts.json`). Stage 3a needs a go, not yet given: its per-frame halo quads (performance) and bevy-ts's first system in Bevy's UI extraction. Nothing else in this change needs a go: the two switches follow mesh-picking's pattern, not a new one; stage 3b adds no work and no API.
