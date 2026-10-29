// `bevy-ts/jsx-runtime`: the automatic JSX runtime (`"jsxImportSource": "bevy-ts"`). Elements
// are plain descriptions: an `<entity>`, component elements on it (`<Health initialHp={100} />`),
// prefabs (functions from props to one element) and the asset intrinsics `<mesh>`, `<material>`.
import type { AssetLoad, AssetRef, AssetValue, ColorValue, Commands, Component, ElementProps, Entity, For, NodeRef, ReflectComponent } from "bevy-ts";
import type { Image, TextureAtlasLayout } from "bevy-ts/bevy";
import type { AlphaMode, KindFields } from "bevy-ts/material";

/** Bevy 0.20's picking events, each an `on` prop on any `<entity>` or UI element. */
export type PointerKind =
  | "PointerClick" | "PointerPress" | "PointerRelease" | "PointerOver" | "PointerOut" | "PointerEnter" | "PointerLeave" | "PointerMove"
  | "PointerCancel" | "PointerDragStart" | "PointerDrag" | "PointerDragEnd" | "PointerDragEnter" | "PointerDragOver" | "PointerDragLeave"
  | "PointerDragDrop" | "PointerScroll";

/** The fields Bevy adds to some picking events, by kind. */
interface PointerExtras {
  PointerClick: { readonly count: number };
  PointerPress: { readonly count: number };
  /** How far the pointer moved since the last event. */
  PointerMove: { readonly delta: readonly [number, number] };
  /** `delta` since the last drag event, `distance` since the drag started. */
  PointerDrag: { readonly delta: readonly [number, number]; readonly distance: readonly [number, number] };
  PointerDragEnd: { readonly distance: readonly [number, number] };
  /** The entity being dragged over this one. */
  PointerDragEnter: { readonly dragged: Entity };
  PointerDragOver: { readonly dragged: Entity };
  PointerDragLeave: { readonly dragged: Entity };
  /** The entity dropped on this one. */
  PointerDragDrop: { readonly dropped: Entity };
  PointerScroll: { readonly unit: "line" | "pixel"; readonly x: number; readonly y: number };
}

/** One picking event's payload. */
export type PointerEventOf<K extends PointerKind> = PointerEventBase & { readonly kind: K } & (K extends keyof PointerExtras ? PointerExtras[K] : {});
/** Any picking event's payload. */
export type PointerEvent = { [K in PointerKind]: PointerEventOf<K> }[PointerKind];

/** What a picking handler receives: the event, and the call's `commands` and `frame`. */
interface PointerEventBase {
  /** The entity whose handler runs. */
  readonly entity: Entity;
  /**
   * The entity the pointer hit: `entity` itself, or a descendant the event bubbled up from (a
   * mesh of a glTF scene spawned under a `WorldAssetRoot`, a button inside a `<node>`). Compare
   * the two to take only your own hits; a handler cannot stop the bubbling.
   */
  readonly target: Entity;
  /** The pointer's position in its render target. */
  readonly position: readonly [number, number];
  /** For click, press, release and the drag events. */
  readonly button: "primary" | "secondary" | "middle" | null;
  readonly commands: Commands;
  readonly frame: number;
}
export type PointerHandler<K extends PointerKind = PointerKind> = (event: PointerEventOf<K>) => void;

/** What every `<input>` handler receives besides its event: the call's `commands` and `frame`. */
interface InputEventBase {
  readonly entity: Entity;
  readonly commands: Commands;
  readonly frame: number;
}
/** Bevy's `TextEditChange`: the field applied edits, cursor moves included. */
export interface TextEditChangeEvent extends InputEventBase {
  readonly kind: "TextEditChange";
  readonly value: string;
}
/** Bevy's `KeyboardInput`, as the focused field got it. */
export interface KeyboardInputEvent extends InputEventBase {
  readonly kind: "KeyboardInput";
  /** The logical key as W3C `KeyboardEvent.key`: `"Enter"`, `"Escape"`, `"a"`. */
  readonly key: string;
  /** The physical key as W3C `KeyboardEvent.code`: `"KeyA"`. */
  readonly code: string;
  /** The text the key types, or `null`. */
  readonly text: string | null;
  readonly state: "down" | "up";
  readonly repeat: boolean;
  /** The field's text with this key and every key before it applied. */
  readonly value: string;
}
/** Enter went down in the field. */
export interface SubmitEvent extends InputEventBase {
  readonly kind: "Submit";
  readonly value: string;
}
/** Escape went down in the field; it has already lost focus. */
export interface CancelEvent extends InputEventBase {
  readonly kind: "Cancel";
}

export interface KeyAttribute {
  key?: string | number;
}

export declare const Fragment: unique symbol;

/** A description node. Building one runs nothing. */
export interface Element {
  readonly type: unknown;
  readonly props: Record<string, unknown>;
  readonly key: string | null;
}

export type Child = Element | null | undefined | false | true | readonly Child[];

/** A prop given as a value, or bound: `() => value`, re-run when what it reads changes. */
export type Bindable<T> = T | (() => T);

export declare function jsx(type: unknown, props: Record<string, unknown> | null, key?: string | number): Element;
export declare function jsxs(type: unknown, props: Record<string, unknown> | null, key?: string | number): Element;

type Vec3 = [number, number, number];

export interface EntityProps {
  /** The entity's `Name`. */
  name?: string;
  /** Fills a `ref()` with this entity, so bindings elsewhere can read it. */
  ref?: NodeRef;
  /** Components and child entities. */
  children?: Child;
  /** Entity-local observers of Bevy 0.20's picking events; they bubble up `ChildOf`, as Bevy's do. */
  onPointerClick?: PointerHandler<"PointerClick">;
  onPointerPress?: PointerHandler<"PointerPress">;
  onPointerRelease?: PointerHandler<"PointerRelease">;
  onPointerOver?: PointerHandler<"PointerOver">;
  onPointerOut?: PointerHandler<"PointerOut">;
  onPointerEnter?: PointerHandler<"PointerEnter">;
  onPointerLeave?: PointerHandler<"PointerLeave">;
  onPointerMove?: PointerHandler<"PointerMove">;
  onPointerCancel?: PointerHandler<"PointerCancel">;
  onPointerDragStart?: PointerHandler<"PointerDragStart">;
  onPointerDrag?: PointerHandler<"PointerDrag">;
  onPointerDragEnd?: PointerHandler<"PointerDragEnd">;
  onPointerDragEnter?: PointerHandler<"PointerDragEnter">;
  onPointerDragOver?: PointerHandler<"PointerDragOver">;
  onPointerDragLeave?: PointerHandler<"PointerDragLeave">;
  onPointerDragDrop?: PointerHandler<"PointerDragDrop">;
  onPointerScroll?: PointerHandler<"PointerScroll">;
}

