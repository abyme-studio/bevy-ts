// Type tests for reflected values in TSX (reflect-enums-color): colours, enum components and
// writable holes. `tsc -p test` must pass, and every `@ts-expect-error` must be an error.
import { app, hex, hsl, linear, signal, srgb, Startup, Transform } from "bevy-ts";
import type { ColorValue, Commands, Query } from "bevy-ts";
import { Camera3d, DirectionalLight, DistanceFog, PointLight, Projection } from "bevy-ts/bevy";

const warm: ColorValue = srgb(1, 0.9, 0.8);
export const Lights = () => (
  <>
    <entity><DirectionalLight illuminance={8000} color={hex("#ffd7a8")} /></entity>
    <entity><DirectionalLight color={warm} /></entity>
    <entity><DirectionalLight shadow_maps_enabled soft_shadow_size={0.5} /></entity>
    <entity><DirectionalLight soft_shadow_size={null} /></entity>
    <entity><PointLight color={hsl(30, 0.8, 0.6)} /></entity>
    <entity><PointLight color="#fff" /></entity>
    <entity><PointLight color={[1, 0.5, 0]} /></entity>
    <entity><PointLight color={{ LinearRgba: { red: 1, green: 0.5, blue: 0 } }} /></entity>
    <entity><mesh shape="cuboid" color={linear(1, 0.5, 0)} /></entity>
    <entity><Projection value={{ Orthographic: { scale: 2 } }} /></entity>
    <entity><Projection value={{ Orthographic: { scaling_mode: { FixedVertical: { viewport_height: 12 } } } }} /></entity>
    <entity><Projection value={{ Orthographic: { scaling_mode: "WindowSize" } }} /></entity>
  </>
);

// reflect-bindable, layout-bindable: a reflect-tier prop and a layout-tier hole both bind.
const [zoomed] = signal(false);
const [night] = signal(false);
export const Bound = () => (
  <>
    <entity><Projection value={() => (zoomed() ? { Orthographic: {} } : { Perspective: {} })} /></entity>
    <entity><DirectionalLight color={() => "#fff"} /></entity>
    <entity><DistanceFog color={() => (night() ? "#101020" : "#c0d0ff")} /></entity>
    {/* @ts-expect-error a bound hole is still checked: a number is not a colour */}
    <entity><DistanceFog color={() => 3} /></entity>
    {/* @ts-expect-error a bound value is still checked against the enum */}
    <entity><Projection value={() => ({ Isometric: {} })} /></entity>
  </>
);

export const Bad = () => (
  <>
    {/* @ts-expect-error a number is not a colour */}
    <PointLight color={3} />
    {/* @ts-expect-error Projection has no Isometric variant */}
    <Projection value={{ Isometric: {} }} />
    {/* @ts-expect-error a typo'd field */}
    <Projection value={{ Orthographic: { scael: 2 } }} />
  </>
);

// A hole is a TSX prop, and TS system rows read it through reflection (reflect-coverage stage 3).
export function look(q: Query<[DirectionalLight]>) {
  q.each((l) => {
    const lux: number = l.illuminance;
    const c: ColorValue = l.color;
    void lux;
    void c;
  });
}
app.addSystems(Startup, look);

export function cam(commands: Commands) {
  // reflect-coverage stage 2: reflect-tier values and holes through commands, no recipes.
  commands.spawn(Camera3d, Transform.lookingAt([10, 10, 10], [0, 0, 0]), Projection.of({ Orthographic: { scaling_mode: { FixedVertical: { viewport_height: 12 } } } }));
  commands.spawn(DirectionalLight.of({ illuminance: 8000, color: hsl(30, 1, 0.5) }), Transform.lookingAt([3, 5, 2], [0, 0, 0]));
  commands.spawn(Transform.of({ translation: [0, 0.5, 0] }), <entity><mesh shape="cuboid" size={[1, 1, 1]} /><material color="#ff8000" /></entity>);
  // @ts-expect-error: Projection has no variant Isometric.
  commands.spawn(Projection.of({ Isometric: {} }));
  // @ts-expect-error: DirectionalLight's colour is a ColorValue.
  commands.spawn(DirectionalLight.of({ color: 3 }));
  // @ts-expect-error: .of writes once, so a hole takes a value, not a binding (layout-bindable).
  commands.spawn(DirectionalLight.of({ color: () => "#fff" }));
}
app.addSystems(Startup, cam);
