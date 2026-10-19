// TSX types for the proposed surface. Types only.
import type { Child, Commands, Element, Entity, NodeRef, Ref, Schema } from "./mod.ts";

type Handler<S extends Schema = {}> = (event: Ref<S>, ctx: { entity: Entity; commands: Commands }) => void;
type Bindable<T> = T | (() => T);

interface EntityProps {
  key?: string;
  name?: string;
  ref?: NodeRef;
  children?: Child;
  // Entity-local observers (Bevy 0.20 `on(...)` in BSN, `observe` on an entity).
  onPointerClick?: Handler<{ x: Field32; y: Field32 }>;
  onPointerOver?: Handler;
  onPointerOut?: Handler;
}
type Field32 = import("./mod.ts").Field<"f32">;

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

export declare namespace JSX {
  type Element = import("./mod.ts").Element;
  interface ElementChildrenAttribute { children: {} }
  interface IntrinsicAttributes { key?: string }
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
