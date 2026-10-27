// Every 2D shape as a `<mesh2d>`, in a grid seen by a `Camera2d` (Bevy's 2d_shapes example),
// and a row of sprites: plain, tinted, flipped; click one to tint it. Beside them, a sprite sheet
// a `<SpriteClip>` animates (Bevy's sprite_animation example), a `<text2d>` counting the clicks, and three
// y-sorted walkers pacing past each other at the right edge, the first with a screen-size marker
// over its head (click it). The camera is a pixel camera: keys 1, 2 and 4 zoom it, and zoomed in it
// follows that walker, so the marker and moving texels stay in view. A title in a bitmap font
// counts the clicks too. One world unit is one pixel. docs/2d.md
// lists what 2D takes. Not shipped (no example.json): `bevy-ts dev src/game.tsx`.
import { app, Component, signal, sliced, Time, Update, type f32, type Mut, type Query, type Res, type With, type Without } from "bevy-ts";
import type { Mesh2dProps } from "bevy-ts/jsx-runtime";
import { KeyCode, type Keys } from "bevy-ts/input";
import { Camera2d, PixelCamera, ScreenSize, Sprite, SpriteClip, TextureAtlasLayout, Transform, YSort } from "bevy-ts/bevy";

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

/** A walker's place in its pacing: walkers out of step pass each other. */
export class Walker extends Component {
  phase: f32 = 0;
}

/** Walkers pace up and down; `YSort` draws the lower one in front as they pass. */
export function pace(time: Res<Time>, walkers: Query<[Mut<Transform>, Walker]>) {
  for (const [t, w] of walkers) t.translation.y = Math.sin(time.elapsed_secs * 0.8 + w.phase) * 200;
}
app.addSystems(Update, pace);

/** The walker the camera follows when zoomed in. */
export class Followed extends Component {}

/** Zoomed in, the camera follows the followed walker; at zoom 1 it shows the whole scene. */
export function follow(
  cameras: Query<[Mut<Transform>, PixelCamera], Without<Followed>>,
  followed: Query<[Transform], With<Followed>>,
) {
  for (const [walker] of followed) {
    for (const [t, camera] of cameras) {
      t.translation.x = camera.zoom > 1 ? walker.translation.x : 0;
      t.translation.y = camera.zoom > 1 ? walker.translation.y : 0;
    }
  }
}
app.addSystems(Update, follow);

/** Keys 1, 2 and 4 zoom the pixel camera, easing there. */
export function zoomKeys(keys: Keys, cameras: Query<[Mut<PixelCamera>]>) {
  const zoom = keys.justPressed(KeyCode.Digit1) ? 1 : keys.justPressed(KeyCode.Digit2) ? 2 : keys.justPressed(KeyCode.Digit4) ? 4 : 0;
  if (zoom === 0) return;
  for (const [camera] of cameras) camera.zoom = zoom;
}
app.addSystems(Update, zoomKeys);

const walkers = [
  { key: "walker-a", color: "#f84", phase: 0 },
  { key: "walker-b", color: "#4f8", phase: 2.1 },
  { key: "walker-c", color: "#48f", phase: 4.2 },
];

const click = (key: string) => {
  setClicked((c) => (c === key ? null : key));
  setClicks((n) => n + 1);
};

export const Shapes = () => (
  <>
    <entity name="camera">
      <Camera2d />
      <PixelCamera zoom={1} />
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
    <entity key="frame" name="sprite sheet">
      <Transform translation={[2.5 * STEP, -1.5 * STEP, 0]} />
      <Sprite image="sheet.png" custom_size={[96, 96]} texture_atlas={{ layout: sheet, index: 0 }} />
      <SpriteClip first={0} last={3} fps={10} mode="Repeat" />
    </entity>
    {/* panel.png is 32 px with an 8 px border: sliced, its corners keep their size as it stretches. */}
    <entity key="panel" name="sliced panel">
      <Transform translation={[-2.5 * STEP, -1.5 * STEP, 0]} />
      <Sprite image="panel.png" custom_size={[120, 72]} image_mode={sliced({ border: 8 })} />
    </entity>
    {walkers.map(({ key, color, phase }, i) => (
      <entity key={key} name={key}>
        <Transform translation={[430 + 10 * i, 0, 0]} />
        <Sprite image="arrow.png" custom_size={[40, 40]} color={color} />
        <YSort layer={1} offset={-20} />
        <Walker phase={phase} />
        {i === 0 && <Followed />}
        {i === 0 && (
          <entity key="marker" name="marker" onPointerClick={() => click("marker")}>
            <Transform translation={[0, 32, 0.5]} />
            <Sprite image="arrow.png" custom_size={[16, 16]} color={() => (clicked() === "marker" ? "#f44" : "#ff4")} flip_y />
            <ScreenSize />
          </entity>
        )}
      </entity>
    ))}
    {/* fonts/tiny.json is a 3x5 bitmap font made for this example (an XNA SpriteFont as JSON and PNG). */}
    <entity key="title" name="title">
      <Transform translation={[0, 1.8 * STEP, 0]} />
      <bitmapText font="fonts/tiny.json" scale={3} color="#ff4">
        SHAPES 2D: {() => `${clicks()}`} CLICKS
      </bitmapText>
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
