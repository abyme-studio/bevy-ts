# Bevy 0.20 UI against bevy-ts TSX

Research for team bevy-ui-coverage, 2026-10-23. Read from the pinned crates in the cargo registry (`bevy_ui`, `bevy_ui_widgets`, `bevy_ui_render`, `bevy_text`, `bevy_input_focus`, `bevy_picking`, `bevy_feathers`, all 0.20.0) and from bevy-ts at `1c44e93`: `crates/bevy-ts-tsx/src/{ui,picking,reflected}.rs`, `packages/bevy-ts/{jsx-runtime,index,bevy}.d.ts`, `openspec/specs/{jsx-runtime,reactive-ui}`. Text input is the in-flight change `openspec/changes/text-input` (branch `text-input`).

## What bevy-ts has today

- Four UI intrinsics (`ui.rs`): `<node>`, `<text>`, `<image>`, `<button>`. Props: `style` (a CSS-cased subset of `Node`), `background` (`BackgroundColor`), `border` (`BorderColor::all`), `radius` (`BorderRadius::all`); `<text>` adds `size` and `color`; `<image>` adds `src` and `color`. `<button>` adds `bevy_ui_widgets::Button`.
- Five picking props on any entity: `onPointerClick|Press|Release|Over|Out`, entity-local, not bubbling (`picking.rs`).
- Any Bevy component element on a UI intrinsic (`<node><ZIndex .../></node>`). Native-layout ones are typed by `bevy.d.ts`; reflect-tier ones are inserted from `ReflectDefault` with untyped props. A newtype's field is named `"0"`, which JSX cannot write as an attribute. Components with no `ReflectDefault` (`ScrollArea`, `SliderValue`, `Scrollbar`) cannot be inserted.
- `NodeRef.get(C)` in bindings. No `has(C)`, so marker state (`Pressed`, `Checked`, `InteractionDisabled`) cannot drive a binding.
- `DefaultPlugins` already adds `UiWidgetsPlugins`, `InputFocusPlugin` and `InputDispatchPlugin`. It does not add `TabNavigationPlugin` or `DirectionalNavigationPlugin`. `bevy_feathers` is not enabled anywhere.

In flight in text-input, counted as covered: `<input>` (`UiKind::Input`) on `TextInput` with `initialValue`, `maxCharacters`, `autoFocus` (inserts `AutoFocus`), `size`, `color` and node style; `onTextEditChange`, `onKeyboardInput`, `onSubmit`, `onCancel`; `picking.rs` becomes `events.rs`; `bevy_ts::keys`; module `bevy-ts/input` with `Keys`, `InputFocus.get()`, `KeyCode`; `docs/ui.md`. Not in it: tab order, keyboard navigation, multi-line, placeholder, a controlled `value`.

## Bevy's widget model, and why it fits

`bevy_ui_widgets` is headless and controlled (its `lib.rs` docs): state lives in a component (`Checked`, `SliderValue`), the widget emits `ValueChange<T> { source, value, is_final }` and does not change its own state; the app writes the state back. Stateless widgets emit `Activate`. Each stateful widget ships a `*_self_update` observer that makes it uncontrolled.

That is bevy-ts's `initial*`/controlled rule already: `checked={() => s.on}` is controlled (no self-update; the handler writes game state, the binding writes `Checked`), `initialChecked` is uncontrolled (attach `checkbox_self_update`). No new model is needed. Handler props follow the existing rule `on` + Bevy event name: `onActivate`, `onValueChange`.

## Coverage

Full = expressible and typed today. Partial = some of it, or only through an awkward or untyped component element. None = not reachable.

### bevy_ui