/** `Activate`: a `<button>` was clicked, or pressed Enter or Space while focused. */
export interface ActivateEvent {
  readonly kind: "Activate";
  readonly entity: Entity;
  readonly commands: Commands;
  readonly frame: number;
}

/** `FocusGained`: this element took `InputFocus`; `cause` is Bevy's `FocusCause`. */
export interface FocusGainedEvent {
  readonly kind: "FocusGained";
  readonly entity: Entity;
  readonly cause: "navigated" | "pressed" | "auto";
  readonly commands: Commands;
  readonly frame: number;
}

/** `FocusLost`: this element gave up `InputFocus`. */
export interface FocusLostEvent {
  readonly kind: "FocusLost";
  readonly entity: Entity;
  readonly commands: Commands;
  readonly frame: number;
}

/** `MenuEvent`: a menu's button, popup or item asks its owner to open or close it. */
export interface MenuEventEvent {
  readonly kind: "MenuEvent";
  /** The owner: the element whose `onMenuEvent` took it. */
  readonly entity: Entity;
  /** The menu button, popup or item that raised it. */
  readonly source: Entity;
  readonly action: "toggle" | "open-first" | "open-last" | "close-all" | "focus-root";
  readonly commands: Commands;
  readonly frame: number;
}

/** `RequestClose`: a modal's barrier was clicked, or Escape pressed in it. */
export interface RequestCloseEvent {
  readonly kind: "RequestClose";
  readonly entity: Entity;
  /** The `<modalDialogBarrier>` that raised it. */
  readonly source: Entity;
  readonly commands: Commands;
  readonly frame: number;
}

/** `<button>`: a `<node>` with Bevy's `Button`. */
export interface ButtonProps extends NodeProps {
  /** Entity-local observer of Bevy's `Activate`, raised on click and on Enter or Space while focused; not while `disabled`. */
  onActivate?: (event: ActivateEvent) => void;
  /** `ActivateOnPress`: activate on press rather than on release. */
  activateOnPress?: Bindable<boolean>;
}

/** A length: a number of logical pixels, or `px(8)`, `percent(50)`, `"auto"`, `"2rem"`... */
export type Length = number | string;
/** Sides: one length, CSS shorthand (`[y, x]`, `[top, right, bottom, left]`) or an object. */
export type Sides = Length | readonly Length[] | { top?: Length; right?: Length; bottom?: Length; left?: Length };

/** A grid track (Bevy's `GridTrack`): a length, `fr(n)`, `"auto"`, `"min-content"`, `"max-content"`, `minmax(min, max)` or `{ fitContent }`. */
export type Track = Length | `${number}fr` | "min-content" | "max-content" | { readonly minmax: readonly [unknown, unknown] } | { readonly fitContent: Length };
/** `repeat(count, tracks)`, Bevy's `RepeatedGridTrack`. */
export interface RepeatedTrack {
  readonly repeat: number | "auto-fill" | "auto-fit";
  readonly tracks: readonly Track[];
}
/** A grid line placement (Bevy's `GridPlacement`): a start line, `span(n)`, `{ start, end, span }` or `"auto"`. */
export type Placement = number | "auto" | { readonly start?: number; readonly end?: number; readonly span?: number };
/** A colour stop: a colour, placed evenly, or `[colour, position]`. */
export type ColorStop = ColorValue | readonly [ColorValue, Length];
/** What `linearGradient`, `radialGradient` and `conicGradient` return. */
export interface Gradient {
  readonly gradient: "linear" | "radial" | "conic";
}
type OverflowAxis = "visible" | "clip" | "hidden" | "scroll";

/** `Node`'s fields, CSS-cased. */
export interface Style {
  width?: Length;
  height?: Length;
  minWidth?: Length;
  minHeight?: Length;
  maxWidth?: Length;
  maxHeight?: Length;
  left?: Length;
  right?: Length;
  top?: Length;
  bottom?: Length;
  padding?: Sides;
  margin?: Sides;
  border?: Sides;
  gap?: Length;
  rowGap?: Length;
  columnGap?: Length;
  flexBasis?: Length;
  flexGrow?: number;
  flexShrink?: number;
  aspectRatio?: number;
  position?: "absolute" | "relative";
  display?: "flex" | "grid" | "block" | "none";
  flexDirection?: "row" | "column" | "row-reverse" | "column-reverse";
  flexWrap?: "nowrap" | "wrap" | "wrap-reverse";
  justifyContent?: "start" | "end" | "flex-start" | "flex-end" | "center" | "stretch" | "space-between" | "space-around" | "space-evenly";
  alignItems?: "start" | "end" | "flex-start" | "flex-end" | "center" | "baseline" | "stretch";
  alignSelf?: "start" | "end" | "flex-start" | "flex-end" | "center" | "baseline" | "stretch";
  /** One word for both axes, or each axis. */
  overflow?: OverflowAxis | { x?: OverflowAxis; y?: OverflowAxis };
  boxSizing?: "border-box" | "content-box";
  justifyItems?: "default" | "start" | "start-safe" | "end" | "end-safe" | "center" | "center-safe" | "baseline" | "stretch";
  justifySelf?: "auto" | "start" | "start-safe" | "end" | "end-safe" | "center" | "center-safe" | "baseline" | "stretch";
  alignContent?:
    | "default" | "start" | "start-safe" | "end" | "end-safe" | "flex-start" | "flex-start-safe" | "flex-end" | "flex-end-safe"
    | "center" | "center-safe" | "stretch" | "space-between" | "space-evenly" | "space-around";
  direction?: "ltr" | "rtl";
  scrollbarWidth?: number;
  overflowClipMargin?: { visualBox?: "content-box" | "padding-box" | "border-box"; margin?: number };
  gridAutoFlow?: "row" | "column" | "row-dense" | "column-dense";
  gridTemplateRows?: Track | RepeatedTrack | readonly (Track | RepeatedTrack)[];
  gridTemplateColumns?: Track | RepeatedTrack | readonly (Track | RepeatedTrack)[];
  gridAutoRows?: Track | readonly Track[];
  gridAutoColumns?: Track | readonly Track[];
  gridRow?: Placement;
  gridColumn?: Placement;
}

