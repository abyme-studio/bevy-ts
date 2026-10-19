// `bevy-ts/bevy`: Rust components TS can name. Only those with a native layout registered in
// Rust (`Transform`) can be query data; the rest work in `With`/`Without` filters.
import { rustComponent } from "bevy-ts";

export { Time, Transform } from "bevy-ts";

export const GlobalTransform = rustComponent("GlobalTransform");
export const Name = rustComponent("Name");
export const ChildOf = rustComponent("ChildOf");
export const Children = rustComponent("Children");
export const Visibility = rustComponent("Visibility");
export const Camera3d = rustComponent("Camera3d");
export const DirectionalLight = rustComponent("DirectionalLight");
export const Mesh3d = rustComponent("Mesh3d");
