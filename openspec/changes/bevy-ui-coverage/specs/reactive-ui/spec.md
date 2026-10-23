## MODIFIED Requirements

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
`onPointerClick`, `onPointerPress`, `onPointerRelease`, `onPointerOver`, `onPointerOut`, `onPointerEnter`, `onPointerLeave`, `onPointerMove`, `onPointerCancel`, `onPointerDragStart`, `onPointerDrag`, `onPointerDragEnd`, `onPointerDragEnter`, `onPointerDragOver`, `onPointerDragLeave`, `onPointerDragDrop` and `onPointerScroll` on any `<entity>` or UI element SHALL register handler ids with Rust, which SHALL observe the matching Bevy 0.20 picking event on that entity and queue it; each prop is an entity-local observer, as `on(...)` is in BSN (operator ruling 2026-10-20). The prop name is `on` plus the Bevy event's name. Queued events SHALL be delivered in the same per-frame host call as subscription changes, in arrival order. A handler SHALL receive the event (entity, pointer position, button) and a `ctx` with `commands` and `frame`; drag events add `delta` and `distance`, drag-and-drop events add `dropped` (the dragged entity), and `onPointerScroll` adds `unit` (`"line"` or `"pixel"`), `x` and `y`. Picking events SHALL NOT bubble.

#### Scenario: Three clicks in one frame
- **WHEN** three buttons are clicked in one frame
- **THEN** one host call runs the three handlers in click order

#### Scenario: A drop
- **WHEN** an entity is dragged onto a `<node onPointerDragDrop={h}>`
- **THEN** `h` runs once with `dropped` the dragged entity

#### Scenario: A child's click
- **WHEN** a `<node onPointerClick={h}>` has a child `<node>` and the child is clicked
- **THEN** `h` does not run

## ADDED Requirements

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

### Requirement: Image modes and atlases
`<image>` SHALL take `flipX`, `flipY`, `rect` (`[x, y, w, h]`), `atlas` (`{ layout, index }`) and `mode`, written to `ImageNode`: `mode` is `"auto"`, `"stretch"`, `sliced({ border, center, sides })` or `tiled({ x, y, stretch })`, Bevy's `NodeImageMode` variants.

#### Scenario: A nine-slice panel
- **WHEN** `<image src="ui/panel.png" mode={sliced({ border: 8 })} />` mounts
- **THEN** its `ImageNode.image_mode` is `Sliced` with an 8 px border

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
