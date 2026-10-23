// Type tests for reflected values in TSX (reflect-enums-color): colours, enum components and
// writable holes. `tsc -p test` must pass, and every `@ts-expect-error` must be an error.
import { app, hex, hsl, linear, srgb, Startup } from "bevy-ts";
import type { ColorValue, Commands, Query } from "bevy-ts";
import { DirectionalLight, PointLight, Projection } from "bevy-ts/bevy";

const warm: ColorValue = srgb(1, 0.9, 0.8);
export const Lights = () => (
  <>
    <entity><DirectionalLight illuminance={8000} color={hex("#ffd7a8")} /></entity>
    <entity><DirectionalLight color={warm} /></entity>
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

// A hole is a TSX prop only: TS system rows do not carry it.
export function look(q: Query<[DirectionalLight]>) {
  q.each((l) => {
    const lux: number = l.illuminance;
    // @ts-expect-error `color` is not a column
    const c = l.color;
    void lux;
    void c;
  });
}
app.addSystems(Startup, look);

export function cam(commands: Commands) {
  commands.spawnNative("camera3d", { position: [10, 10, 10], lookAt: [0, 0, 0], projection: { Orthographic: { scaling_mode: { FixedVertical: { viewport_height: 12 } } } } });
  commands.spawnNative("cuboid", { color: hsl(30, 1, 0.5) });
}
app.addSystems(Startup, cam);
