// `bevy-ts/bevy`: Rust components TS can name. `Transform` has a native layout; the rest are
// usable in `With`/`Without` filters only.
import type { ReflectComponent } from "bevy-ts";

type Reflected = abstract new () => ReflectComponent;

export { Time, Transform } from "bevy-ts";

export declare const GlobalTransform: Reflected;
export type GlobalTransform = InstanceType<typeof GlobalTransform>;
/** `Name`; as an element, `<Name value="..." />` (or `<entity name="...">`). */
export declare const Name: Reflected & { readonly __nameElement: true };
export type Name = InstanceType<typeof Name>;
export declare const ChildOf: Reflected;
export type ChildOf = InstanceType<typeof ChildOf>;
export declare const Children: Reflected;
export type Children = InstanceType<typeof Children>;
export declare const Visibility: Reflected;
export type Visibility = InstanceType<typeof Visibility>;
export declare const Camera3d: Reflected;
export type Camera3d = InstanceType<typeof Camera3d>;
export declare const DirectionalLight: Reflected;
export type DirectionalLight = InstanceType<typeof DirectionalLight>;
export declare const Mesh3d: Reflected;
export type Mesh3d = InstanceType<typeof Mesh3d>;
