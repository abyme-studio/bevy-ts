// `bevy-ts/bevy`: Rust components TS can name. Only those with a native layout registered in
// Rust (`Transform`) can be read or written by TS systems; the rest work in `with`/`without`.
import { nativeComponent, rustComponent, vec3, quat } from "bevy-ts";

export const Transform = nativeComponent("Transform", {
  translation: vec3.default([0, 0, 0]),
  rotation: quat.default([0, 0, 0, 1]),
  scale: vec3.default([1, 1, 1]),
});

export const GlobalTransform = rustComponent("GlobalTransform");
export const Name = rustComponent("Name");
export const ChildOf = rustComponent("ChildOf");
export const Children = rustComponent("Children");
export const Visibility = rustComponent("Visibility");
export const Camera3d = rustComponent("Camera3d");
export const DirectionalLight = rustComponent("DirectionalLight");
export const Mesh3d = rustComponent("Mesh3d");
