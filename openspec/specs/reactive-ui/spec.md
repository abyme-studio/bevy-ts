# reactive-ui Specification

## Purpose
TSX components run once, and a function-valued prop or child is a binding. Reads inside a binding subscribe to Bevy change detection in Rust, with no hooks, so only the bound field is patched and quiet frames cost no calls into JS. The four UI intrinsics map onto bevy_ui, and `bevy_picking` events reach JS in one batched call per frame.

## Requirements

### Requirement: Components run once; functions are bindings
Prefab and UI component functions SHALL run once per mount and once per reload. A prop or child whose value is a function SHALL be a binding: bevy-ts SHALL run it in a tracking scope, write its result on mount, and re-run it when a signal it read changes, patching only that field of that node. `bevy-ts` SHALL export `signal`, `computed`, `effect`, `untrack`, `onCleanup` and `ref`, implemented inside the `bevy-ts` module.

#### Scenario: One text node updates
- **WHEN** a HUD with ten text nodes binds one of them to `() => count()` and `count` changes
- **THEN** exactly one `Text` component is written and the other nine change ticks do not move

### Requirement: TSX has no hooks
`bevy-ts` SHALL export no `use*` functions (operator ruling 2026-10-20). A binding SHALL read game state directly: `target.get(C)` for component `C` of the entity behind node ref `target` (`ref(entity)` wraps an `Entity`), and `res(R)` for resource `R`. `signal()` SHALL remain for UI-local state that is not game state, such as a dropdown's open flag or a text field's draft.

#### Scenario: A HUD without a hook
- **WHEN** a HUD writes `<text>{() => `HP ${player.get(Health).hp}`}</text>`
- **THEN** it shows the player's `hp` on mount and updates when it changes, with no hook call in the component body

### Requirement: Reads inside bindings subscribe to change detection in Rust
A `target.get(C)` or `res(R)` read inside a binding SHALL register the pair (entity or resource entity, `ComponentId`) with Rust for that binding. After the bevy-ts stages each frame, Rust SHALL check the change ticks of subscribed pairs without calling JS. Only when one moved SHALL it deliver the new values to JS, in one host call per frame for all subscriptions and events, and re-run the bindings that read them. A subscription SHALL be dropped when its binding's owner is disposed.

#### Scenario: Enemies take damage, the player does not
- **WHEN** the HUD subscribes to the player's `Health` and a system changes only enemies' `Health` for 60 frames
- **THEN** no host call into JS is made in those frames and the HUD is not written

#### Scenario: The player takes damage
- **WHEN** a system lowers the player's `hp`
- **THEN** in the same frame one host call delivers it and the HUD text shows the new value

### Requirement: Bindings on native-layout fields are derived fields with one owner
A binding on a field of a component with a native layout SHALL be grouped by site (the element and prop in source) together with every instance of that element. Each frame Rust SHALL find the instances whose reads changed without calling JS, and SHALL deliver them in the per-frame host call. JS SHALL evaluate the site's closure once per changed instance through the row accessors and SHALL write the field through the column view with the run's tick only when the value differs from the row's. The reconciler SHALL NOT write a bound field from a literal. A site's reads SHALL be fixed at its first evaluation; under `bevy-ts dev`, a later run that reads something new SHALL be an error naming the site. A binding on a reflected component SHALL follow the per-pair path above instead.

#### Scenario: Ten thousand bound lights, one hundred alarms change
- **WHEN** a prefab binds `PointLight.intensity` to its own `Alarm`, is instanced 10,000 times, and a system changes 100 alarms in one frame
- **THEN** one host call runs the site's closure 100 times, 100 `PointLight` rows are written, and the frame costs within 2× of a TS system doing the same

#### Scenario: A quiet frame
- **WHEN** no instance's reads changed in a frame
- **THEN** no host call is made and no `PointLight` change tick moves

#### Scenario: Same value
- **WHEN** an alarm changes but the binding evaluates to the value already in the row
- **THEN** the row is not written and its change tick does not move

### Requirement: Reading an accessor outside a tracking scope warns in dev
Under `bevy-ts dev`, reading a signal, `target.get(C)` or `res(R)` directly in a component body or a prop value (not inside a function binding, `computed` or `effect`) SHALL log a warning once per location, because the value will never update.

