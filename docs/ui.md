# Game UI in TSX

Game UI is bevy_ui, written in TSX. Each intrinsic is an entity with Bevy's UI components, and
each prop writes one component: a changed prop writes that component and no other. Names are
Bevy's. `examples/ui-gallery` shows every element (`bevy-ts dev src/gallery.tsx`).

```tsx
import { app, px } from "bevy-ts";

app.scene("hud", () => (
  <node style={{ position: "absolute", top: px(12), left: px(12), padding: px(8) }} background="#0008">
    <text size={20}>Score</text>
  </node>
));
```

| Intrinsic | Bevy |
|---|---|
| `<node>` | `Node` |
| `<text>` | `Text`, `TextFont`, `TextColor`, `TextLayout` |
| `<span>` | `TextSpan`, inside `<text>` or `<span>` |
| `<image src>` | `ImageNode` |
| `<viewport camera>` | `Node`, `ViewportNode` |
| `<button>` | `Node`, `Button`, picking |
| `<input>` | `Node`, `TextInput` (with `EditableText`) |

An unknown prop, style key or word fails the load and names it with the accepted ones.

A list is `<For each={items} key={(i) => i.id}>{(i) => ...}</For>`. Its rows keep their entities
by key, and sit in the items' order among their parent's children, so a reordered list moves rows
in the layout without respawning them. Rows sit where the `<For>` is written: after the siblings
before it and before the siblings after it. A change costs what it adds, removes or moves, not the
length of the list.

## Style

`style` is `Node`'s fields in camelCase. Lengths are numbers (pixels) or `px`, `percent`, `vw`,
`vh`, `em`, `rem`, `"auto"`. Words are Bevy's variants in kebab case (`"space-between"`,
`"row-dense"`, `"border-box"`).

- Flex: `display`, `flexDirection`, `flexWrap`, `flexGrow`, `flexShrink`, `flexBasis`,
  `justifyContent`, `alignItems`, `alignSelf`, `alignContent`, `gap`, `rowGap`, `columnGap`.
- Box: `width`, `height`, min and max of each, `aspectRatio`, `padding`, `margin`, `border`
  (widths), `boxSizing`, `position`, `left`, `right`, `top`, `bottom`, `direction`.
- Overflow: `overflow` (one word, or `{ x, y }`), `scrollbarWidth`, `overflowClipMargin`
  (`{ visualBox, margin }`).
- Grid: `gridTemplateRows`, `gridTemplateColumns`, `gridAutoRows`, `gridAutoColumns`,
  `gridAutoFlow`, `gridRow`, `gridColumn`, `justifyItems`, `justifySelf`.

Grid helpers come from `bevy-ts` and are named after Bevy's grid types:

```tsx
<node style={{ display: "grid", gridTemplateColumns: [fr(1), px(200), repeat(2, minmax(px(40), fr(2)))] }}>
  <node style={{ gridColumn: span(2) }} />
  <node style={{ gridRow: { start: 1, end: 3 } }} />
</node>
```

A track is a length, `fr(n)`, `"auto"`, `"min-content"`, `"max-content"`, `minmax(min, max)` or
`{ fitContent: length }`. `repeat` takes a count, `"auto-fill"` or `"auto-fit"`. A placement is a
start line, `span(n)`, `{ start, end, span }` (two of them) or `"auto"`.

Props beside `style`, on every UI intrinsic:

| Prop | Bevy | Takes |
|---|---|---|
| `background` | `BackgroundColor` or `BackgroundGradient` | a colour, a gradient, or several gradients |
| `border` | `BorderColor` or `BorderGradient` | a colour, `{ top, right, bottom, left }`, or gradients |
| `radius` | `Node.border_radius` | a length, `[topLeft, topRight, bottomRight, bottomLeft]` or by name |
| `outline` | `Outline` | `{ width, offset, color }`; a missing field is Bevy's default (1 px, 0, white) |
| `shadow` | `BoxShadow` | `{ color, x, y, spread, blur }` or an array; a missing field is Bevy's default (black, x and y 20%, spread 0, blur 10%) |
| `transform` | `UiTransform` | `{ translation: [x, y], scale, rotation }`, rotation in radians or `deg(n)` |

Colours are `"#rgb"`, `"#rrggbb"` or `"#rrggbbaa"`. Gradients:

