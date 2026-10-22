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
| `<text>` | `Text`, `TextFont`, `TextColor` |
| `<image src>` | `ImageNode` |
| `<button>` | `Node`, `Button`, picking |

An unknown prop, style key or word fails the load and names it with the accepted ones.

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