#### Scenario: A forgotten arrow
- **WHEN** a HUD writes `<text>{player.get(Health).hp}</text>`
- **THEN** a warning names `hud.tsx` and the line, and the text shows the value at mount

### Requirement: Control flow components
`bevy-ts` SHALL export `<Show when fallback>` and `<For each key>`. When their inputs change, they SHALL mount or unmount only the affected subtree through the reconciler, keyed by `key` for `<For>` items, and dispose the owners of unmounted subtrees. A `<For>`'s item entities SHALL be in its items' order among their parent's children, and a changed order SHALL move entities without despawning any. The work of one `<For>` change SHALL grow with the items added, removed or moved, not with the items already mounted: growing a list from 0 to N items SHALL cost time linear in N.

#### Scenario: A list item removed
- **WHEN** a `<For each={items} key={(i) => i.id}>` loses the middle item
- **THEN** only that item's entities are despawned and the other items keep their entity ids

#### Scenario: Bound children of a mounted subtree
- **WHEN** `<Show when={() => s() !== null}><text>{() => `show ${s()}`}</text></Show>` mounts with `s` already set, or a `<Show>` or `<For>` mounts its content later (a signal set by a system or a handler)
- **THEN** each bound child shows its value in the frame its subtree mounts, and follows its signals afterwards

#### Scenario: A list reversed
- **WHEN** a `<For>` over `["a", "b", "c"]` is set to `["c", "b", "a"]`
- **THEN** no entity is spawned or despawned, every item keeps its entity id, and the parent's `Children` hold the items' entities in the order c, b, a

#### Scenario: A list grows from nothing
- **WHEN** a `<For>`'s items go from 0 to 10,000 in one signal set
- **THEN** the 10,000 items mount in the next host call, and the time taken at 100,000 items is no more than about ten times the time at 10,000 (`bench/tsx-for/`)

### Requirement: UI intrinsics over bevy_ui
`<node>` SHALL spawn an entity with `Node`; its `style` prop SHALL map to `Node`'s fields (lengths through `px`, `percent`, `vw`, `vh`, `em`, `rem` helpers) and `background`, `border`, `radius` to `BackgroundColor`, `BorderColor` and `BorderRadius`. `style` SHALL cover every layout field of Bevy 0.20's `Node`: besides today's keys, `boxSizing`, `justifyItems`, `justifySelf`, `alignContent`, `direction`, `scrollbarWidth`, `overflowClipMargin`, `overflow` as one word or `{ x, y }`, and grid: `gridTemplateRows`, `gridTemplateColumns`, `gridAutoRows`, `gridAutoColumns`, `gridAutoFlow`, `gridRow` and `gridColumn`, with helpers `fr`, `minmax`, `repeat` and `span` named after Bevy's grid types. `radius` SHALL take one length or four corners (`[topLeft, topRight, bottomRight, bottomLeft]` or `{ topLeft, ... }`), and `border` one colour or `{ top, right, bottom, left }`. `<text>` SHALL spawn `Text` with `size` and `color` mapping to `TextFont` and `TextColor`; string and bound children SHALL be its content. `<image src>` SHALL spawn `ImageNode` with the asset loaded. `<button>` SHALL be a `<node>` with `Button` and picking enabled. An unknown prop, style key or word SHALL be an error naming it and the accepted ones. UI entities SHALL be reconciled by the same reconciler as scene entities.

#### Scenario: A styled panel
- **WHEN** `<node style={{ padding: px(8) }} background="#0008">` mounts
- **THEN** the entity's `Node.padding` is 8 px on every side and its `BackgroundColor` is black at 50% alpha

#### Scenario: A grid
- **WHEN** `<node style={{ display: "grid", gridTemplateColumns: [fr(1), px(200)] }}>` holds a child with `style={{ gridColumn: span(2) }}`
- **THEN** the parent's `Node.grid_template_columns` is one flex track and one 200 px track, and the child's `Node.grid_column` spans 2

#### Scenario: Per-corner radius
- **WHEN** `<node radius={[px(4), 0, px(4), 0]}>` mounts
- **THEN** `Node.border_radius` has 4 px top-left and bottom-right and 0 elsewhere

