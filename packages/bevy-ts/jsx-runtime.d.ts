// `bevy-ts/jsx-runtime`: the automatic JSX runtime (`"jsxImportSource": "bevy-ts"`). Elements
// are plain descriptions: an `<entity>`, component elements on it (`<Health initialHp={100} />`),
// prefabs (functions from props to one element) and the asset intrinsics `<mesh>`, `<material>`.
import type { ComponentType, For, NodeRef } from "bevy-ts";

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
}

/** A length: a number of logical pixels, or `px(8)`, `percent(50)`, `"auto"`, `"2rem"`... */
export type Length = number | string;
/** Sides: one length, CSS shorthand (`[y, x]`, `[top, right, bottom, left]`) or an object. */
export type Sides = Length | readonly Length[] | { top?: Length; right?: Length; bottom?: Length; left?: Length };

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
  overflow?: "visible" | "clip" | "hidden" | "scroll";
}

/** `<node>`, `<button>`: a bevy_ui `Node`; children are UI elements and components. */
export interface NodeProps extends EntityProps {
  style?: Bindable<Style>;
  /** `BackgroundColor`, `"#rrggbb"` or `"#rrggbbaa"`. */
  background?: Bindable<string>;
  /** `BorderColor`; widths are `style.border`. */
  border?: Bindable<string>;
  /** `Node.border_radius`, every corner. */
  radius?: Bindable<Length>;
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
}

type DescElement = Element;

export declare namespace JSX {
  type Element = DescElement;
  /** What may be a tag: an intrinsic, a component handle, or a prefab. */
  type ElementType = string | ComponentType<any> | ((props: any) => Element | null);
  interface ElementChildrenAttribute {
    children: {};
  }
  interface IntrinsicAttributes {}
  /**
   * Every tag takes `key`: identity among siblings; without it, `type#n` (n counts siblings of
   * that type). On `<For>`, `key` is the item key function instead.
   */
  type LibraryManagedAttributes<C, P> = C extends typeof For ? P : P & KeyAttribute;
  interface IntrinsicElements {
    entity: EntityProps & KeyAttribute;
    mesh: MeshProps;
    material: MaterialProps;
    node: NodeProps & KeyAttribute;
    button: NodeProps & KeyAttribute;
    text: TextProps & KeyAttribute;
    image: ImageProps & KeyAttribute;
  }
}
