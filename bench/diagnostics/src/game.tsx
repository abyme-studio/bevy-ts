// The diagnostics bench: n entities, and what a frame costs with each diagnostics plugin.
// run.mjs writes src/config.ts and bevy-ts.json (the plugin under test) and reads `bench:`.
import { app, Component, First, Last, Startup } from "bevy-ts";
import type { Commands } from "bevy-ts";
import { Camera3d, Transform } from "bevy-ts/bevy";
import { C } from "./config.ts";

export class Thing extends Component {}

const DRAWN = 1000;
const WARM = 120;
const WINDOW = 300;

export function spawn(commands: Commands) {
  commands.spawn(
    <entity name="camera">
      <Camera3d />
      <Transform translation={[0, 60, 60]} rotation={[-0.3826834, 0, 0, 0.9238795]} />
    </entity>,
  );
  for (let i = 0; i < C.n; i++) {
    const at: [number, number, number] = [(i % 100) - 50, 0, Math.floor(i / 100) % 100 - 50];
    if (i < DRAWN) commands.spawnNative("cuboid", { size: [0.5, 0.5, 0.5], color: "#9aa4b0", position: at });
    else commands.spawn(<entity><Transform translation={at} /><Thing /></entity>);
  }
}

let frame = 0;
let t0 = 0;
let last = 0;
const main: number[] = [];
const delta: number[] = [];

export function start() {
  t0 = performance.now();
}

export function end() {
  const now = performance.now();
  frame++;
  if (frame > WARM && frame <= WARM + WINDOW) {
    main.push(now - t0);
    delta.push(now - last);
  }
  last = now;
  if (frame === WARM + WINDOW) {
    const mean = (xs: number[]) => xs.reduce((a, b) => a + b, 0) / xs.length;
    console.log(`bench: main ${mean(main).toFixed(3)} ms, frame ${mean(delta).toFixed(3)} ms, n ${C.n}`);
  }
}

app.addSystems(Startup, spawn);
app.addSystems(First, start);
app.addSystems(Last, end);
