// The gizmos bench: what 1,000 lines a frame cost a TS system, by arm (README.md).
// run.mjs writes src/config.ts and reads the `bench:` line.
import { app, First, Last, srgb, Update } from "bevy-ts";
import type { Gizmos } from "bevy-ts/gizmos";
import { KeyCode, type Keys } from "bevy-ts/input";
import { C } from "./config.ts";

const LINES = 1000;
const WARM = 60;
const WINDOW = 300;

const red = srgb(1, 0, 0);
const strip = new Float32Array(LINES * 3);
for (let k = 0; k < LINES; k++) {
  strip[k * 3] = k * 0.01;
  strip[k * 3 + 1] = k % 2;
}

// Time inside the drawing system, summed per frame: the calls' own cost, free of frame noise.
let sys = 0;

export function linesHex(gizmos: Gizmos) {
  const t = performance.now();
  for (let k = 0; k < LINES; k++) gizmos.line([k * 0.01, 0, 0], [k * 0.01, 1, 0], "#f00");
  sys += performance.now() - t;
}

export function linesSrgb(gizmos: Gizmos) {
  const t = performance.now();
  for (let k = 0; k < LINES; k++) gizmos.line([k * 0.01, 0, 0], [k * 0.01, 1, 0], red);
  sys += performance.now() - t;
}

export function linesArray(gizmos: Gizmos) {
  const t = performance.now();
  for (let k = 0; k < LINES; k++) gizmos.line([k * 0.01, 0, 0], [k * 0.01, 1, 0], [1, 0, 0]);
  sys += performance.now() - t;
}

export function stripF32(gizmos: Gizmos) {
  const t = performance.now();
  gizmos.linestrip(strip, red);
  sys += performance.now() - t;
}

// The floor: 1,000 calls of another exported param with one argument and no drawing.
export function keys(keys: Keys) {
  const t = performance.now();
  for (let k = 0; k < LINES; k++) keys.pressed(KeyCode.KeyA);
  sys += performance.now() - t;
}

let frame = 0;
let t0 = 0;
let last = 0;
const main: number[] = [];
const delta: number[] = [];
const sysMs: number[] = [];

export function start() {
  sys = 0;
  t0 = performance.now();
}

export function end() {
  const now = performance.now();
  frame++;
  if (frame > WARM && frame <= WARM + WINDOW) {
    main.push(now - t0);
    delta.push(now - last);
    sysMs.push(sys);
  }
  last = now;
  if (frame === WARM + WINDOW) {
    const mean = (xs: number[]) => xs.reduce((a, b) => a + b, 0) / xs.length;
    console.log(`bench: main ${mean(main).toFixed(3)} ms, frame ${mean(delta).toFixed(3)} ms, sys ${mean(sysMs).toFixed(4)} ms, arm ${C.arm}`);
  }
}

if (C.arm === "lines_hex") app.addSystems(Update, linesHex);
if (C.arm === "lines_srgb") app.addSystems(Update, linesSrgb);
if (C.arm === "lines_array") app.addSystems(Update, linesArray);
if (C.arm === "keys") app.addSystems(Update, keys);
if (C.arm === "strip_f32") app.addSystems(Update, stripF32);
app.addSystems(First, start);
app.addSystems(Last, end);
