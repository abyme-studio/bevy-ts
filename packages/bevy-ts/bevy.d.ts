// `bevy-ts/bevy`: Rust components TS can name. `Transform` has a native layout; the rest are
// usable in `With`/`Without` filters only.
import type { ComponentClass } from "bevy-ts";

export { Time, Transform } from "bevy-ts";

export declare const GlobalTransform: ComponentClass<{}, "reflect">;
export type GlobalTransform = InstanceType<typeof GlobalTransform>;
/** `Name`; as an element, `<Name value="..." />` (or `<entity name="...">`). */
export declare const Name: ComponentClass<{}, "reflect"> & { readonly __nameElement: true };
export type Name = InstanceType<typeof Name>;
export declare const ChildOf: ComponentClass<{}, "reflect">;
export type ChildOf = InstanceType<typeof ChildOf>;
export declare const Children: ComponentClass<{}, "reflect">;
export type Children = InstanceType<typeof Children>;
export declare const Visibility: ComponentClass<{}, "reflect">;
export type Visibility = InstanceType<typeof Visibility>;
export declare const Camera3d: ComponentClass<{}, "reflect">;
export type Camera3d = InstanceType<typeof Camera3d>;
export declare const DirectionalLight: ComponentClass<{}, "reflect">;
export type DirectionalLight = InstanceType<typeof DirectionalLight>;
export declare const Mesh3d: ComponentClass<{}, "reflect">;
export type Mesh3d = InstanceType<typeof Mesh3d>;
