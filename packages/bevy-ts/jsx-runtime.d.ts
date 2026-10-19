// `bevy-ts/jsx-runtime`: the automatic JSX runtime (`"jsxImportSource": "bevy-ts"`). Elements
// are plain descriptions: an `<entity>`, component elements on it (`<Health initialHp={100} />`),
// prefabs (functions from props to one element) and the asset intrinsics `<mesh>`, `<material>`.
import type { ComponentType } from "bevy-ts";

export declare const Fragment: unique symbol;

/** A description node. Building one runs nothing. */
export interface Element {
  readonly type: unknown;
  readonly props: Record<string, unknown>;
  readonly key: string | null;
}

export type Child = Element | null | undefined | false | true | readonly Child[];

export declare function jsx(type: unknown, props: Record<string, unknown> | null, key?: string | number): Element;
export declare function jsxs(type: unknown, props: Record<string, unknown> | null, key?: string | number): Element;

type Vec3 = [number, number, number];

export interface EntityProps {
  /** The entity's `Name`. */
  name?: string;
  /** Components and child entities. */
  children?: Child;
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
  interface IntrinsicAttributes {
    /** Identity among siblings; without it, `type#n` (n counts siblings of that type). */
    key?: string | number;
  }
  interface IntrinsicElements {
    entity: EntityProps & IntrinsicAttributes;
    mesh: MeshProps;
    material: MaterialProps;
  }
}