```tsx
background={linearGradient(90, ["#f00", ["#00f", percent(80)]])}
border={radialGradient(["#fff", "#000"], { at: [px(10), px(20)], shape: { circle: px(30) } })}
background={conicGradient(["#f00", ["#0f0", 180]], { from: 90 })}
```

Angles are degrees, 0 up and clockwise, as CSS's. A stop is a colour, or `[colour, position]`
(`[colour, degrees]` for conic). `background` owns both its components: a colour removes the
gradient, a gradient removes the colour, and removing the prop removes both. `border` the same.

A Rust tuple struct with one field names it `value` in TS, so a UI component reads as
`<ZIndex value={2} />` and `ref.get(Hovered).value` (see [native](native.md)).

## Placing many nodes

A node that moves every frame keeps a static `style` and binds `transform`:

```tsx
<node style={{ position: "absolute", left: 0, top: 0 }}
  transform={() => ({ translation: [px(at().x), px(at().y)] })} />
```

A `style` binding rebuilds the node's `Node`, and Bevy then lays out its tree again in taffy.
`transform` writes only `UiTransform`, which Bevy applies after layout, so nothing is laid out
again. Picking and clipping read the drawn place: a moved button is clicked where it is drawn.
Bind `style` for what changes the layout (a size, a gap); place by `transform`.

## Text

`<text>` is screen UI; text in the 3D world is `<worldText>` ([world-text.md](world-text.md)).
`<text>` holds strings, bindings and `<span>`s. Text before the first `<span>` is the `Text`
entity's own; each `<span>`, and each run of text after one, is a child entity with `TextSpan`, as
Bevy builds rich text. A bound span writes its own `TextSpan` and nothing else.

```tsx
<text size={20} color="#fff">
  HP <span color="#f00" weight={700}>{() => `${hp()}`}</span> / 100
</text>
```

Bevy gives a span no inheritance; bevy-ts gives a `<span>` its parent's `size`, `color`, `font`,
`weight`, `italic`, `lineHeight` and `letterSpacing` unless it sets its own. A `<span>` outside a
`<text>` or `<span>` fails the load and names its location. `<Show>` and `<For>` do not go inside `<text>`: for a conditional
span, bind its text (`<span>{() => (low() ? "!" : "")}</span>`).

| Prop | On | Bevy | Takes |
|---|---|---|---|
| `size` | both | `TextFont.font_size` | pixels |
| `font` | both | `TextFont.font` | a font asset path |
| `weight` | both | `TextFont.weight` | 1 to 1000 (400 normal, 700 bold); needs a `font` with that weight |
| `italic` | both | `TextFont.style` | `true`; needs a `font` with an italic face |
| `color` | both | `TextColor` | a colour |
| `lineHeight` | both | `LineHeight` | a multiple of the font size, or `px(n)` |
| `letterSpacing` | both | `LetterSpacing` | pixels, `px(n)` or `rem(n)` |
| `textBackground` | both | `TextBackgroundColor` | a colour |
| `underline` | both | `Underline`, `UnderlineColor` | `true` or a colour |
| `strikethrough` | both | `Strikethrough`, `StrikethroughColor` | `true` or a colour; drawn at the font's own thickness, which at 14 px and below can fall between pixels and not show |
| `justify` | `<text>` | `TextLayout.justify` | `left`, `center`, `right`, `justified`, `start`, `end` |
| `linebreak` | `<text>` | `TextLayout.linebreak` | `word`, `char`, `word-or-char`, `none` |
| `textShadow` | `<text>` | `TextShadow` | `{ x, y, color }`; a missing field is Bevy's default |
| `textHalo` | `<text>` | bevy-ts's `TextHalo` | `{ width, color }`: an outline `width` pixels wide (more than 0, at most 4; default 1) in `color` (default black), round the whole text, spans too |

`textHalo` keeps text readable on any ground: the glyphs are drawn again in its colour at 8 offsets
round them (16 above 2 px), under the text, with no extra layout or entity. Each copy is a glyph to
draw: 40 labels moving every frame add about 1 ms; 1,000 take twice the frame time at 8 offsets
and three times at 16 (`docs/research/tsx-scale.md`). Text without a halo pays nothing.

## Cameras and images

`camera={ref}` on a root UI element draws it on that camera (`UiTargetCamera`), once the ref
mounts. On an element inside another it fails the load: Bevy reads it only on a root.