#### Scenario: A misspelt key
- **WHEN** `style={{ gridTemplateColums: [fr(1)] }}` is loaded
- **THEN** the load fails naming `gridTemplateColums`

### Requirement: Events from bevy_picking, batched
`onPointerClick`, `onPointerPress`, `onPointerRelease`, `onPointerOver`, `onPointerOut`, `onPointerEnter`, `onPointerLeave`, `onPointerMove`, `onPointerCancel`, `onPointerDragStart`, `onPointerDrag`, `onPointerDragEnd`, `onPointerDragEnter`, `onPointerDragOver`, `onPointerDragLeave`, `onPointerDragDrop` and `onPointerScroll` on any `<entity>` or UI element SHALL register handler ids with Rust, which SHALL observe the matching Bevy 0.20 picking event on that entity and queue it; each prop is an entity-local observer, as `on(...)` is in BSN (operator ruling 2026-10-20). The prop name is `on` plus the Bevy event's name. Events SHALL bubble up `ChildOf` as Bevy's do: a handler SHALL also run for an event whose original target is a descendant of its entity, including a mesh of a scene spawned under a `WorldAssetRoot`. Queued events SHALL be delivered in the same per-frame host call as subscription changes, in arrival order. A handler SHALL receive the event (`entity`, the entity whose handler runs; `target`, the entity the pointer hit; pointer position; button) and a `ctx` with `commands` and `frame`; click and press add `count`, `onPointerMove` adds `delta`, drag events add `delta` and `distance` as Bevy gives them, `onPointerDragEnter`, `onPointerDragOver` and `onPointerDragLeave` add `dragged`, `onPointerDragDrop` adds `dropped` (the dragged entity), and `onPointerScroll` adds `unit` (`"line"` or `"pixel"`), `x` and `y`. A handler SHALL NOT be able to stop propagation: it runs after Bevy's walk.

#### Scenario: Three clicks in one frame
- **WHEN** three buttons are clicked in one frame
- **THEN** one host call runs the three handlers in click order

#### Scenario: A click bubbles to its parent's handler
- **WHEN** an entity with `onPointerClick` has a child entity, and the child is clicked
- **THEN** the parent's handler runs with `entity` the parent and `target` the child

#### Scenario: A click on a glTF scene's mesh
- **WHEN** an entity carries `WorldAssetRoot("models/Fox.glb#Scene0")` and `onPointerClick`, and a mesh of the spawned fox is clicked
- **THEN** the root's handler runs once, with `entity` the root and `target` the mesh's entity

#### Scenario: A drop
- **WHEN** an entity is dragged onto a `<node onPointerDragDrop={h}>`
- **THEN** `h` runs once with `dropped` the dragged entity

### Requirement: An input intrinsic over Bevy's TextInput
`<input>` SHALL be a `<node>` with Bevy 0.20's `TextInput` (which requires `EditableText`), as `<button>` is a `<node>` with `Button`. It SHALL take the node props (`style`, `background`, `border`, `radius`), `size` and `color` (mapping to `TextFont` and `TextColor` as on `<text>`), and:
- `initialValue`: the text at mount, set through `EditableText::new` or the editor in Rust, never through reflection (`EditableText` has no `Reflect`). Like every `initial*` prop, a reload SHALL NOT reset it.
- `maxCharacters`: `EditableText::max_characters`; edits past it are ignored, as Bevy does.
- `autoFocus`: when true, the entity gets `AutoFocus`, so `InputFocus` moves to it when it is added.

The field SHALL be one line, Bevy's default (`allow_newlines: false`, one visible line). Focus, typing, selection, paste and Escape SHALL be Bevy's own `TextInput` behaviour; bevy-ts SHALL NOT capture keys for the field itself.

#### Scenario: A focused field with a starting value
- **WHEN** `<input initialValue="hi" autoFocus />` mounts
- **THEN** the entity has `Node`, `TextInput` and `EditableText` whose value is `"hi"`, and `InputFocus` holds it

#### Scenario: A reload keeps what was typed
- **WHEN** the operator types `"abc"` into a field mounted with `initialValue=""` and the file reloads
- **THEN** the field's entity id and its value `"abc"` are kept