/** Four corners, `[topLeft, topRight, bottomRight, bottomLeft]` or by name. */
export type Corners = Length | readonly [Length, Length, Length, Length] | { topLeft?: Length; topRight?: Length; bottomRight?: Length; bottomLeft?: Length };
/** `BoxShadow`'s `ShadowStyle`; a missing field is Bevy's default. */
export interface Shadow {
  color?: ColorValue;
  x?: Length;
  y?: Length;
  spread?: Length;
  blur?: Length;
}
/** `UiTransform`: `rotation` in radians (`deg(45)`), clockwise. */
export interface UiTransform {
  translation?: readonly [Length, Length];
  scale?: number | readonly [number, number];
  rotation?: number;
}

/** `<node>`, `<button>`: a bevy_ui `Node`; children are UI elements and components. */
export interface NodeProps extends EntityProps {
  style?: Bindable<Style>;
  /** `BackgroundColor` (`"#rrggbb"` or `"#rrggbbaa"`), or `BackgroundGradient` from a gradient helper. */
  background?: Bindable<ColorValue | Gradient | readonly Gradient[]>;
  /** `BorderColor` (one colour or per side), or `BorderGradient`; widths are `style.border`. */
  border?: Bindable<ColorValue | { top?: ColorValue; right?: ColorValue; bottom?: ColorValue; left?: ColorValue } | Gradient | readonly Gradient[]>;
  /** `Node.border_radius`: one length, or per corner. */
  radius?: Bindable<Corners>;
  /** `Outline`, drawn outside the border; a missing field is Bevy's default (1 px, no offset, white). */
  outline?: Bindable<{ width?: Length; offset?: Length; color?: ColorValue }>;
  /** `BoxShadow`: one shadow or several. */
  shadow?: Bindable<Shadow | readonly Shadow[]>;
  /** `UiTransform`: moves, scales and rotates the node after layout. */
  transform?: Bindable<UiTransform>;
  /** `UiTargetCamera`: the camera a root UI element draws on, written once the ref mounts. Root elements only. */
  camera?: NodeRef | Entity;
  /** `InteractionDisabled` while true: Bevy's widgets ignore pointer and keyboard input. */
  disabled?: Bindable<boolean>;
  /** `TabIndex`: Tab visits it in this order (`0` in tree order, negative never); needs `"tabNavigation": true`. */
  tabIndex?: Bindable<number>;
  /** `TabGroup`: Tab cycles inside a `modal` group; groups are visited by `order`. Needs `"tabNavigation": true`. */
  tabGroup?: Bindable<{ order?: number; modal?: boolean }>;
  /** `AutoFocus`: take focus when it mounts. */
  autoFocus?: Bindable<boolean>;
  /** Entity-local observer of Bevy's `FocusGained`: this element's own, not a descendant's. */
  onFocusGained?: (event: FocusGainedEvent) => void;
  /** Entity-local observer of Bevy's `FocusLost`. */
  onFocusLost?: (event: FocusLostEvent) => void;
  /** Bevy's `MenuEvent` from this element or a descendant; the nearest `onMenuEvent` takes it and stops it. */
  onMenuEvent?: (event: MenuEventEvent) => void;
  /** Bevy's `RequestClose` from this element or a descendant (a `<modalDialogBarrier>`); the nearest `onRequestClose` takes it and stops it. */
  onRequestClose?: (event: RequestCloseEvent) => void;
}

/** The text props Bevy applies per span (`TextFont`, `TextColor`, decorations). */
export interface SpanStyle {
  /** `TextFont.font_size`, in pixels. */
  size?: Bindable<number>;
  /** `TextColor`. */
  color?: Bindable<ColorValue>;
  /** `TextFont.font`: a font asset path (`"fonts/Inter.ttf"`). */
  font?: Bindable<string>;
  /** `TextFont.weight`: 1 to 1000, 400 normal, 700 bold. */
  weight?: Bindable<number>;
  /** `TextFont.style` italic. */
  italic?: Bindable<boolean>;
  /** `LineHeight`: a multiple of the font size, or `px(n)`. */
  lineHeight?: Bindable<number | string>;
  /** `LetterSpacing`: pixels, `px(n)` or `rem(n)`. */
  letterSpacing?: Bindable<number | string>;
  /** `TextBackgroundColor`. */
  textBackground?: Bindable<ColorValue>;
  /** `Underline`, with `UnderlineColor` when a colour. */
  underline?: Bindable<boolean | ColorValue>;
  /** `Strikethrough`, with `StrikethroughColor` when a colour. */
  strikethrough?: Bindable<boolean | ColorValue>;
}

/** `<text>`'s and `<span>`'s children: strings, bindings and `<span>`s. */
export type TextChild = string | number | (() => string | number) | Element | null | undefined | false;

/**
 * `<text>`: `Text` with `TextFont`, `TextColor` and `TextLayout`. Text before the first `<span>`
 * is its own; each `<span>` (and text after one) is a child span that takes the font and colour
 * props it does not set.
 */
export interface TextProps extends Omit<NodeProps, "children">, SpanStyle {
  /** `TextLayout.justify`. */
  justify?: Bindable<"left" | "center" | "right" | "justified" | "start" | "end">;
  /** `TextLayout.linebreak`. */
  linebreak?: Bindable<"word" | "char" | "word-or-char" | "none">;
  /** `TextShadow`; a missing field is Bevy's default. */
  textShadow?: Bindable<{ x?: number; y?: number; color?: ColorValue }>;
  /**
   * bevy-ts's `TextHalo`: the glyphs, spans included, drawn again in `color` (default black)
   * `width` logical pixels around them (default 1, more than 0 and at most 4), under the text.
   */
  textHalo?: Bindable<{ width?: number; color?: ColorValue }>;
  children?: TextChild | readonly TextChild[];
}

/** `<span>`: a `TextSpan` inside a `<text>` or `<span>`, with its own text props. */
export interface SpanProps extends SpanStyle {
  children?: TextChild | readonly TextChild[];
}

/**
 * `<text2d>`: Bevy's `Text2d`, a component of its `<entity>`, placed by its `Transform` in pixels
 * under a `Camera2d`. It takes `<text>`'s text props and children; its `<span>`s are child
 * entities. docs/2d.md.
 */