| Element | Bevy type | Today | Proposed TSX |
|---|---|---|---|
| Box, flex | `Node` (display, position_type, sizes, insets, margin/padding/border, gaps, flex_*) | full | as today |
| Grid layout | `Node.grid_template_rows/columns`, `grid_auto_*`, `grid_auto_flow`, `grid_row/column` | none | `style={{ display: "grid", gridTemplateColumns: [fr(1), px(200)], gridColumn: span(2) }}` with `fr`, `minmax`, `repeat`, `span` helpers |
| Other `Node` fields | `box_sizing`, `justify_items`, `justify_self`, `align_content`, `direction`, `scrollbar_width`, `overflow_clip_margin`, per-axis `overflow` | none | `style` keys `boxSizing`, `justifyItems`, `justifySelf`, `alignContent`, `direction`, `scrollbarWidth`, `overflowClipMargin`, `overflow: { x, y }` |
| Corner radii | `BorderRadius` per corner | partial (all corners) | `radius={[tl, tr, br, bl]}` or `{ topLeft, ... }` |
| Border colour per side | `BorderColor { top, right, bottom, left }` | partial (all sides) | `border={{ top: "#fff", ... }}` |
| Outline | `Outline { width, offset, color }` | partial (component element, untyped `Val`s) | `outline={{ width: px(2), offset: px(1), color: "#fff" }}` |
| Box shadow | `BoxShadow(Vec<ShadowStyle>)` | none (Vec of structs, no typing) | `shadow={{ color, x, y, spread, blur }}` or an array |
| Gradients | `BackgroundGradient`, `BorderGradient`; `LinearGradient`, `RadialGradient`, `ConicGradient` | none | `background={linearGradient(90, ["#f00", "#00f"])}`; `border` the same. Helpers named as Bevy's types |
| Text | `Text`, `TextFont.font_size`, `TextColor` | full | as today |
| Text spans | `TextSpan` children | none (one string) | `<text>HP <span color="#f00">{() => hp}</span></text>` |
| Font, weight, style | `TextFont { font, weight, style, ... }` | none | `<text font="fonts/x.ttf" weight={700} italic>` |
| Text layout | `TextLayout { justify, linebreak }`, `LineHeight`, `LetterSpacing` | none | `justify="center"`, `linebreak="word"`, `lineHeight`, `letterSpacing` |
| Text decoration | `TextShadow`, `TextBackgroundColor`, `Underline`, `Strikethrough` (+ colours) | none | `textShadow={{ x, y, color }}`, `underline`, `strikethrough` (bool or colour) |
| Image | `ImageNode { image, color }` | full | as today |
| Image options | `flip_x/y`, `rect`, `texture_atlas`, `image_mode` (Auto, Stretch, Sliced, Tiled) | partial (flips via component; rest none) | `flipX`, `rect`, `atlas={{ layout, index }}`, `mode={sliced({ border: px(8) })}` |
| Inline image in text | `InlineImage` | none | later; low demand |
| Scroll position | `ScrollPosition(Vec2)` | partial (typed component, field `"0"`) | `scroll` / `initialScroll={[x, y]}` on `<node>`; reads via `ref.get(ScrollPosition)` |
| Scroll exemption | `IgnoreScroll(BVec2)` | partial (marker insertable) | component element, fine as is |
| Z-order | `ZIndex(i32)`, `GlobalZIndex(i32)` | partial (typed, but field `"0"`) | `<ZIndex value={2} />`: the generator names a newtype's field `value`, as it does for enum newtypes (`FocusPolicy`) |
| UI transform | `UiTransform { translation: Val2, scale, rotation }` | partial (only `scale`) | `transform={{ translation: [px(4), 0], rotation: deg(10), scale: [1, 1] }}` |
| Target camera | `UiTargetCamera(Entity)` | none | `camera={cameraRef}` on a root `<node>` |
| Render to texture | camera `RenderTarget::Image` + `UiTargetCamera` | none | a camera concern, not UI; `camera` above covers the UI half |
| Camera in UI | `ViewportNode { camera }` | none | `<viewport camera={ref} />` intrinsic |
| UI material | `MaterialNode<M: UiMaterial>` | none | `<node material={Kind} param=... />` from a `uiMaterial()` in bevy-ts-material. Ask first (below) |
| Interaction markers | `Pressed`, `InteractionDisabled`, `Checkable`, `Checked`, `Selectable`, `Selected` | partial (insertable; not readable as booleans) | `disabled` prop for `InteractionDisabled`; `ref.has(Pressed)` in bindings |
| Hover | `bevy_picking::hover::Hovered` | partial (via `get`) | `ref.get(Hovered)` in bindings, with `Hovered` added by intrinsics that style on hover |
| Accessibility | `AccessibleLabel`, `AccessibilityNode` | none | `label="Close"` on intrinsics |
| Auto nav | `AutoDirectionalNavigation` | full (typed component) | component element |
| Deprecated focus | `DeprecatedInteraction`, `FocusPolicy`, `RelativeCursorPosition`, `DeprecatedButton` | n/a | do not expose; picking and markers replace them |
| Ghost nodes, debug overlay, `LayoutConfig`, `FixedNode` | experimental / debug | n/a | leave to component elements |

### Picking events

