// TSX types for the ruled surface. Types only.
import type { Child, Commands, Element, Entity, NodeRef, ReadonlyVec3, Vec3, Quat } from "./mod.ts";

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

type PropValue<T> =
  T extends Vec3 ? readonly [number, number, number] | ReadonlyVec3
  : T extends Quat ? readonly [number, number, number, number]
  : T;
type Fields<T> = { [K in keyof T as K extends symbol ? never : K]: T[K] };
/** A component tag's props: each field controlled (`hp`), initial (`initialHp`) or bound (`hp={() => ...}`). */
type ComponentProps<T> =
  & { [K in keyof Fields<T>]?: PropValue<Fields<T>[K]> | (() => PropValue<Fields<T>[K]>) }
  & { [K in keyof Fields<T> & string as `initial${Capitalize<K>}`]?: PropValue<Fields<T>[K]> };

export declare namespace JSX {
  type Element = import("./mod.ts").Element;
  interface ElementClass {}
  interface ElementAttributesProperty {}
  interface ElementChildrenAttribute { children: {} }
  interface IntrinsicAttributes { key?: string }
  /** A component class used as a tag takes its fields as props. */
  type LibraryManagedAttributes<C, P> = C extends abstract new () => infer I extends import("./mod.ts").Component | import("./mod.ts").ReflectComponent ? ComponentProps<I> : P;
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