export interface Text2dProps extends SpanStyle {
  /** `TextLayout.justify`. */
  justify?: Bindable<"left" | "center" | "right" | "justified" | "start" | "end">;
  /** `TextLayout.linebreak`. */
  linebreak?: Bindable<"word" | "char" | "word-or-char" | "none">;
  /** `Text2dShadow`, y up; a missing field is Bevy's default (`x` 4, `y` -4, black). */
  textShadow?: Bindable<{ x?: number; y?: number; color?: ColorValue }>;
  /** `Anchor`: the point of the text's box at the entity's origin (default `"center"`). */
  anchor?: Bindable<WorldTextAnchor>;
  /** `TextBounds`: `[width, height]` in pixels to wrap and clip at; `null` is unbounded. */
  bounds?: Bindable<readonly [number | null, number | null]>;
  children?: TextChild | readonly TextChild[];
}

/**
 * `<bitmapText>`: text in a bitmap font (an XNA SpriteFont exported as JSON beside its PNG), a
 * component of its `<entity>`, one sprite per glyph laid out as MonoGame does, placed by its
 * `Transform` in pixels under a `Camera2d`. Its children are strings, numbers and bindings, joined.
 * docs/2d.md.
 */
export interface BitmapTextProps {
  /** The font's JSON, an asset path (`"fonts/tiny.json"`, `"stardew://Fonts/SmallFont.json"`). */
  font: Bindable<string>;
  /** Tints every glyph; default white. */
  color?: Bindable<ColorValue>;
  /** Screen pixels per font pixel at zoom 1; default 1. */
  scale?: Bindable<number>;
  /** `Anchor`: the point of the measured box at the entity's origin (default `"center"`). */
  anchor?: Bindable<WorldTextAnchor>;
  children?: WorldTextChild | readonly WorldTextChild[];
}

/** `<worldText>`'s children: strings, numbers and bindings, no element. */
export type WorldTextChild = string | number | (() => string | number) | null | undefined | false;

/** Where a label's box sits at its entity's origin: a side, a corner, or `[x, y]` from -0.5 to 0.5. */
export type WorldTextAnchor =
  | "center" | "top" | "bottom" | "left" | "right" | "top-left" | "top-right" | "bottom-left" | "bottom-right"
  | readonly [number, number];

/**
 * `<worldText>`: text in the 3D world (`WorldText`, `WorldTextStyle`), a component of its
 * `<entity>`. It lies in the entity's XY plane facing +Z, placed by its `Transform`; unlit,
 * culled from behind, no shadow. docs/world-text.md.
 */
export interface WorldTextProps {
  /** The em size in world units (default 1). */
  size?: Bindable<number>;
  /** The fill colour (default white). */
  color?: Bindable<ColorValue>;
  /** `TextFont.font`: a font asset path (`"fonts/Inter.ttf"`). */
  font?: Bindable<string>;
  /** `TextFont.weight`: 1 to 1000, 400 normal, 700 bold. */
  weight?: Bindable<number>;
  /** `TextFont.style` italic. */
  italic?: Bindable<boolean>;
  /** `TextLayout.justify`. */
  justify?: Bindable<"left" | "center" | "right" | "justified" | "start" | "end">;
  /** `TextLayout.linebreak`. */
  linebreak?: Bindable<"word" | "char" | "word-or-char" | "none">;
  /** A multiple of `size` (default 1.2). */
  lineHeight?: Bindable<number>;
  /** Extra space between letters, a fraction of `size` (default 0). */
  letterSpacing?: Bindable<number>;
  /** The wrap width in world units (default: no wrap). */
  width?: Bindable<number>;
  /** The point of the layout box at the entity's origin (default `"center"`). */
  anchor?: Bindable<WorldTextAnchor>;
  /** A band around every glyph: `width` a fraction of `size`, 0 to 0.125; `color` default black. */
  outline?: Bindable<{ width: number; color?: ColorValue }>;
  /** Shaded by the scene's lights (default false: unlit). */
  lit?: Bindable<boolean>;
  children?: WorldTextChild | readonly WorldTextChild[];
}

/** Bevy's reflected `SliceScaleMode`: `"Stretch"` or `{ Tile: { stretch_value } }`. */
export type SliceScaleMode = "Stretch" | { readonly Tile: { readonly stretch_value: number } };

/** What `sliced(...)` and `tiled(...)` return: Bevy's `NodeImageMode::Sliced` and `Tiled`, reflected. */
export type ImageMode =
  | {
      readonly Sliced: {
        /** Pixels: `min_inset` is `[left, top]`, `max_inset` `[right, bottom]`. */
        readonly border: { readonly min_inset: readonly [number, number]; readonly max_inset: readonly [number, number] };
        readonly center_scale_mode: SliceScaleMode;
        readonly sides_scale_mode: SliceScaleMode;
        readonly max_corner_scale: number;
      };
    }
  | { readonly Tiled: { readonly tile_x: boolean; readonly tile_y: boolean; readonly stretch_value: number } };

/** `<image>`: an `ImageNode` from an asset path. */
export interface ImageProps extends NodeProps {
  src: Bindable<string>;
  /** `ImageNode.color`, a tint. */
  color?: Bindable<ColorValue>;
  flipX?: Bindable<boolean>;
  flipY?: Bindable<boolean>;
  /** The part of the image to draw, `[x, y, width, height]` in pixels. */
  rect?: Bindable<readonly [number, number, number, number]>;
  /** A tile of an atlas: `layout` is `TextureAtlasLayout.fromGrid({...})`, shared with sprites given an equal grid. */
  atlas?: Bindable<{
    layout: string | AssetRef<TextureAtlasLayout> | AssetValue<TextureAtlasLayout>;
    index: number;
  }>;
  /** `NodeImageMode`: `"auto"`, `"stretch"`, `sliced(...)` or `tiled(...)` (or what they return, written out). */
  mode?: Bindable<"auto" | "stretch" | ImageMode>;
}

/**
 * `<viewport camera={ref}>`: a `<node>` with `ViewportNode`, showing the camera's output. A camera
 * on the window gets an image of its own to render into, so it leaves the window.
 */
export interface ViewportProps extends Omit<NodeProps, "camera"> {
  camera: NodeRef | Entity;
}

/** `<input>`: a `Node` with Bevy's one-line `TextInput`. Its text is Bevy's editor's; to clear
 * it, remount with a new `key`. */