#### Scenario: Too many characters
- **WHEN** `<input maxCharacters={3} autoFocus />` mounts and `a`, `b`, `c`, `d` are typed
- **THEN** its value is `"abc"`

### Requirement: Events from an input, batched
`<input>` SHALL take `onTextEditChange`, `onKeyboardInput`, `onSubmit` and `onCancel`. Each SHALL be an entity-local observer on the field, as the picking props are:
- `onTextEditChange` of `TextEditChange`, named `on` plus the Bevy event; the handler gets `{ entity, value }`. It fires when Bevy fires it, which includes cursor moves with no change to `value`.
- `onKeyboardInput` of `FocusedInput<KeyboardInput>` whose original target is the field, named `on` plus the Bevy event; the handler gets `{ entity, key, code, text, state, repeat, value }`: `key` the logical key as a W3C `KeyboardEvent.key` string (`"Enter"`, `"Escape"`, `"a"`), `code` the `KeyCode` as a W3C `KeyboardEvent.code` string (`"KeyA"`), `text` the key's text or `null`, `state` `"down"` or `"up"`.
- `onSubmit` from the same observer, on `Enter` down that is not a repeat; the handler gets `{ entity, value }`.
- `onCancel` from the same observer, on `Escape` down that is not a repeat; the handler gets `{ entity }`.

