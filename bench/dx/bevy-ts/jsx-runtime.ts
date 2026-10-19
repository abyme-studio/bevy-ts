// TSX types for the ruled surface. Types only.
import type { Child, Commands, Element, Entity, Field, NodeRef, ReadonlyVec3, Schema } from "./mod.ts";

type Handler = (ctx: { entity: Entity; commands: Commands }) => void;
type Bindable<T> = T | (() => T);

interface EntityProps {
  key?: string;
  name?: string;
  ref?: NodeRef;
  children?: Child;
  // Entity-local observers (Bevy 0.20 `on(...)` in BSN).
  onPointerClick?: Handler;
  onPointerOver?: Handler;
  onPointerOut?: Handler;
}
interface NodeProps extends EntityProps {
  style?: Record<string, string | number>;
  background?: Bindable<string>;
}
interface TextProps extends EntityProps {
  size?: Bindable<number>;
  color?: Bindable<string>;
}
interface MeshProps {
  shape: "cuboid" | "sphere" | "plane";
  size?: readonly [number, number, number];
  color?: Bindable<string>;
}

type PropValue<F> =
  F extends Field<"vec2" | "vec3"> ? readonly [number, number, number] | ReadonlyVec3
  : F extends Field<"bool"> ? boolean
  : F extends Field<"enum", infer E> ? E
  : F extends Field<"entity"> ? Entity
  : number;
/** A component tag's props: each field controlled (`hp`), initial (`initialHp`) or bound (`hp={() => ...}`). */
type ComponentProps<S> =
  & { [K in keyof S]?: PropValue<S[K]> | (() => PropValue<S[K]>) }
  & { [K in keyof S & string as `initial${Capitalize<K>}`]?: PropValue<S[K]> };

export declare namespace JSX {
  type Element = import("./mod.ts").Element;
  interface ElementClass {}
  interface ElementAttributesProperty {}
  interface ElementChildrenAttribute { children: {} }
  interface IntrinsicAttributes { key?: string }
  /** A component class used as a tag takes its fields as props. */
  type LibraryManagedAttributes<C, P> = C extends { readonly schema: infer S extends Schema } ? ComponentProps<S> : P;
  interface IntrinsicElements {
    entity: EntityProps;
    node: NodeProps;
    text: TextProps;
    button: NodeProps;
    mesh: MeshProps;
  }
}

export declare function jsx(type: unknown, props: unknown, key?: string): Element;
export declare function jsxs(type: unknown, props: unknown, key?: string): Element;
export declare const Fragment: unique symbol;
