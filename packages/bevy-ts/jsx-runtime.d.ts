// `bevy-ts/jsx-runtime`: the automatic JSX runtime (`"jsxImportSource": "bevy-ts"`). Elements
// are plain descriptions: an `<entity>`, component elements on it (`<Health initialHp={100} />`),
// prefabs (functions from props to one element) and the asset intrinsics `<mesh>`, `<material>`.
import type { Commands, Component, ElementProps, Entity, For, NodeRef, ReflectComponent } from "bevy-ts";

/** What a picking handler receives: the event, and the call's `commands` and `frame`. */
export interface PointerEvent {
  readonly kind: "PointerClick" | "PointerPress" | "PointerRelease" | "PointerOver" | "PointerOut";
  readonly entity: Entity;
  /** The pointer's position in its render target. */
  readonly position: readonly [number, number];
  /** For click, press and release. */
  readonly button: "primary" | "secondary" | "middle" | null;
  readonly commands: Commands;
  readonly frame: number;
}
export type PointerHandler = (event: PointerEvent) => void;

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
  /** Entity-local observers of Bevy 0.20's picking events; they do not bubble. */
  onPointerClick?: PointerHandler;
  onPointerPress?: PointerHandler;
  onPointerRelease?: PointerHandler;
  onPointerOver?: PointerHandler;
  onPointerOut?: PointerHandler;
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
export type ColorStop = string | readonly [string, Length];
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
  color?: string;
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
  background?: Bindable<string | Gradient | readonly Gradient[]>;
  /** `BorderColor` (one colour or per side), or `BorderGradient`; widths are `style.border`. */
  border?: Bindable<string | { top?: string; right?: string; bottom?: string; left?: string } | Gradient | readonly Gradient[]>;
  /** `Node.border_radius`: one length, or per corner. */
  radius?: Bindable<Corners>;
  /** `Outline`, drawn outside the border; a missing field is Bevy's default (1 px, no offset, white). */
  outline?: Bindable<{ width?: Length; offset?: Length; color?: string }>;
  /** `BoxShadow`: one shadow or several. */
  shadow?: Bindable<Shadow | readonly Shadow[]>;
  /** `UiTransform`: moves, scales and rotates the node after layout. */
  transform?: Bindable<UiTransform>;
}

/** `<text>`: `Text`, with `TextFont` size and `TextColor`. Children are strings and bindings. */
export interface TextProps extends Omit<NodeProps, "children"> {
  size?: Bindable<number>;
  color?: Bindable<string>;
  children?: TextChild | readonly TextChild[];
}
export type TextChild = string | number | (() => string | number) | null | undefined | false;

/** `<image>`: an `ImageNode` from an asset path. */
export interface ImageProps extends NodeProps {
  src: Bindable<string>;
  color?: Bindable<string>;
}

export interface MeshProps {
  shape: "cuboid" | "sphere" | "plane";
  /** `[x, y, z]` for a cuboid, `[x, z]` for a plane. */
  size?: Vec3 | [number, number];
  /** A sphere's radius. */
  radius?: number;
  /** Shorthand for `<material color>`. */
  color?: string;
}

export interface MaterialProps {
  color?: string;
  metallic?: number;
  roughness?: number;
  emissive?: string;
  unlit?: boolean;
  kind?: never;
}

/**
 * `<material kind={Glow} color="#f80" pulse={2} />`: a script material from `material()` in
 * `bevy-ts/material`, with its params by name and its textures as asset paths. Checked against
 * the kind's schema when the generation loads. On a glTF scene root it draws the scene's meshes.
 */
export interface ScriptMaterialProps {
  kind: { readonly name: string };
  /**
   * On an entity with a glTF scene (`WorldAssetRoot`), only the scene's meshes whose glTF material
   * is named this take the material. Without it, every mesh of the scene does.
   */
  gltfMaterial?: string;
  [param: string]: unknown;
}

type DescElement = Element;

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
      : C extends { readonly __element: infer E }
      ? ElementProps<E> & KeyAttribute
      : C extends { scene(props: infer Props): unknown } & (abstract new () => infer I)
      ? ElementProps<I> & Props & KeyAttribute
      : C extends abstract new () => infer I
      ? I extends Component | ReflectComponent ? ElementProps<I> & KeyAttribute : P & KeyAttribute
      : P & KeyAttribute;
  interface IntrinsicElements {
    entity: EntityProps & KeyAttribute;
    mesh: MeshProps;
    material: MaterialProps | ScriptMaterialProps;
    node: NodeProps & KeyAttribute;
    button: NodeProps & KeyAttribute;
    text: TextProps & KeyAttribute;
    image: ImageProps & KeyAttribute;
  }
}