export interface InputProps extends Omit<NodeProps, "children"> {
  size?: Bindable<number>;
  color?: Bindable<string>;
  /** The text at mount; a reload does not reset it. */
  initialValue?: string;
  maxCharacters?: Bindable<number>;
  /** `TabIndex`; a field without one has `TabIndex(0)`, so Tab visits it inside a `tabGroup`. */
  tabIndex?: Bindable<number>;
  onTextEditChange?: (event: TextEditChangeEvent) => void;
  onKeyboardInput?: (event: KeyboardInputEvent) => void;
  onSubmit?: (event: SubmitEvent) => void;
  onCancel?: (event: CancelEvent) => void;
  value?: never;
}

/** `onValueChange`: Bevy's `ValueChange<T>` on this widget; `isFinal` is false while a slider is dragged. */
export interface ValueChangeEvent<T> {
  readonly kind: "ValueChange";
  readonly entity: Entity;
  readonly value: T;
  readonly isFinal: boolean;
  readonly commands: Commands;
  readonly frame: number;
}

/** `onValueChange` choosing a child: `value` its entity, `key` its TSX key (`null` for one TSX did not make). */
export interface ChoiceChangeEvent<T extends Entity | null> extends ValueChangeEvent<T> {
  readonly key: string | null;
}

/**
 * A widget's state: controlled (`checked`, the game's, written from its binding; the widget only
 * raises `onValueChange`) or initial (`initialChecked`, written at mount; the widget keeps its own).
 * Never both.
 */
type StateProp<S extends string, T> =
  | ({ [K in S]?: Bindable<T> } & { [K in `initial${Capitalize<S>}`]?: never })
  | ({ [K in S]?: never } & { [K in `initial${Capitalize<S>}`]?: T });

/** `<node>`: a bevy_ui `Node`. */
export interface PlainNodeProps extends NodeProps {
  /** `ScrollArea`: the wheel scrolls it; give it `style.overflow` `"scroll"` on the axis. */
  scrollArea?: Bindable<boolean>;
}

/** `<checkbox>` and `<radio>`: Bevy's `Checkbox` or `RadioButton`; `Checked` while checked. */
export type CheckboxProps = NodeProps & StateProp<"checked", boolean> & {
  onValueChange?: (event: ValueChangeEvent<boolean>) => void;
};

/** `<radioGroup>`: Bevy's `RadioGroup`. Its radios are uncontrolled unless they have `checked`. */
export interface RadioGroupProps extends NodeProps {
  onValueChange?: (event: ChoiceChangeEvent<Entity>) => void;
}

/** `<slider>`: Bevy's `Slider` with `SliderValue`, `SliderRange` (`min` to `max`, 0 to 1), `SliderStep` and `SliderPrecision`. */
export type SliderProps = NodeProps & StateProp<"value", number> & {
  min?: Bindable<number>;
  max?: Bindable<number>;
  /** Arrow keys move it this much (default 1). */
  step?: Bindable<number>;
  /** Decimal places a drag rounds to. */
  precision?: Bindable<number>;
  orientation?: Bindable<"auto" | "horizontal" | "vertical">;
  /** A click on the track: start a drag there, step towards it, or snap to it. */
  trackClick?: Bindable<"drag" | "step" | "snap">;
  onValueChange?: (event: ValueChangeEvent<number>) => void;
};

/** `<scrollbar target={ref}>`: Bevy's `Scrollbar`, scrolling the target's `ScrollPosition`. */
export interface ScrollbarProps extends NodeProps {
  target: NodeRef | Entity;
  orientation?: Bindable<"horizontal" | "vertical">;
  /** The thumb's shortest length, in pixels (default 8). */
  minThumbLength?: Bindable<number>;
}

/** `<scrollbarThumb>`: Bevy's `ScrollbarThumb`, with no `Node`: its scrollbar places and sizes it. */
export interface ScrollbarThumbProps extends EntityProps {
  background?: Bindable<ColorValue>;
  border?: Bindable<ColorValue>;
  radius?: Bindable<Corners>;
  /** Only `border`, its widths. */
  style?: Bindable<{ border?: Sides }>;
}

/** `<tabList>`: Bevy's `TabList`; `selected` is a `<tab>`'s key, or `null` for none. */
export type TabListProps = NodeProps & StateProp<"selected", string | null> & {
  orientation?: Bindable<"horizontal" | "vertical">;
  /** Arrow keys select the tab they move to (`"automatic"`), or only focus it (`"manual"`). */
  activation?: Bindable<"manual" | "automatic">;
  onValueChange?: (event: ChoiceChangeEvent<Entity | null>) => void;
};

/** `<listBox>`: Bevy's `ListBox`. Its items' `selected` is the game's: Bevy changes none itself. */
export interface ListBoxProps extends NodeProps {
  multiSelect?: Bindable<boolean>;
  onValueChange?: (event: ChoiceChangeEvent<Entity>) => void;
}

/** `<listItem>`: Bevy's `ListItem`; `Selected` while `selected`. */
export interface ListItemProps extends NodeProps {
  selected?: Bindable<boolean>;
}

/** `<menuItem>`: Bevy's `MenuItem`, with `TabIndex(0)` unless `tabIndex` says otherwise, so the menu can focus it. */
export interface MenuItemProps extends NodeProps {
  /** Entity-local observer of Bevy's `Activate`: a click, or Enter or Space while focused. */
  onActivate?: (event: ActivateEvent) => void;
}

/** `<menuPopup>`: Bevy's `MenuPopup`. It opens with focus on its first item and closes when focus leaves it. */
export interface MenuPopupProps extends NodeProps {
  /** Which arrows move between items: up and down (`"column"`), left and right (`"row"`), or none (`"grid"`). */
  layout?: Bindable<"column" | "row" | "grid">;
}

/** One place a `<popover>` may go against its parent: Bevy's `PopoverPlacement`. */
export interface PopoverPlacement {
  side?: "top" | "bottom" | "left" | "right";
  align?: "start" | "center" | "end";
  /** Logical pixels between the parent and the popover. */
  gap?: number;
}

/** `<popover>`: Bevy's `Popover`, placed against its parent at the first of `positions` that fits the window. */
export interface PopoverProps extends NodeProps {
  positions?: Bindable<PopoverPlacement[]>;
  /** How near the window's edge it may go, in logical pixels. */
  windowMargin?: Bindable<number>;
}

