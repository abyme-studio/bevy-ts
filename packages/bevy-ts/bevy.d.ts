// `bevy-ts/bevy`: Rust components TS can name.
import type { ComponentType, FieldType, RustComponent, Vec3View, Vec3Value, Vec4View, Vec4Value } from "bevy-ts";

export declare const Transform: ComponentType<{
  translation: FieldType<Vec3View, Vec3Value, "vec3">;
  rotation: FieldType<Vec4View, Vec4Value, "quat">;
  scale: FieldType<Vec3View, Vec3Value, "vec3">;
}>;

export declare const GlobalTransform: RustComponent;
export declare const Name: RustComponent;
export declare const ChildOf: RustComponent;
export declare const Children: RustComponent;
export declare const Visibility: RustComponent;
export declare const Camera3d: RustComponent;
export declare const DirectionalLight: RustComponent;
export declare const Mesh3d: RustComponent;