`<viewport camera={ref}>` is a `<node>` showing that camera's output (`ViewportNode`), sized to
the node. Bevy shows only a camera that renders to an image, so the viewport gives its camera an
image of its own, unless it already renders to one. That takes the camera off the window: give a
viewport a camera of its own, not the game's. The camera keeps that image when the viewport goes
or shows another camera. Under `--headless` the screenshot image counts as the window.

```tsx
const mini = ref();
<entity ref={mini}><Camera3d /><Transform translation={[0, 20, 0]} rotation={[-0.7071, 0, 0, 0.7071]} /></entity>
<viewport camera={mini} style={{ width: px(200), height: px(150) }} />
```

`<image>` writes `ImageNode`:

| Prop | Bevy | Takes |
|---|---|---|
| `src` | `ImageNode.image` | an asset path |
| `color` | `ImageNode.color` | a tint colour |
| `flipX`, `flipY` | `flip_x`, `flip_y` | `true` |
| `rect` | `rect` | `[x, y, width, height]` in pixels |
| `atlas` | `texture_atlas` | `{ layout: { tile: [w, h], columns, rows, padding, offset }, index }`, a grid layout (`TextureAtlasLayout::from_grid`); equal grids share one layout |
| `mode` | `image_mode` | `"auto"`, `"stretch"`, `sliced({ border, center, sides, maxCornerScale })` or `tiled({ x, y, stretch })` |

`sliced` is a nine-slice: `border` in pixels, one number or `{ left, right, top, bottom }`;
`center` and `sides` are `"stretch"` (the default) or `{ tile: n }`. `tiled` repeats along `x` and
`y` (both by default) once the image is stretched past `stretch` (1).

## Events

An event prop is `on` plus a Bevy event's name and an entity-local observer of it. Every UI
element and `<entity>` takes the picking events, which bubble as Bevy's do
([picking.md](picking.md)). A handler runs in the frame's one host call, in arrival order, and gets
the event with the call's `commands` and `frame`.

| Prop | Adds to `{ kind, entity, target, position, button }` |
|---|---|
| `onPointerClick`, `onPointerPress` | `count` |
| `onPointerRelease`, `onPointerOver`, `onPointerOut`, `onPointerEnter`, `onPointerLeave`, `onPointerCancel`, `onPointerDragStart` | nothing |
| `onPointerMove` | `delta` |
| `onPointerDrag` | `delta`, `distance` |
| `onPointerDragEnd` | `distance` |
| `onPointerDragEnter`, `onPointerDragOver`, `onPointerDragLeave` | `dragged` |
| `onPointerDragDrop` | `dropped` |
| `onPointerScroll` | `unit` (`"line"` or `"pixel"`), `x`, `y` |

Drags work on any picked entity: a UI node, a mesh, or a glTF scene's root, which hears a drag on
its meshes as it hears a click. As in Bevy, a drag starts on the first move while a button is
down, with no threshold; a game that wants one ignores `onPointerDrag` until `distance` passes it.

`<button onActivate>` hears Bevy's `Activate`: a click, or Enter or Space while the button has
focus. It does not bubble. `activateOnPress` (`ActivateOnPress`) activates on press rather than
release. `disabled` on any UI element inserts `InteractionDisabled` while true: Bevy's widgets
then ignore pointer and keys, and `onActivate` does not run.

```tsx
<button onActivate={({ commands }) => start(commands)} disabled={() => !ready()}>
  <text>Start</text>
</button>
```

## Reading markers

`ref.has(C)` is whether the entity carries `C`, a marker included (`Pressed`, `Hovered`,
`InteractionDisabled`; later `Checked`). In a binding it re-runs when that changes. Rust compares
presence after the bevy-ts stages each frame, without calling JS, so a quiet frame costs one
archetype lookup per read and no host call.

```tsx
const b = ref();
<button ref={b} background={() => (b.has(Pressed) ? "#246" : "#36c")} />
```

A `has` read stays watched while the component is absent: absence is its value. It works in UI
prop bindings, not in a bound component field.

## Focus

Every UI element takes Bevy's focus components as props:

- `tabIndex`: `TabIndex(n)`, a whole number. Tab visits lower numbers first, then tree order.
- `tabGroup`: `TabGroup`, `{ order, modal }` (defaults 0 and `false`). As in Bevy, Tab reaches
  only a `tabIndex` element inside a `tabGroup`. In a `modal` group, Tab stays inside it.
