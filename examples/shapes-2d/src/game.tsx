// Every 2D shape as a `<mesh2d>`, in a grid seen by a `Camera2d` (Bevy's 2d_shapes example),
// and a row of sprites: plain, tinted, flipped; click one to tint it. Beside them, frame 3 of a
// sprite sheet, and a `<text2d>` counting the clicks. One world unit is one pixel. docs/2d.md
// lists what 2D takes. Not shipped (no example.json): `bevy-ts dev src/game.tsx`.
import { app, signal, sliced } from "bevy-ts";
import type { Mesh2dProps } from "bevy-ts/jsx-runtime";
import { Camera2d, Sprite, TextureAtlasLayout, Transform } from "bevy-ts/bevy";

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
const [clicks, setClicks] = signal(0);
/** sheet.png: four 32-pixel tiles in a row, marked 1 to 4. */
const sheet = TextureAtlasLayout.fromGrid({ tileSize: [32, 32], columns: 4, rows: 1 });
const click = (key: string) => {
  setClicked((c) => (c === key ? null : key));
  setClicks((n) => n + 1);
};

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
      <entity key={key} name={`sprite ${key}`} onPointerClick={() => click(key)}>
        <Transform translation={[(i - (sprites.length - 1) / 2) * STEP, -1.5 * STEP, 0]} />
        <Sprite image="arrow.png" custom_size={[96, 96]} flip_x={flip_x} flip_y={flip_y} color={() => (clicked() === key ? "#f44" : color)} />
      </entity>
    ))}
    <entity key="frame" name="sheet frame 3">
      <Transform translation={[2.5 * STEP, -1.5 * STEP, 0]} />
      <Sprite image="sheet.png" custom_size={[96, 96]} texture_atlas={{ layout: sheet, index: 2 }} />
    </entity>
    {/* panel.png is 32 px with an 8 px border: sliced, its corners keep their size as it stretches. */}
    <entity key="panel" name="sliced panel">
      <Transform translation={[-2.5 * STEP, -1.5 * STEP, 0]} />
      <Sprite image="panel.png" custom_size={[120, 72]} image_mode={sliced({ border: 8 })} />
    </entity>
    <entity key="clicks" name="clicks">
      {/* Above the shapes: inside a 540-pixel-high view. */}
      <Transform translation={[0, 1.4 * STEP, 0]} />
      <text2d size={28} color="#fff" textShadow={{ y: -2 }}>
        Clicks: {() => `${clicks()}`}<span color="#f44">{() => (clicked() ? ` (${clicked()} is red)` : "")}</span>
      </text2d>
    </entity>
  </>
);

app.scene("shapes", Shapes);