`onSubmit` and `onCancel` are the one place bevy-ts names an event Bevy lacks: Bevy 0.20 has no submit, and its own widgets that need one (feathers' number input, `ValueChange { is_final: true }` on Enter) build it the same way, from a keyboard observer on the field.

`value` SHALL be the field's text with this key and every earlier key applied: Bevy's global `TextInput` observer runs first and queues this key's edit, and bevy-ts applies the field's pending edits before reading `EditableText::value()` in Rust. It is sent as a string. Events SHALL be queued and delivered with picking events in the frame's one host call, in arrival order, each with a `ctx` holding `commands` and `frame`; for one key, `onKeyboardInput` comes before `onSubmit` or `onCancel`. Escape SHALL keep Bevy's behaviour: the field loses focus, and the handlers still run.

#### Scenario: Type and submit
- **WHEN** a focused `<input onKeyboardInput={k} onSubmit={s} />` in viewport mode receives key down events `h`, `i` and `Enter` through `bevy-ts/viewport_input`
- **THEN** `k` runs with `value` `"h"`, `"hi"` and `"hi"`, and `s` runs once with `value` `"hi"`

#### Scenario: Three keys in one frame
- **WHEN** `h`, `i` and `Enter` reach the field in the same frame
- **THEN** `onSubmit` still gets `value` `"hi"`

#### Scenario: Escape
- **WHEN** a focused `<input onCancel={c} onSubmit={s} />` receives `Escape`
- **THEN** `c` runs once, `s` does not, and `InputFocus` is empty afterwards

#### Scenario: A held Enter
- **WHEN** `Enter` is held and the viewer sends repeats
- **THEN** `onSubmit` runs once, and `onKeyboardInput` runs for each repeat with `repeat` true

#### Scenario: A change event
- **WHEN** `a` is typed into a focused `<input onTextEditChange={h} />`
- **THEN** `h` runs with `value` `"a"` in the next frame's call

### Requirement: Visual props on every UI intrinsic
Every UI intrinsic SHALL take `outline` (`{ width, offset, color }` to `Outline`), `shadow` (one `{ color, x, y, spread, blur }` or an array to `BoxShadow`), `transform` (`{ translation, scale, rotation }` to `UiTransform`, `rotation` through `deg` or radians), and gradients through `background` and `border`: `linearGradient`, `radialGradient` and `conicGradient` helpers return values that `background` writes as `BackgroundGradient` and `border` as `BorderGradient`. Each of `background` and `border` SHALL own both its components: writing a colour removes the gradient and writing a gradient removes the colour, and removing the prop removes both.

#### Scenario: Colour to gradient
- **WHEN** `background={() => hot() ? linearGradient(90, ["#f00", "#00f"]) : "#222"}` flips from the colour to the gradient
- **THEN** the entity has `BackgroundGradient` and no `BackgroundColor`

#### Scenario: A shadow and an outline
- **WHEN** `<node shadow={{ color: "#0008", x: 0, y: px(4), blur: px(8) }} outline={{ width: px(2), color: "#fff" }}>` mounts
- **THEN** it has a `BoxShadow` of one `ShadowStyle` with those values and an `Outline` 2 px wide

### Requirement: Rich text
`<text>` SHALL take `font` (an asset path), `weight` (a number), `italic`, `justify` (`"left"`, `"center"`, `"right"`, `"justified"`), `linebreak` (`"word"`, `"char"`, `"word-or-char"`, `"none"`), `lineHeight`, `letterSpacing`, `textShadow` (`{ x, y, color }`), `textBackground` (a colour), `underline` and `strikethrough` (`true` or a colour), mapping to `TextFont`, `TextLayout`, `LineHeight`, `LetterSpacing`, `TextShadow`, `TextBackgroundColor`, `Underline` (and `UnderlineColor`) and `Strikethrough` (and `StrikethroughColor`). `<span>` SHALL be valid only inside `<text>` or `<span>`, SHALL spawn a child entity with `TextSpan`, and SHALL take `size`, `color` and the text props above that Bevy applies per span. A bound span SHALL write only its own `TextSpan`. Bevy gives a span no inheritance, so a `<span>` SHALL take its parent's `size`, `color`, `font`, `weight`, `italic`, `lineHeight` and `letterSpacing` where it does not set them, a span's own prop winning; `textBackground`, `underline`, `strikethrough`, `justify`, `linebreak` and `textShadow` SHALL NOT be inherited.

#### Scenario: A coloured number
- **WHEN** `<text>HP <span color="#f00">{() => hp()}</span></text>` mounts and `hp` changes
- **THEN** the text entity has a child with `TextSpan` and `TextColor` red, and only that child's `TextSpan` is written

#### Scenario: A span takes its parent's size
- **WHEN** `<text size={20}>HP <span color="#f00">5</span></text>` mounts
- **THEN** the span's `TextFont` is 20 px and its `TextColor` red

#### Scenario: A span outside text
- **WHEN** `<node><span>x</span></node>` is loaded
- **THEN** the load fails naming `<span>` and its location

### Requirement: Text halo
`<text>` SHALL take `textHalo` (`{ width, color }`), written as bevy-ts's `TextHalo`: `width` in logical pixels, default 1, more than 0 and at most 4; `color` default black. The halo SHALL draw the text's laid-out glyphs, spans included, in `color` at 8 offsets `width` around them (16 when `width` is above 2), under the glyphs, with no extra text layout and no extra entity. Changing or removing `textHalo` SHALL redraw the text without laying it out again. `<span>` SHALL NOT take `textHalo`: a `textHalo` on a span fails the load naming its location.

#### Scenario: A label on pale and dark ground
- **WHEN** `<text color="#fff" textHalo={{ width: 1.5, color: "#000" }}>Harbour</text>` is drawn headless over a pale ground and over a dark one
- **THEN** in both screenshots the pixels just outside each glyph's edge are the halo's colour and the glyphs stay white

#### Scenario: A halo removed
- **WHEN** a bound `textHalo` goes from a value to `undefined`
- **THEN** the entity has no `TextHalo`, the next frame draws no halo, and the text's `TextLayoutInfo` is not recomputed

#### Scenario: A halo on a span
- **WHEN** `<text>a <span textHalo={{ width: 1 }}>b</span></text>` is loaded
- **THEN** the load fails naming `textHalo`, `<span>` and its location

### Requirement: Placement without layout
`transform` SHALL write only `UiTransform` and never `Node`, so a binding that moves a node each frame through `transform.translation` SHALL cause no taffy layout. The moved node SHALL be drawn, clipped and picked at its transformed place.

#### Scenario: A thousand labels moved each frame
- **WHEN** 1,000 `<node>`s with a static `style` bind `transform={() => ({ translation: [px(x()), px(y())] })}` and `x` changes every frame
- **THEN** no `Node` change tick moves, each `ComputedNode` keeps its size, and each `UiGlobalTransform` follows `x`

#### Scenario: A moved node is picked where it is drawn
- **WHEN** a `<button>` moved 200 px right by `transform` is clicked at its drawn place
- **THEN** its `onPointerClick` runs

### Requirement: Image modes and atlases
`<image>` SHALL take `flipX`, `flipY`, `rect` (`[x, y, w, h]`), `atlas` (`{ layout, index }`) and `mode`, written to `ImageNode`: `mode` is `"auto"`, `"stretch"`, `sliced({ border, center, sides })` or `tiled({ x, y, stretch })`, Bevy's `NodeImageMode` variants.

#### Scenario: A nine-slice panel
- **WHEN** `<image src="ui/panel.png" mode={sliced({ border: 8 })} />` mounts
- **THEN** its `ImageNode.image_mode` is `Sliced` with an 8 px border

#### Scenario: The mode helpers return Bevy's reflected shape
- **WHEN** `<image src="ui/a.png" mode={{ Tiled: { tile_x: false, tile_y: true, stretch_value: 2 } }} />` mounts, the shape `tiled({ x: false, stretch: 2 })` returns
- **THEN** its `ImageNode.image_mode` is `Tiled { tile_x: false, tile_y: true, stretch_value: 2.0 }`
- **AND** a mode naming a field Bevy's type does not have fails to load, naming the field

### Requirement: UI on a camera
A root UI intrinsic SHALL take `camera`, a node ref, written as `UiTargetCamera` with the ref's entity once it is mounted; `camera` on a UI element inside another SHALL fail the load naming its location. `<viewport camera>` SHALL be a `<node>` with `ViewportNode` showing that camera's output. Bevy shows only a camera that renders to an image, so a viewport SHALL give its camera an image of its own (a render target, sized to the node by Bevy) unless the camera already renders to an image other than the window's; this takes the camera off the window. A windowless run's image that stands for the window (`--headless`) SHALL count as the window.

#### Scenario: A minimap
- **WHEN** a scene mounts `<entity ref={mini}><Camera3d /></entity>` and `<viewport camera={mini} style={{ width: px(200), height: px(200) }} />`
- **THEN** the viewport node has `ViewportNode` whose camera is that entity, and the camera renders to an image of its own while the game's camera stays on the window

#### Scenario: A camera with its own image
- **WHEN** the viewport's camera already renders to an image the game made
- **THEN** it keeps that image

#### Scenario: Headless
- **WHEN** a `--headless` run mounts a `<viewport>` while every camera renders to its screenshot image
- **THEN** the viewport's camera gets an image of its own, and the game camera and the screenshot image's size are unchanged

### Requirement: Marker reads with has
`NodeRef` SHALL have `has(C): boolean`, true while the entity carries component `C`, for any registered component, markers included (`Pressed`, `Checked`, `InteractionDisabled`, `Hovered`). Inside a binding it SHALL subscribe to presence: after the bevy-ts stages each frame Rust SHALL compare the component's presence with the delivered value without calling JS, and deliver a change in the frame's one host call. A `has` subscription SHALL NOT be dropped while the component is absent. A frame in which no binding reads `has` SHALL cost what it costs today.

#### Scenario: A pressed button
- **WHEN** `<button ref={b} background={() => b.has(Pressed) ? "#246" : "#36c"}>` is pressed and released
- **THEN** its `BackgroundColor` is `#246` while pressed and `#36c` after, with one host call for each change

#### Scenario: Quiet frames
- **WHEN** 1,000 bindings read `has(Pressed)` and nothing is pressed for 60 frames
- **THEN** no host call is made in those frames

### Requirement: Activation and disabled
`<button>` SHALL take `onActivate`, an entity-local observer of `Activate`, which Bevy raises on click and on Enter or Space while the button has focus, and `activateOnPress` (inserts `ActivateOnPress`). Every UI intrinsic SHALL take `disabled`, which inserts `InteractionDisabled` while true and removes it while false; Bevy's widgets ignore input while it is present.

#### Scenario: Keyboard activation
- **WHEN** a focused `<button onActivate={h}>` receives `Enter` in viewport mode
- **THEN** `h` runs once with the button's entity

#### Scenario: Disabled
- **WHEN** `<button disabled onActivate={h}>` is clicked
- **THEN** `h` does not run and the entity has `InteractionDisabled`
