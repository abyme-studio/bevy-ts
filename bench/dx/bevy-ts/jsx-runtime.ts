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
// A copy of packages/bevy-ts/jsx-runtime.d.ts's MeshProps.
type V2 = readonly [number, number];
type V3 = readonly [number, number, number];
interface CuboidMesh {
  shape: "cuboid";
  size?: V3;
}
interface SphereMesh {
  shape: "sphere";
  radius?: number;
  kind?: "ico" | "uv";
  subdivisions?: number;
  sectors?: number;
  stacks?: number;
}
interface PlaneMesh {
  shape: "plane";
  size?: V2;
  normal?: V3;
  subdivisions?: number;
  subdivisionsX?: number;
  subdivisionsZ?: number;
}
interface CylinderMesh {
  shape: "cylinder";
  radius?: number;
  height?: number;
  resolution?: number;
  segments?: number;
  caps?: boolean;
  anchor?: "midPoint" | "top" | "bottom";
}
interface Capsule3dMesh {
  shape: "capsule3d";
  radius?: number;
  length?: number;
  rings?: number;
  longitudes?: number;
  latitudes?: number;
  uvProfile?: "aspect" | "uniform" | "fixed";
}
interface ConeMesh {
  shape: "cone";
  radius?: number;
  height?: number;
  resolution?: number;
  anchor?: "midPoint" | "tip" | "base";
}
interface ConicalFrustumMesh {
  shape: "conicalFrustum";
  radiusTop?: number;
  radiusBottom?: number;
  height?: number;
  resolution?: number;
  segments?: number;
}
interface TorusMesh {
  shape: "torus";
  innerRadius?: number;
  outerRadius?: number;
  minorResolution?: number;
  majorResolution?: number;
  angleRange?: V2;
}
interface TetrahedronMesh {
  shape: "tetrahedron";
  vertices?: [V3, V3, V3, V3];
}
interface Triangle3dMesh {
  shape: "triangle3d";
  vertices?: [V3, V3, V3];
}
interface Extrude {
  depth?: number;
  segments?: number;
}
interface CircleMesh extends Extrude {
  shape: "circle";
  radius?: number;
  resolution?: number;
}
interface CircularSectorMesh extends Extrude {
  shape: "circularSector";
  radius?: number;
  angle?: number;
  resolution?: number;
  uvAngle?: number;
}
interface CircularSegmentMesh extends Extrude {
  shape: "circularSegment";
  radius?: number;
  halfAngle?: number;
  resolution?: number;
  uvAngle?: number;
}
interface EllipseMesh extends Extrude {
  shape: "ellipse";
  halfSize?: V2;
  resolution?: number;
}
interface AnnulusMesh extends Extrude {
  shape: "annulus";
  innerRadius?: number;
  outerRadius?: number;
  resolution?: number;
}
interface RectangleMesh extends Extrude {
  shape: "rectangle";
  size?: V2;
}
interface RhombusMesh extends Extrude {
  shape: "rhombus";
  horizontalDiagonal?: number;
  verticalDiagonal?: number;
}
interface RegularPolygonMesh extends Extrude {
  shape: "regularPolygon";
  circumradius?: number;
  sides?: number;
}
interface Triangle2dMesh extends Extrude {
  shape: "triangle2d";
  vertices?: [V2, V2, V2];
}
interface Capsule2dMesh extends Extrude {
  shape: "capsule2d";
  radius?: number;
  length?: number;
  resolution?: number;
}
interface ConvexPolygonMesh extends Extrude {
  shape: "convexPolygon";
  vertices: V2[];
}

type MeshProps = (
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
  | ConvexPolygonMesh
) & {
  color?: Bindable<string>;
};

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
  /** A scene component takes its fields and its scene's props; a component class takes its fields. */
  type LibraryManagedAttributes<C, P> =
    C extends { scene(props: infer Props): unknown } & (abstract new () => infer I) ? ComponentProps<I> & Props & { key?: string }
    : C extends abstract new () => infer I extends import("./mod.ts").Component | import("./mod.ts").ReflectComponent ? ComponentProps<I>
    : P;
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