| Element | Bevy type | Today | Proposed TSX |
|---|---|---|---|
| Click, press, release, over, out | `PointerClick`, `PointerPress`, `PointerRelease`, `PointerOver`, `PointerOut` | full | as today |
| Enter, leave, move, cancel | `PointerEnter`, `PointerLeave`, `PointerMove`, `PointerCancel` | none | `onPointerEnter`... (rule already in spec) |
| Drag and drop | `PointerDragStart`, `PointerDrag`, `PointerDragEnd`, `PointerDragEnter/Over/Leave/Drop` | none | `onPointerDrag` etc., payload adds `delta`, `distance`, `dropped` entity |
| Wheel | `PointerScroll` | none | `onPointerScroll`, payload `unit`, `x`, `y` |

### Focus and navigation

| Element | Bevy type | Today | Proposed TSX |
|---|---|---|---|
| Focused entity | `InputFocus` resource | in flight (`bevy-ts/input` `InputFocus.get()`) | text-input |
| Autofocus | `AutoFocus` | in flight on `<input>`; insertable marker elsewhere | `autoFocus` on every focusable intrinsic |
| Focus visible | `InputFocusVisible(bool)` | none | `InputFocusVisible` read beside `InputFocus` in `bevy-ts/input` |
| Focus events | `FocusGained`, `FocusLost` (`gained_and_lost.rs`) | none | `onFocusGained`, `onFocusLost` |
| Tab order | `TabIndex(i32)`, `TabGroup { order, modal }`, `TabNavigationPlugin` | partial (typed, field `"0"`; plugin not added, so Tab does nothing) | `tabIndex={0}` on focusable intrinsics, `<node tabGroup={{ order: 0, modal: true }}>`; engine adds `TabNavigationPlugin` |
| Directional nav | `DirectionalNavigation`, `DirectionalNavigationMap`, `AutoDirectionalNavigation`, plugin | partial (component only; plugin not added) | engine adds `DirectionalNavigationPlugin`; gamepad/arrow driving from TS systems |

### bevy_ui_widgets

| Element | Bevy type | Today | Proposed TSX |
|---|---|---|---|
| Button | `Button`, `Activate`, `ActivateOnPress` | partial (no `onActivate`, so keyboard activation is lost) | `<button onActivate={...} activateOnPress disabled>` |
| Checkbox / toggle | `Checkbox`, `Checked`, `ValueChange<bool>`, `checkbox_self_update` | none (no events) | `<checkbox checked={() => s.on} onValueChange={({ value }) => ...}>` or `initialChecked` |
| Radio | `RadioGroup`, `RadioButton`, `ValueChange<Entity>`, `radio_self_update` | none | `<radioGroup onValueChange={({ value }) => ...}><radio key="a" checked={...} /></radioGroup>` |
| Slider | `Slider { track_click, orientation }`, `SliderValue`, `SliderRange`, `SliderStep`, `SliderPrecision`, `SliderThumb`, `ValueChange<f32>` | none (no `ReflectDefault` on `SliderValue`) | `<slider value={() => vol()} min={0} max={1} step={0.1} onValueChange>` with a `<sliderThumb>` child |
| Scrollbar | `Scrollbar { target, orientation, min_thumb_length }`, `ScrollbarThumb` | none | `<scrollbar target={listRef} orientation="vertical"><scrollbarThumb /></scrollbar>`; no events, it writes `ScrollPosition` |
| Scroll area | `ScrollArea` (wheel scrolling), `ScrollIntoView` | none (no `ReflectDefault`) | `<node scrollArea style={{ overflow: { y: "scroll" } }}>` |
| Tabs | `TabList { orientation, activation }`, `Tab`, `SelectedTab`, `ValueChange<Entity>` | partial (`TabList` typed; no events) | `<tabList selected={...} onValueChange><tab key="a">` |
| List box | `ListBox`, `ListBoxMultiSelect`, `ListItem`, `ActiveDescendant` | none | `<listBox onValueChange><listItem key=.../></listBox>` |
| Menu | `MenuButton`, `MenuPopup { layout }`, `MenuItem`, `MenuEvent`, `MenuAction` | none | `<menuButton>`, `<menuPopup>`, `<menuItem onActivate>` |
| Popover | `Popover { positions, window_margin }`, `PopoverPlacement` | none | `<popover anchor... placements={[{ side: "bottom", align: "start", gap: 4 }]}>` |
| Dialog | `Dialog`, `DialogDragHandle`, `RequestClose`, `ModalDialog`, `ModalDialogBarrier` | none | `<dialog modal onRequestClose>`; shown with `<Show>` |
| Text input | `TextInput`, `EditableText`, `SelectAllOnFocus` | in flight (`<input>`) | text-input; `selectAllOnFocus` later |