- `autoFocus`: `AutoFocus`; the element takes `InputFocus` when it mounts.

An `<input>` has `TabIndex(0)` without the prop; its `tabIndex` replaces it, and removing the prop
puts `TabIndex(0)` back.

```tsx
<node tabGroup={{}}>
  <button tabIndex={0} autoFocus onFocusGained={({ cause }) => ring(cause)}><text>Play</text></button>
  <button tabIndex={0}><text>Quit</text></button>
</node>
```

`onFocusGained({ entity, cause })` and `onFocusLost({ entity })` are the element's own focus,
Bevy's `FocusGained` and `FocusLost`: a descendant's focus does not reach them. `cause` is Bevy's:
`"navigated"` (Tab, the arrows, or a click on a `tabIndex` element), `"pressed"` (a widget that
focuses itself on press, as a checkbox) or `"auto"` (`autoFocus`).

Tab and the arrow keys are off by default: Bevy keeps both plugins out of its defaults, and a game
may bind Tab or the arrows itself. Turn them on in `bevy-ts.json`, as `meshPicking`
([picking.md](picking.md#what-picks)):

```json
{ "tabNavigation": true, "directionalNavigation": true }
```

- `tabNavigation` adds `TabNavigationPlugin`: Tab and Shift-Tab move focus, and a click focuses a
  `tabIndex` element. With it off, a `tabIndex` or `tabGroup` prop logs one warning per process
  naming `"tabNavigation": true`; an `<input>`'s own `TabIndex(0)` does not.
- `directionalNavigation` adds `DirectionalNavigationPlugin`. Bevy moves nothing on its own: a
  system calls `DirectionalNavigation.navigate` from `bevy-ts/input` on the keys it chooses, over
  elements with an `<AutoDirectionalNavigation />` child
  ([native.md](native.md#ports-and-built-in-modules)).

Keys need a primary window: they work in a game window and in viewport mode, not under
`--headless`. In edit mode the keys are the editor's: Tab moves no game focus and a field types nothing until
Play. `autoFocus` still takes `InputFocus` at mount in edit mode; the keys stay the editor's.

## `<input>`

A one-line text field: Bevy 0.20's `TextInput`. Focus, typing, selection, paste and Escape are
Bevy's; bevy-ts reads them. The text never wraps: wider than the field, it scrolls. A focused
field shows Bevy's blinking cursor in the text's `color`; Bevy draws no other focus cue, so a
game that wants one styles it.

```tsx
<input
  key="notes"
  autoFocus
  maxCharacters={200}
  onSubmit={({ value, commands }) => save(commands, value)}
  onCancel={() => close()}
/>
```

- `initialValue`: the text at mount. Like every `initial*` prop, a reload does not reset it, and
  what the player typed survives a reload.
- `maxCharacters`: edits past it are dropped, as Bevy does.
- `autoFocus`: the field takes `InputFocus` when it mounts ([Focus](#focus)).

There is no `value` prop. The text is Bevy's editor's; writing it from TS would fight the cursor
and IME state. To clear a field, remount it with a new `key`.

Its events, each with `entity`, are the field's own: unlike picking events, they do not bubble
from a descendant.

- `onSubmit({ value })`: Enter went down.
- `onCancel()`: Escape went down. Bevy has already cleared `InputFocus`.
- `onKeyboardInput({ key, code, text, state, repeat, value })`: Bevy's `KeyboardInput` as the field
  got it. `key` is the W3C `KeyboardEvent.key` (`"Enter"`, `"a"`), `code` the W3C
  `KeyboardEvent.code` (`"KeyA"`), `text` what the key types or `null`, `state` `"down"` or
  `"up"`. `value` is the text with this key and every key before it applied, even when several
  keys arrive in one frame.
- `onTextEditChange({ value })`: Bevy's `TextEditChange`, also fired on cursor moves. Bevy applies
  edits late in the frame, so the handler runs in the next frame's call.

Bevy's `TextInput` inserts a character only with no modifier or Shift, so Option (macOS) and
AltGr characters are not typed, as in a native Bevy window.

Keys reach a field only with a primary window: in a game window or in viewport mode, not under
`--headless` ([viewport.md](viewport.md#typing)). A key typed into a field still shows in
`Keys` from `bevy-ts/input`; check `InputFocus` for single-key shortcuts
([native.md](native.md#ports-and-built-in-modules)).
