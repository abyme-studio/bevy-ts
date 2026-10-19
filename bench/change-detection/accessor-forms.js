// Accessor forms for the per-row tick store, for engines without a JIT.
//
//   qjs bench/change-detection/accessor-forms.js
//   deno run --v8-flags=--jitless bench/change-detection/accessor-forms.js
//   deno run bench/change-detection/accessor-forms.js
//
// 100k rows, every row written through three field setters. Compares a class
// accessor (fields on `this`) with a closure accessor (module-level variables),
// each with and without the tick store. The question: what the store costs once
// the accessor itself is as cheap as the engine allows.

const STRIDE = 8, N = 100000, TICK = 1234567;
const now = typeof performance !== "undefined" ? () => performance.now() : () => Date.now();
const engine = typeof Deno !== "undefined" ? `deno v8 ${Deno.version.v8}` : typeof scriptArgs !== "undefined" ? "quickjs-ng" : "node";

const f32 = new Float32Array(N * STRIDE);
const ticks = new Uint32Array(N);
for (let i = 0; i < N; i++) { f32[i * STRIDE + 3] = 1.5; f32[i * STRIDE + 4] = 0.5; f32[i * STRIDE + 5] = 0.25; }

class ClassAcc {
  constructor() { this.f32 = f32; this.ticks = ticks; this.b = 0; this.i = 0; }
  get x() { return this.f32[this.b]; } set x(v) { this.f32[this.b] = v; }
  get y() { return this.f32[this.b + 1]; } set y(v) { this.f32[this.b + 1] = v; }
  get z() { return this.f32[this.b + 2]; } set z(v) { this.f32[this.b + 2] = v; }
  get vx() { return this.f32[this.b + 3]; } get vy() { return this.f32[this.b + 4]; } get vz() { return this.f32[this.b + 5]; }
}
class ClassAccTick {
  constructor() { this.f32 = f32; this.ticks = ticks; this.b = 0; this.i = 0; }
  get x() { return this.f32[this.b]; } set x(v) { this.f32[this.b] = v; this.ticks[this.i] = TICK; }
  get y() { return this.f32[this.b + 1]; } set y(v) { this.f32[this.b + 1] = v; this.ticks[this.i] = TICK; }
  get z() { return this.f32[this.b + 2]; } set z(v) { this.f32[this.b + 2] = v; this.ticks[this.i] = TICK; }
  get vx() { return this.f32[this.b + 3]; } get vy() { return this.f32[this.b + 4]; } get vz() { return this.f32[this.b + 5]; }
}
// tick store hoisted: the setter flags, each() stores once per row
class ClassAccFlag {
  constructor() { this.f32 = f32; this.b = 0; this.d = 0; }
  get x() { return this.f32[this.b]; } set x(v) { this.f32[this.b] = v; this.d = 1; }
  get y() { return this.f32[this.b + 1]; } set y(v) { this.f32[this.b + 1] = v; this.d = 1; }
  get z() { return this.f32[this.b + 2]; } set z(v) { this.f32[this.b + 2] = v; this.d = 1; }
  get vx() { return this.f32[this.b + 3]; } get vy() { return this.f32[this.b + 4]; } get vz() { return this.f32[this.b + 5]; }
}

// closure accessors: position in module-level variables, arrays in module scope
let cb = 0, ci = 0, cd = 0;
const closureAcc = {
  get x() { return f32[cb]; }, set x(v) { f32[cb] = v; },
  get y() { return f32[cb + 1]; }, set y(v) { f32[cb + 1] = v; },
  get z() { return f32[cb + 2]; }, set z(v) { f32[cb + 2] = v; },
  get vx() { return f32[cb + 3]; }, get vy() { return f32[cb + 4]; }, get vz() { return f32[cb + 5]; },
};
const closureAccTick = {
  get x() { return f32[cb]; }, set x(v) { f32[cb] = v; ticks[ci] = TICK; },
  get y() { return f32[cb + 1]; }, set y(v) { f32[cb + 1] = v; ticks[ci] = TICK; },
  get z() { return f32[cb + 2]; }, set z(v) { f32[cb + 2] = v; ticks[ci] = TICK; },
  get vx() { return f32[cb + 3]; }, get vy() { return f32[cb + 4]; }, get vz() { return f32[cb + 5]; },
};
const closureAccFlag = {
  get x() { return f32[cb]; }, set x(v) { f32[cb] = v; cd = 1; },
  get y() { return f32[cb + 1]; }, set y(v) { f32[cb + 1] = v; cd = 1; },
  get z() { return f32[cb + 2]; }, set z(v) { f32[cb + 2] = v; cd = 1; },
  get vx() { return f32[cb + 3]; }, get vy() { return f32[cb + 4]; }, get vz() { return f32[cb + 5]; },
};

function user(m, dt) { m.x += m.vx * dt; m.y += m.vy * dt; m.z += m.vz * dt; }

const classAcc = new ClassAcc(), classAccTick = new ClassAccTick(), classAccFlag = new ClassAccFlag();
const runs = {
  "class": (dt) => { const m = classAcc; for (let i = 0; i < N; i++) { m.b = i * STRIDE; user(m, dt); } },
  "class+tick": (dt) => { const m = classAccTick; for (let i = 0; i < N; i++) { m.b = i * STRIDE; m.i = i; user(m, dt); } },
  "class+flag": (dt) => { const m = classAccFlag; for (let i = 0; i < N; i++) { m.b = i * STRIDE; user(m, dt); if (m.d) { ticks[i] = TICK; m.d = 0; } } },
  "closure": (dt) => { const m = closureAcc; for (let i = 0; i < N; i++) { cb = i * STRIDE; user(m, dt); } },
  "closure+tick": (dt) => { const m = closureAccTick; for (let i = 0; i < N; i++) { cb = i * STRIDE; ci = i; user(m, dt); } },
  "closure+flag": (dt) => { const m = closureAccFlag; for (let i = 0; i < N; i++) { cb = i * STRIDE; user(m, dt); if (cd) { ticks[i] = TICK; cd = 0; } } },
  "raw": (dt) => { for (let i = 0; i < N; i++) { const b = i * STRIDE; f32[b] += f32[b + 3] * dt; f32[b + 1] += f32[b + 4] * dt; f32[b + 2] += f32[b + 5] * dt; } },
  "raw+tick": (dt) => { for (let i = 0; i < N; i++) { const b = i * STRIDE; f32[b] += f32[b + 3] * dt; f32[b + 1] += f32[b + 4] * dt; f32[b + 2] += f32[b + 5] * dt; ticks[i] = TICK; } },
};

function bench(fn) {
  const dt = 1 / 60;
  const w0 = now(); let k = 0;
  while (now() - w0 < 400 || k < 3) { fn(dt); k++; }
  const times = [];
  const t0 = now();
  while (times.length < 15 || (now() - t0 < 1500 && times.length < 200)) { const a = now(); fn(dt); times.push(now() - a); }
  times.sort((p, q) => p - q);
  return times[times.length >> 1];
}

console.log(`engine: ${engine}, ${N} rows, every row written through 3 setters, ms per run (median)\n`);
console.log("| form | ms |\n|---|---|");
for (const name in runs) console.log(`| ${name} | ${bench(runs[name]).toFixed(2)} |`);
