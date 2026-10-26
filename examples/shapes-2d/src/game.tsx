// Every 2D shape as a `<mesh2d>`, in a grid seen by a `Camera2d` (Bevy's 2d_shapes example),
// and a row of sprites: plain, tinted, flipped; click one to tint it. One world unit is one
// pixel. docs/2d.md lists what 2D takes. Not shipped (no example.json):
// `bevy-ts dev src/game.tsx`.
import { app, signal } from "bevy-ts";
import type { Mesh2dProps } from "bevy-ts/jsx-runtime";
import { Camera2d, Sprite, Transform } from "bevy-ts/bevy";

const shapes: Mesh2dProps[] = [
  { shape: "circle", radius: 50 },
  { shape: "circularSector", radius: 50, angle: 2 },
  { shape: "circularSegment", radius: 50, halfAngle: 1.2 },
  { shape: "ellipse", halfSize: [25, 50] },
  { shape: "annulus", innerRadius: 25, outerRadius: 50 },
  { shape: "capsule2d", radius: 25, length: 50 },
  { shape: "rhombus", horizontalDiagonal: 75, verticalDiagonal: 100 },
  { shape: "rectangle", size: [50, 100] },
  { shape: "regularPolygon", circumradius: 50, sides: 6 },
  { shape: "triangle2d", vertices: [[-50, -50], [0, 50], [50, -50]] },
  { shape: "convexPolygon", vertices: [[-50, -40], [50, -40], [30, 50], [-30, 50]] },
];

const COLUMNS = 6;
const STEP = 140;

type SpriteRow = { key: string; color?: string; flip_x?: boolean; flip_y?: boolean };
const sprites: SpriteRow[] = [
  { key: "plain" },
  { key: "tinted", color: "#4cf" },
  { key: "flipped", flip_x: true },
  { key: "upside-down", flip_y: true },
];
const [clicked, setClicked] = signal<string | null>(null);

export const Shapes = () => (
  <>
    <entity name="camera">
      <Camera2d />
    </entity>
    {shapes.map((props, i) => {
      const col = i % COLUMNS;
      const row = Math.floor(i / COLUMNS);
      const x = (col - (COLUMNS - 1) / 2) * STEP;
      const y = (0.5 - row) * STEP;
      return (
        <entity key={props.shape} name={props.shape}>
          <Transform translation={[x, y, 0]} />
          <mesh2d {...props} />
          <material2d color={{ Hsla: { hue: (360 * i) / shapes.length, saturation: 0.95, lightness: 0.7 } }} />
        </entity>
      );
    })}
    {sprites.map(({ key, color = "#fff", flip_x = false, flip_y = false }, i) => (
      <entity key={key} name={`sprite ${key}`} onPointerClick={() => setClicked((c) => (c === key ? null : key))}>
        <Transform translation={[(i - (sprites.length - 1) / 2) * STEP, -1.5 * STEP, 0]} />
        <Sprite image="arrow.png" custom_size={[96, 96]} flip_x={flip_x} flip_y={flip_y} color={() => (clicked() === key ? "#f44" : color)} />
      </entity>
    ))}
  </>
);

app.scene("shapes", Shapes);