### bevy_feathers

Present in 0.20 (button, checkbox, radio, slider, toggle switch, number input, color controls, menu, select, list view, dialog, scrollbar, text input, virtual keyboard; themes and tokens). Its own docs say it is styled for the Bevy editor and "deliberately not intended for" game UI. Not enabled in bevy-ts. Recommendation: do not expose it to game TSX. Enabling it would be a new dependency (ask first).

## Proposed shape

- One intrinsic per Bevy widget, named as the widget component in lower camel case (`<checkbox>`, `<slider>`, `<radioGroup>`, `<radio>`, `<scrollbar>`, `<tabList>`, `<tab>`, `<menuButton>`, `<popover>`, `<dialog>`), each a `<node>` plus its widget components, as `<button>` and `<input>` are. A state prop has a controlled and an `initial*` form; `initial*` attaches Bevy's `*_self_update` observer.
- Visual props stay on all UI intrinsics: `outline`, `shadow`, gradients through `background`/`border`, `transform`, `disabled`, `tabIndex`, `autoFocus`, `label`.
- Events live in `events.rs` (text-input's rename): `onActivate`, `onValueChange` (payload `value`, `isFinal`; radio and tabs give the chosen child's `key` beside its entity), `onFocusGained`, `onFocusLost`, `onRequestClose`, the remaining picking events.
- `NodeRef.has(C)` for marker reads in bindings, so a button styles itself: `background={() => b.has(Pressed) ? "#246" : "#36c"}`.

## Open questions

1. Widgets as intrinsics or as component elements on `<node>` (`<node><Checkbox /><Checked /></node>`)? Component elements are Bevy's own model and need no new tags, but state and events need props anyway, and several widget components lack `ReflectDefault`. Recommendation: intrinsics, as `<button>` and `<input>` already are.
2. Newtype fields: name them `value` in the generator (`<ZIndex value={2} />`) or add props (`zIndex`)? Recommendation: `value`, since `FocusPolicy` and `Name` already read that way; props only where the component is UI-only and common.
3. Gradients inside `background`, or a separate `backgroundGradient` prop? Bevy has two components; one prop that writes either keeps one owner of "the background". Recommendation: inside `background`.
4. `has(C)` needs removal detection (`RemovedComponents`) in the subscription check, which today watches change ticks only.
5. Bubbling: Bevy widgets stop propagation of what they consume and expect it elsewhere. bevy-ts handlers do not bubble. `onValueChange` on `<radioGroup>` works because Bevy triggers it on the group; others may need a bubbling rule. (2026-10-23, change scene-pick: bevy-ts picking handlers now bubble up `ChildOf` as Bevy's do, with `target` naming the entity hit; see docs/picking.md.)
6. `TabNavigationPlugin` attaches its Tab observer to the `PrimaryWindow` at `Startup`. Does viewport mode's virtual window exist by then?
7. `ValueChange<Entity>` for radio, tabs and list box: give TS the child's `key` as well as its entity?

## Ask-first items

- UI materials (`MaterialNode` from bevy-ts-material): a new material target and render pipeline in an existing crate. Architectural and could affect performance.
- Enabling `bevy_feathers`: a new dependency. Recommended against.
- Adding `TabNavigationPlugin` and `DirectionalNavigationPlugin` to the engine set: small, but a change to every engine's plugins.
- `has(C)` removal tracking: touches the per-frame subscription check, which is performance-sensitive (`docs/research/tsx-scale.md`, quiet frame 0.8 µs).

## Suggested stages

1. Style completeness: grid, the missing `Node` fields, per-corner radius, per-side border colour, `outline`, `shadow`, gradients, `transform`, newtype `value` naming. Pure mapping in `ui.rs`; no new model.
2. Text: `<span>`, font, weight, layout, decoration, text shadow.
3. Events and state reads: `onActivate`, the rest of the picking events, `NodeRef.has`, `disabled`. Needs `events.rs` from text-input landed first.
4. Focus and navigation: `tabIndex`, `tabGroup`, `autoFocus` everywhere, focus events, `InputFocusVisible`, the two plugins. After text-input (shares `bevy-ts/input`).
5. Value widgets: `<checkbox>`, `<radioGroup>`/`<radio>`, `<slider>`, `<scrollbar>`, `scrollArea`, `<tabList>`/`<tab>`, `<listBox>`.
6. Overlays: `<menuButton>`/`<menuPopup>`, `<popover>`, `<dialog>`.
7. Cameras and images: `camera`, `<viewport>`, image modes and atlases.
8. UI materials, if the operator says go.