/** `<dialog>`: Bevy's `Dialog`, a tab group; `modal` adds `ModalDialog`, which traps focus. */
export interface DialogProps extends NodeProps {
  modal?: Bindable<boolean>;
}

type Vec2 = [number, number];

// `<mesh shape>`: Bevy 0.20's mesh primitives (docs/meshes.md). Props are the Bevy constructor's
// parameters and mesh builder's settings, in camelCase; an unset prop is Bevy's default. Angles
// are radians.

export interface CuboidMesh {
  shape: "cuboid";
  /** Full lengths `[x, y, z]`. Default `[1, 1, 1]`. */
  size?: Vec3;
}
export interface SphereMesh {
  shape: "sphere";
  /** Default 0.5. */
  radius?: number;
  /** Default `"ico"`. */
  kind?: "ico" | "uv";
  /** An ico sphere's subdivisions, at most 79. Default 5. */
  subdivisions?: number;
  /** A uv sphere's sectors (around), at least 3. Default 32. */
  sectors?: number;
  /** A uv sphere's stacks (pole to pole), at least 2. Default 18. */
  stacks?: number;
}
export interface PlaneMesh {
  shape: "plane";
  /** Full lengths `[x, z]`. Default `[1, 1]`. */
  size?: Vec2;
  /** Default `[0, 1, 0]`. */
  normal?: Vec3;
  /** Cuts along both axes. Default 0. */
  subdivisions?: number;
  /** Default 0. */
  subdivisionsX?: number;
  /** Default 0. */
  subdivisionsZ?: number;
}
export interface CylinderMesh {
  shape: "cylinder";
  /** Default 0.5. */
  radius?: number;
  /** Default 1. */
  height?: number;
  /** Vertices around, at least 3. Default 32. */
  resolution?: number;
  /** Segments along the height. Default 1. */
  segments?: number;
  /** Default true. */
  caps?: boolean;
  /** Default `"midPoint"`. */
  anchor?: "midPoint" | "top" | "bottom";
}
export interface Capsule3dMesh {
  shape: "capsule3d";
  /** Default 0.5. */
  radius?: number;
  /** The cylinder's length, between the hemispheres' centres. Default 1. */
  length?: number;
  /** Rings on the cylinder. Default 0. */
  rings?: number;
  /** At least 3. Default 32. */
  longitudes?: number;
  /** Even, at least 4. Default 16. */
  latitudes?: number;
  /** Default `"aspect"`. */
  uvProfile?: "aspect" | "uniform" | "fixed";
}
export interface ConeMesh {
  shape: "cone";
  /** Default 0.5. */
  radius?: number;
  /** Default 1. */
  height?: number;
  /** At least 3. Default 32. */
  resolution?: number;
  /** Default `"midPoint"`. */
  anchor?: "midPoint" | "tip" | "base";
}
export interface ConicalFrustumMesh {
  shape: "conicalFrustum";
  /** Default 0.25. */
  radiusTop?: number;
  /** Default 0.5. */
  radiusBottom?: number;
  /** Default 0.5. */
  height?: number;
  /** At least 3. Default 32. */
  resolution?: number;
  /** Default 1. */
  segments?: number;
}
export interface TorusMesh {
  shape: "torus";
  /** Below `outerRadius`. Default 0.5. */
  innerRadius?: number;
  /** Default 1. */
  outerRadius?: number;
  /** Around the tube, at least 3. Default 24. */
  minorResolution?: number;
  /** Around the ring, at least 3. Default 32. */
  majorResolution?: number;
  /** `[start, end]`. Default `[0, 2π]`. */
  angleRange?: Vec2;
}
export interface TetrahedronMesh {
  shape: "tetrahedron";
  /** Default Bevy's regular tetrahedron in the unit cube. */
  vertices?: [Vec3, Vec3, Vec3, Vec3];
}
export interface Triangle3dMesh {
  shape: "triangle3d";
  /** Default `[[0, 0.5, 0], [-0.5, -0.5, 0], [0.5, -0.5, 0]]`. */
  vertices?: [Vec3, Vec3, Vec3];
}

/** A 2D shape lies in the XY plane facing +Z; with `depth` it is Bevy's extrusion along Z. */
export interface Extrude {
  /** Full depth along Z. Unset: flat. */
  depth?: number;
  /** Segments along the depth; needs `depth`. Default 1. */
  segments?: number;
}
export interface CircleMesh extends Extrude {
  shape: "circle";
  /** Default 0.5. */
  radius?: number;
  /** At least 3. Default 32. */
  resolution?: number;
}
export interface CircularSectorMesh extends Extrude {
  shape: "circularSector";
  /** Default 0.5. */
  radius?: number;
  /** The whole angle, up to 2π. Default 4π/3. */
  angle?: number;
  /** At least 3. Default 32. */
  resolution?: number;
  /** Rotates the UV mask. Default 0. */
  uvAngle?: number;
}
export interface CircularSegmentMesh extends Extrude {
  shape: "circularSegment";
  /** Default 0.5. */
  radius?: number;
  /** Half the angle, up to π. Default 2π/3. */
  halfAngle?: number;
  /** At least 3. Default 32. */
  resolution?: number;
  /** Rotates the UV mask. Default 0. */
  uvAngle?: number;
}
export interface EllipseMesh extends Extrude {
  shape: "ellipse";
  /** Half lengths `[x, y]`. Default `[1, 0.5]`. */
  halfSize?: Vec2;
  /** At least 3. Default 32. */
  resolution?: number;
}
export interface AnnulusMesh extends Extrude {
  shape: "annulus";
  /** Below `outerRadius`. Default 0.5. */
  innerRadius?: number;
  /** Default 1. */
  outerRadius?: number;
  /** At least 3. Default 32. */
  resolution?: number;
}
export interface RectangleMesh extends Extrude {
  shape: "rectangle";
  /** Full lengths `[x, y]`. Default `[1, 1]`. */
  size?: Vec2;
}
export interface RhombusMesh extends Extrude {
  shape: "rhombus";
  /** Default 1. */
  horizontalDiagonal?: number;
  /** Default 1. */
  verticalDiagonal?: number;
}
export interface RegularPolygonMesh extends Extrude {
  shape: "regularPolygon";
  /** Default 0.5. */
  circumradius?: number;
  /** At least 3. Default 6. */
  sides?: number;
}
export interface Triangle2dMesh extends Extrude {
  shape: "triangle2d";
  /** Either winding. Default `[[0, 0.5], [-0.5, -0.5], [0.5, -0.5]]`. */
  vertices?: [Vec2, Vec2, Vec2];
}
export interface Capsule2dMesh extends Extrude {
  shape: "capsule2d";
  /** Default 0.5. */
  radius?: number;
  /** The straight part's length. Default 1. */
  length?: number;
  /** Vertices per half circle, at least 3. Default 16. */
  resolution?: number;
}
export interface ConvexPolygonMesh extends Extrude {
  shape: "convexPolygon";
  /** At least 3 points of a convex polygon, either winding. */
  vertices: Vec2[];
}
/**
 * A mesh from vertex data, as `Mesh.of` builds one: each prop is one of Bevy's `Mesh` calls.
 * Every attribute has one entry per position. For a mesh that changes each frame, use
 * `ResMut<Assets<Mesh>>` instead: props are hashed whenever they change.
 */
export interface VertexDataMesh {
  shape?: never;
  /** `Mesh::ATTRIBUTE_POSITION`. */
  positions: Vec3[];
  /** `Mesh::ATTRIBUTE_NORMAL`, or Bevy's `compute_normals`, `compute_flat_normals`, `compute_smooth_normals` or `compute_area_weighted_normals`. */
  normals?: Vec3[] | "computed" | "flat" | "smooth" | "areaWeighted";
  /** `Mesh::ATTRIBUTE_UV_0`. */
  uvs?: Vec2[];
  /** `Mesh::ATTRIBUTE_UV_1`. */
  uvs1?: Vec2[];
  /** `Mesh::ATTRIBUTE_TANGENT`, or `"generated"`: Bevy's `generate_tangents`, after the normals. */
  tangents?: [number, number, number, number][] | "generated";
  /** `Mesh::ATTRIBUTE_COLOR`: linear RGBA. */
  colors?: [number, number, number, number][];
  /** `Mesh::ATTRIBUTE_JOINT_INDEX`. */
  jointIndices?: [number, number, number, number][];
  /** `Mesh::ATTRIBUTE_JOINT_WEIGHT`. */
  jointWeights?: [number, number, number, number][];
  /** A plain list is `Indices::U32`. */
  indices?: number[] | { U16: number[] } | { U32: number[] };
  /** Default `"TriangleList"`. */
  topology?: "PointList" | "LineList" | "LineStrip" | "TriangleList" | "TriangleStrip";
  /** Default both worlds. */
  assetUsage?: "MAIN_WORLD" | "RENDER_WORLD" | "MAIN_WORLD | RENDER_WORLD";
}

/** The shapes in the XY plane, which `<mesh2d>` also takes. */
export type Shape2dMesh =
  | CircleMesh
  | CircularSectorMesh
  | CircularSegmentMesh
  | EllipseMesh
  | AnnulusMesh
  | RectangleMesh
  | RhombusMesh
  | RegularPolygonMesh
  | Triangle2dMesh
  | Capsule2dMesh
  | ConvexPolygonMesh;

export type MeshProps = (
  | CuboidMesh
  | SphereMesh
  | PlaneMesh
  | CylinderMesh
  | Capsule3dMesh
  | ConeMesh
  | ConicalFrustumMesh
  | TorusMesh
  | TetrahedronMesh
  | Triangle3dMesh
  | Shape2dMesh
  | VertexDataMesh
) & {
  /** Shorthand for `<material color>`. */
  color?: ColorValue;
};

/** A 2D shape without its extrusion: `<mesh2d>` is flat. */
type Flat<T> = T extends unknown ? Omit<T, keyof Extrude> & { depth?: never; segments?: never } : never;

/** `<mesh2d shape>`: a `Mesh2d` of one of `<mesh>`'s 2D shapes, sharing its mesh asset. */
export type Mesh2dProps = Flat<Shape2dMesh> & {
  /** Shorthand for `<material2d color>`. */
  color?: ColorValue;
};

/** A texture's UV channel. */
export type UvChannel = "uv0" | "uv1";
/** A face to cull. */
export type Face = "back" | "front";
/** `ParallaxMappingMethod`: one sample, or a binary search of up to `maxSteps`. */
export type ParallaxMappingMethod = "occlusion" | { relief: { maxSteps: number } };
/** `OpaqueRendererMethod`. */
export type OpaqueRendererMethod = "forward" | "deferred" | "auto";
/** `Affine2::from_scale_angle_translation`; unset parts are identity. */
export interface UvTransform {
  scale?: [number, number];
  /** Radians. */
  rotation?: number;
  translation?: [number, number];
}

/**
 * A `<material>` texture: an asset path (base colour and emissive load as sRGB, the rest as linear
 * data), or an image ref, `Image.of(...)` or `Image.load(path, settings)` for a sampler.
 */
export type Texture = string | AssetRef<Image> | AssetValue<Image> | AssetLoad<Image>;

/**
 * `<material>`: a `StandardMaterial`, every field under its Bevy name in camelCase (`color` is
 * `base_color`, `roughness` is `perceptual_roughness`). Textures are [`Texture`]s.
 */
export interface MaterialProps {
  color?: ColorValue;
  baseColorChannel?: UvChannel;
  baseColorTexture?: Texture;
  emissive?: ColorValue;
  emissiveExposureWeight?: number;
  emissiveChannel?: UvChannel;
  emissiveTexture?: Texture;
  roughness?: number;
  metallic?: number;
  metallicRoughnessChannel?: UvChannel;
  metallicRoughnessTexture?: Texture;
  reflectance?: number;
  specularTint?: ColorValue;
  diffuseTransmission?: number;
  specularTransmission?: number;
  thickness?: number;
  ior?: number;
  attenuationDistance?: number;
  attenuationColor?: ColorValue;
  normalMapChannel?: UvChannel;
  /** Gives the entity's `<mesh>` vertex tangents. */
  normalMapTexture?: Texture;
  flipNormalMapY?: boolean;
  occlusionChannel?: UvChannel;
  occlusionTexture?: Texture;
  clearcoat?: number;
  clearcoatPerceptualRoughness?: number;
  /** Non-zero gives the entity's `<mesh>` vertex tangents. */
  anisotropyStrength?: number;
  /** Radians. */
  anisotropyRotation?: number;
  /** Pair with `cullMode={null}` to draw back faces. */
  doubleSided?: boolean;
  /** `null` culls nothing. */
  cullMode?: Face | null;
  unlit?: boolean;
  /** Replaces `color`'s alpha, 0 to 1. To fade over time, use `MaterialAlpha` instead. */
  alpha?: number;
  /** Unset: `"blend"` when the alpha is below 1, `"opaque"` otherwise. */
  alphaMode?: AlphaMode;
  fogEnabled?: boolean;
  depthBias?: number;
  depthMap?: Texture;
  parallaxDepthScale?: number;
  parallaxMappingMethod?: ParallaxMappingMethod;
  maxParallaxLayerCount?: number;
  lightmapExposure?: number;
  opaqueRenderMethod?: OpaqueRendererMethod;
  /** 0 to 255. */
  deferredLightingPassId?: number;
  /** A scale above 1 clamps rather than tiles: textures sample clamped to the edge. */
  uvTransform?: UvTransform;
  kind?: never;
  asset?: never;
}

/** `<material2d>`: a `ColorMaterial`. The texture is an asset path, loaded as sRGB. */
export interface Material2dProps {
  color?: ColorValue;
  /** Replaces `color`'s alpha, 0 to 1. */
  alpha?: number;
  /** `AlphaMode2d`. Unset: `"blend"` when the alpha is below 1, `"opaque"` otherwise. */
  alphaMode?: "opaque" | "blend" | { mask: number };
  texture?: string;
  uvTransform?: UvTransform;
}

/**
 * `<material kind={Glow} color="#f80" pulse={2} />`: a script material from `material()` in
 * `bevy-ts/material`, with its params by name and its textures as asset paths. Checked against
 * the kind's schema when the generation loads. On a glTF scene root it draws the scene's meshes.
 * Instance params (`hover={0}`, or bound, `hover={() => h()}`) go to the entity, not the shared
 * asset.
 */
export interface ScriptMaterialProps {
  kind: { readonly name: string };
  asset?: never;
  /**
   * On an entity with a glTF scene (`WorldAssetRoot`), only the scene's meshes whose glTF material
   * is named this take the material. Without it, every mesh of the scene does.
   */
  gltfMaterial?: string;
  [param: string]: unknown;
}

/**
 * `<material asset={g} />`: an owned script material from `Kind.add(props)`, the one asset every
 * element given `g` draws, written in place with `assets.set(g, patch)`. Takes nothing else.
 */
export interface OwnedMaterialProps {
  asset: AssetRef<KindFields<any, any>>;
  kind?: never;
  /** On a glTF scene root, only the scene's meshes whose glTF material is named this. */
  gltfMaterial?: string;
}

/**
 * `<pass kind={Dusk} amount={() => dusk()} />` on an entity with `<Camera2d />`: a fullscreen pass
 * from `pass()` in `bevy-ts/material`, run over the camera's image after the main pass. Its params
 * live on the camera: each a value or bound. One `<pass>` per kind on a camera; unset params are
 * zero, colours white. Checked against the kind's schema when the generation loads, as
 * `<material kind>` is: an intrinsic's props cannot follow the type of its `kind`.
 */
export interface PassProps extends KeyAttribute {
  kind: { readonly name: string };
  [param: string]: unknown;
}

type DescElement = Element;

/**
 * Props a component element takes beyond its columns, written through reflection: a layout-tier
 * Rust component's writable holes (`<DirectionalLight color>`), or an enum component's `value`
 * (`<Projection value>`), each a value or `() => value`. TS systems never see them.
 */
type TsxProps<C> = C extends { readonly tsxProps: infer X } ? X : {};

export declare namespace JSX {
  type Element = DescElement;
  /** What may be a tag: an intrinsic, a component handle, or a prefab. */
  type ElementType = string | (abstract new (...args: any) => any) | ((props: any) => Element | null);
  interface ElementClass {}
  interface ElementChildrenAttribute {
    children: {};
  }
  interface IntrinsicAttributes {}
  /**
   * Every tag takes `key`: identity among siblings; without it, `type#n` (n counts siblings of
   * that type). On `<For>`, `key` is the item key function instead.
   */
  type LibraryManagedAttributes<C, P> = C extends typeof For
    ? P
    : C extends { readonly __nameElement: true }
      ? { value: string } & KeyAttribute
      : C extends { readonly __stateScoped: true }
      ? { value: import("bevy-ts").StateValue } & KeyAttribute
      : C extends { readonly __element: infer E }
      ? ElementProps<E> & KeyAttribute
      : C extends { scene(props: infer Props): unknown } & (abstract new () => infer I)
      ? ElementProps<I> & Props & KeyAttribute
      : C extends abstract new () => infer I
      ? I extends Component | ReflectComponent ? ElementProps<I> & TsxProps<C> & KeyAttribute : P & KeyAttribute
      : P & KeyAttribute;
  interface IntrinsicElements {
    entity: EntityProps & KeyAttribute;
    mesh: MeshProps;
    material: MaterialProps | ScriptMaterialProps | OwnedMaterialProps;
    mesh2d: Mesh2dProps;
    material2d: Material2dProps;
    pass: PassProps;
    worldText: WorldTextProps;
    text2d: Text2dProps;
    bitmapText: BitmapTextProps;
    node: PlainNodeProps & KeyAttribute;
    button: ButtonProps & KeyAttribute;
    text: TextProps & KeyAttribute;
    span: SpanProps & KeyAttribute;
    image: ImageProps & KeyAttribute;
    viewport: ViewportProps & KeyAttribute;
    input: InputProps & KeyAttribute;
    checkbox: CheckboxProps & KeyAttribute;
    radioGroup: RadioGroupProps & KeyAttribute;
    radio: CheckboxProps & KeyAttribute;
    slider: SliderProps & KeyAttribute;
    sliderThumb: NodeProps & KeyAttribute;
    scrollbar: ScrollbarProps & KeyAttribute;
    scrollbarThumb: ScrollbarThumbProps & KeyAttribute;
    tabList: TabListProps & KeyAttribute;
    tab: NodeProps & KeyAttribute;
    listBox: ListBoxProps & KeyAttribute;
    listItem: ListItemProps & KeyAttribute;
    menuButton: NodeProps & KeyAttribute;
    menuPopup: MenuPopupProps & KeyAttribute;
    menuItem: MenuItemProps & KeyAttribute;
    popover: PopoverProps & KeyAttribute;
    dialog: DialogProps & KeyAttribute;
    modalDialogBarrier: NodeProps & KeyAttribute;
    dialogDragHandle: NodeProps & KeyAttribute;
  }
}
