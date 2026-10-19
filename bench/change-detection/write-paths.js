// Change-detection write paths for TS systems over zero-copy Bevy columns.
//
// Runs unchanged on V8 (deno, node), V8 jitless and QuickJS-ng:
//   deno run bench/change-detection/write-paths.js [rows=10000,100000,1000000] [fracs=0.003,0.1,1]
//   deno run --v8-flags=--jitless bench/change-detection/write-paths.js
//   node bench/change-detection/write-paths.js
//   qjs bench/change-detection/write-paths.js
//
// Model: one table column of a 32-byte component (x y z vx vy vz flags pad), a
// Uint32Array of Bevy changed ticks of the same length (Bevy 0.20 keeps them as a
// contiguous u32 array per column, so JS can view it the way it views the data), a
// random mask `want` saying which rows the system's own logic writes, and a
// per-frame tick. Each path is its own function and its own accessor class so V8
// optimises and inlines them separately.
//
// Two write patterns:
//   cond: the system writes only rows it decides to (frac of rows), reads the rest.
//   all:  the system writes every row, but only frac of rows get a different value
//         (vx == 0 elsewhere). Distinguishes "wrote" from "changed".
//
// Output: one markdown table per run, median of repeated runs, in ms.

const STRIDE = 8; // f32 slots per row (32 bytes)
const TICK = 1234567;

const now = (() => {
  if (typeof performance !== "undefined" && performance.now) return () => performance.now();
  return () => Date.now();
})();

const argv = (() => {
  if (typeof Deno !== "undefined") return Deno.args;
  if (typeof scriptArgs !== "undefined") return scriptArgs.slice(1);
  if (typeof process !== "undefined") return process.argv.slice(2);
  return [];
})();

const engine = (() => {
  if (typeof Deno !== "undefined") return `deno ${Deno.version.deno} / v8 ${Deno.version.v8}`;
  if (typeof process !== "undefined") return `node ${process.version} / v8 ${process.versions.v8}`;
  if (typeof scriptArgs !== "undefined") return "quickjs-ng";
  return "unknown";
})();

const rowsArg = argv[0] ?? "10000,100000,1000000";
const fracArg = argv[1] ?? "0.003,0.1,1";
const ROWS = rowsArg.split(",").map(Number);
const FRACS = fracArg.split(",").map(Number);
const BUDGET = Number(argv[2] ?? 1); // multiplies the per-cell time budget (interpreters are noisy)

// xorshift so every engine sees the same mask
let seed = 0x9e3779b9;
function rnd() {
  seed ^= seed << 13; seed >>>= 0;
  seed ^= seed >>> 17;
  seed ^= seed << 5; seed >>>= 0;
  return seed / 4294967296;
}

function makeWorld(n, frac) {
  const buf = new ArrayBuffer(n * STRIDE * 4);
  const f32 = new Float32Array(buf);
  const u32 = new Uint32Array(buf);
  const ticks = new Uint32Array(n);
  const want = new Uint8Array(n);
  const shadow = new Uint32Array(n * STRIDE);
  const bits = new Uint32Array((n + 31) >>> 5);
  // command arena: 16 bytes per field write, sized for the worst case (3 fields × n)
  const arena = new ArrayBuffer(n * 3 * 16);
  const arenaU32 = new Uint32Array(arena);
  const arenaF32 = new Float32Array(arena);
  seed = 0x9e3779b9;
  for (let i = 0; i < n; i++) {
    const w = rnd() < frac ? 1 : 0;
    want[i] = w;
    const b = i * STRIDE;
    f32[b] = i; f32[b + 1] = i * 2; f32[b + 2] = i * 3;
    f32[b + 3] = w ? 1.5 : 0; f32[b + 4] = w ? 0.5 : 0; f32[b + 5] = w ? 0.25 : 0;
    u32[b + 6] = 0; u32[b + 7] = 0;
  }
  return { n, frac, f32, u32, ticks, want, shadow, bits, arenaU32, arenaF32, cursor: 0 };
}

// ---------- raw typed-array paths (q.tables style) ----------

function rawCond(w, dt) {
  const { f32, want, n } = w;
  for (let i = 0; i < n; i++) {
    if (want[i]) {
      const b = i * STRIDE;
      f32[b] += f32[b + 3] * dt;
      f32[b + 1] += f32[b + 4] * dt;
      f32[b + 2] += f32[b + 5] * dt;
    }
  }
}

function rawTickCond(w, dt) {
  const { f32, want, n, ticks } = w;
  for (let i = 0; i < n; i++) {
    if (want[i]) {
      const b = i * STRIDE;
      f32[b] += f32[b + 3] * dt;
      f32[b + 1] += f32[b + 4] * dt;
      f32[b + 2] += f32[b + 5] * dt;
      ticks[i] = TICK;
    }
  }
}

// tick store on every field write (what a naive accessor setter does)
function rawTickPerFieldCond(w, dt) {
  const { f32, want, n, ticks } = w;
  for (let i = 0; i < n; i++) {
    if (want[i]) {
      const b = i * STRIDE;
      f32[b] += f32[b + 3] * dt; ticks[i] = TICK;
      f32[b + 1] += f32[b + 4] * dt; ticks[i] = TICK;
      f32[b + 2] += f32[b + 5] * dt; ticks[i] = TICK;
    }
  }
}

function rawBitCond(w, dt) {
  const { f32, want, n, bits } = w;
  for (let i = 0; i < n; i++) {
    if (want[i]) {
      const b = i * STRIDE;
      f32[b] += f32[b + 3] * dt;
      f32[b + 1] += f32[b + 4] * dt;
      f32[b + 2] += f32[b + 5] * dt;
      bits[i >>> 5] |= 1 << (i & 31);
    }
  }
}

function rawAll(w, dt) {
  const { f32, n } = w;
  for (let i = 0; i < n; i++) {
    const b = i * STRIDE;
    f32[b] += f32[b + 3] * dt;
    f32[b + 1] += f32[b + 4] * dt;
    f32[b + 2] += f32[b + 5] * dt;
  }
}

function rawTickAll(w, dt) {
  const { f32, n, ticks } = w;
  for (let i = 0; i < n; i++) {
    const b = i * STRIDE;
    f32[b] += f32[b + 3] * dt;
    f32[b + 1] += f32[b + 4] * dt;
    f32[b + 2] += f32[b + 5] * dt;
    ticks[i] = TICK;
  }
}

// set_if_neq on raw arrays: compare before store, tick only on a real change
function rawNeqAll(w, dt) {
  const { f32, n, ticks } = w;
  for (let i = 0; i < n; i++) {
    const b = i * STRIDE;
    const x = f32[b] + f32[b + 3] * dt;
    const y = f32[b + 1] + f32[b + 4] * dt;
    const z = f32[b + 2] + f32[b + 5] * dt;
    let c = 0;
    if (f32[b] !== x) { f32[b] = x; c = 1; }
    if (f32[b + 1] !== y) { f32[b + 1] = y; c = 1; }
    if (f32[b + 2] !== z) { f32[b + 2] = z; c = 1; }
    if (c) ticks[i] = TICK;
  }
}

// ---------- flyweight accessor paths (q.each style) ----------

class AccPlain {
  constructor(f32) { this.f32 = f32; this.b = 0; }
  get x() { return this.f32[this.b]; } set x(v) { this.f32[this.b] = v; }
  get y() { return this.f32[this.b + 1]; } set y(v) { this.f32[this.b + 1] = v; }
  get z() { return this.f32[this.b + 2]; } set z(v) { this.f32[this.b + 2] = v; }
  get vx() { return this.f32[this.b + 3]; }
  get vy() { return this.f32[this.b + 4]; }
  get vz() { return this.f32[this.b + 5]; }
}

// setter writes the row's tick on every write
class AccTick {
  constructor(f32, ticks) { this.f32 = f32; this.ticks = ticks; this.b = 0; this.i = 0; }
  get x() { return this.f32[this.b]; } set x(v) { this.f32[this.b] = v; this.ticks[this.i] = TICK; }
  get y() { return this.f32[this.b + 1]; } set y(v) { this.f32[this.b + 1] = v; this.ticks[this.i] = TICK; }
  get z() { return this.f32[this.b + 2]; } set z(v) { this.f32[this.b + 2] = v; this.ticks[this.i] = TICK; }
  get vx() { return this.f32[this.b + 3]; }
  get vy() { return this.f32[this.b + 4]; }
  get vz() { return this.f32[this.b + 5]; }
}

// setter sets a dirty flag; each() writes the tick once per dirty row
class AccDirty {
  constructor(f32) { this.f32 = f32; this.b = 0; this.d = 0; }
  get x() { return this.f32[this.b]; } set x(v) { this.f32[this.b] = v; this.d = 1; }
  get y() { return this.f32[this.b + 1]; } set y(v) { this.f32[this.b + 1] = v; this.d = 1; }
  get z() { return this.f32[this.b + 2]; } set z(v) { this.f32[this.b + 2] = v; this.d = 1; }
  get vx() { return this.f32[this.b + 3]; }
  get vy() { return this.f32[this.b + 4]; }
  get vz() { return this.f32[this.b + 5]; }
}

// setter compares, writes and flags only on a real change (set_if_neq)
class AccNeq {
  constructor(f32) { this.f32 = f32; this.b = 0; this.d = 0; }
  get x() { return this.f32[this.b]; } set x(v) { if (this.f32[this.b] !== v) { this.f32[this.b] = v; this.d = 1; } }
  get y() { return this.f32[this.b + 1]; } set y(v) { if (this.f32[this.b + 1] !== v) { this.f32[this.b + 1] = v; this.d = 1; } }
  get z() { return this.f32[this.b + 2]; } set z(v) { if (this.f32[this.b + 2] !== v) { this.f32[this.b + 2] = v; this.d = 1; } }
  get vx() { return this.f32[this.b + 3]; }
  get vy() { return this.f32[this.b + 4]; }
  get vz() { return this.f32[this.b + 5]; }
}

// setter appends a field-write op to the command arena (option C)
class AccCmd {
  constructor(w) { this.w = w; this.u32 = w.arenaU32; this.f32v = w.arenaF32; this.f32 = w.f32; this.b = 0; this.i = 0; }
  _emit(off, v) {
    const w = this.w; const c = w.cursor;
    this.u32[c] = 1 /* opcode: write f32 */; this.u32[c + 1] = this.i; this.u32[c + 2] = off; this.f32v[c + 3] = v;
    w.cursor = c + 4;
  }
  get x() { return this.f32[this.b]; } set x(v) { this._emit(0, v); }
  get y() { return this.f32[this.b + 1]; } set y(v) { this._emit(4, v); }
  get z() { return this.f32[this.b + 2]; } set z(v) { this._emit(8, v); }
  get vx() { return this.f32[this.b + 3]; }
  get vy() { return this.f32[this.b + 4]; }
  get vz() { return this.f32[this.b + 5]; }
}

function userCond(i, m, dt, want) {
  if (want[i]) {
    m.x += m.vx * dt;
    m.y += m.vy * dt;
    m.z += m.vz * dt;
  }
}
function userAll(i, m, dt) {
  m.x += m.vx * dt;
  m.y += m.vy * dt;
  m.z += m.vz * dt;
}

function accPlainCond(w, dt) {
  const m = w.accPlain, want = w.want, n = w.n;
  for (let i = 0; i < n; i++) { m.b = i * STRIDE; userCond(i, m, dt, want); }
}
function accTickCond(w, dt) {
  const m = w.accTick, want = w.want, n = w.n;
  for (let i = 0; i < n; i++) { m.b = i * STRIDE; m.i = i; userCond(i, m, dt, want); }
}
function accDirtyCond(w, dt) {
  const m = w.accDirty, want = w.want, n = w.n, ticks = w.ticks;
  for (let i = 0; i < n; i++) {
    m.b = i * STRIDE; userCond(i, m, dt, want);
    if (m.d) { ticks[i] = TICK; m.d = 0; }
  }
}
function accCmdCond(w, dt) {
  const m = w.accCmd, want = w.want, n = w.n;
  w.cursor = 0;
  for (let i = 0; i < n; i++) { m.b = i * STRIDE; m.i = i; userCond(i, m, dt, want); }
}
function accPlainAll(w, dt) {
  const m = w.accPlain, n = w.n;
  for (let i = 0; i < n; i++) { m.b = i * STRIDE; userAll(i, m, dt); }
}
function accTickAll(w, dt) {
  const m = w.accTick, n = w.n;
  for (let i = 0; i < n; i++) { m.b = i * STRIDE; m.i = i; userAll(i, m, dt); }
}
function accDirtyAll(w, dt) {
  const m = w.accDirty, n = w.n, ticks = w.ticks;
  for (let i = 0; i < n; i++) {
    m.b = i * STRIDE; userAll(i, m, dt);
    if (m.d) { ticks[i] = TICK; m.d = 0; }
  }
}
function accNeqAll(w, dt) {
  const m = w.accNeq, n = w.n, ticks = w.ticks;
  for (let i = 0; i < n; i++) {
    m.b = i * STRIDE; userAll(i, m, dt);
    if (m.d) { ticks[i] = TICK; m.d = 0; }
  }
}
function accCmdAll(w, dt) {
  const m = w.accCmd, n = w.n;
  w.cursor = 0;
  for (let i = 0; i < n; i++) { m.b = i * STRIDE; m.i = i; userAll(i, m, dt); }
}

// ---------- post-run passes (what Rust would do; measured here for scale) ----------

function markAll(w) { w.ticks.fill(TICK); } // option A
function shadowCopy(w) { w.shadow.set(w.u32); } // option D, before the run
function shadowDiff(w) { // option D, after the run: per-row compare, tick on change
  const { u32, shadow, n, ticks } = w;
  for (let i = 0; i < n; i++) {
    const b = i * STRIDE;
    if (u32[b] !== shadow[b] || u32[b + 1] !== shadow[b + 1] || u32[b + 2] !== shadow[b + 2] ||
        u32[b + 3] !== shadow[b + 3] || u32[b + 4] !== shadow[b + 4] || u32[b + 5] !== shadow[b + 5] ||
        u32[b + 6] !== shadow[b + 6] || u32[b + 7] !== shadow[b + 7]) ticks[i] = TICK;
  }
}
function bitScan(w) { // option B with a separate bitset: Rust turns bits into ticks
  const { bits, ticks } = w;
  for (let wi = 0; wi < bits.length; wi++) {
    let word = bits[wi];
    while (word !== 0) {
      const t = word & -word;
      const bit = 31 - Math.clz32(t);
      ticks[(wi << 5) + bit] = TICK;
      word ^= t;
    }
  }
  bits.fill(0);
}
function applyCmds(w) { // option C, the Rust half: decode ops, write column and tick
  const { arenaU32, arenaF32, f32, ticks, cursor } = w;
  for (let c = 0; c < cursor; c += 4) {
    const row = arenaU32[c + 1], off = arenaU32[c + 2] >>> 2;
    f32[row * STRIDE + off] = arenaF32[c + 3];
    ticks[row] = TICK;
  }
}

// ---------- harness ----------

function bench(name, fn, w, minReps, budgetMs) {
  const dt = 1 / 60;
  // warm up by time so V8 reaches TurboFan before anything is measured
  const w0 = now();
  let warm = 0;
  while ((now() - w0 < budgetMs / 3 && warm < 2000) || warm < 3) { fn(w, dt); warm++; }
  const times = [];
  const t0 = now();
  while (times.length < minReps || (now() - t0 < budgetMs && times.length < 400)) {
    const a = now(); fn(w, dt); times.push(now() - a);
  }
  times.sort((p, q) => p - q);
  return { name, ms: times[times.length >> 1], reps: times.length };
}

const PATHS = [
  ["cond", "raw", rawCond],
  ["cond", "raw+tick", rawTickCond],
  ["cond", "raw+tick×3", rawTickPerFieldCond],
  ["cond", "raw+bit", rawBitCond],
  ["cond", "acc", accPlainCond],
  ["cond", "acc+tick", accTickCond],
  ["cond", "acc+dirty", accDirtyCond],
  ["cond", "acc+cmd", accCmdCond],
  ["all", "raw", rawAll],
  ["all", "raw+tick", rawTickAll],
  ["all", "raw+neq", rawNeqAll],
  ["all", "acc", accPlainAll],
  ["all", "acc+tick", accTickAll],
  ["all", "acc+dirty", accDirtyAll],
  ["all", "acc+neq", accNeqAll],
  ["all", "acc+cmd", accCmdAll],
  ["post", "markAll(fill)", markAll],
  ["post", "shadowCopy", shadowCopy],
  ["post", "shadowDiff", shadowDiff],
  ["post", "bitScan", (w) => { rawBitCond(w, 0); bitScan(w); }],
  ["post", "applyCmds", (w) => { accCmdCond(w, 1 / 60); applyCmds(w); }],
];

const fmt = (ms) => ms >= 100 ? ms.toFixed(0) : ms >= 10 ? ms.toFixed(1) : ms >= 1 ? ms.toFixed(2) : ms >= 0.01 ? ms.toFixed(3) : ms.toFixed(4);

console.log(`engine: ${engine}`);
for (const n of ROWS) {
  const header = ["pattern", "path", ...FRACS.map((f) => `${(f * 100).toFixed(1)}%`)];
  const rows = [];
  const worlds = FRACS.map((f) => {
    const w = makeWorld(n, f);
    w.accPlain = new AccPlain(w.f32);
    w.accTick = new AccTick(w.f32, w.ticks);
    w.accDirty = new AccDirty(w.f32);
    w.accNeq = new AccNeq(w.f32);
    w.accCmd = new AccCmd(w);
    return w;
  });
  const minReps = n >= 1000000 ? 5 : 9;
  const budget = (n >= 1000000 ? 1500 : 600) * BUDGET;
  for (const [pattern, name, fn] of PATHS) {
    const cells = worlds.map((w) => fmt(bench(name, fn, w, minReps, budget).ms));
    rows.push([pattern, name, ...cells]);
  }
  console.log(`\n### ${n.toLocaleString("en-US")} rows, ms per run (median)\n`);
  console.log(`| ${header.join(" | ")} |`);
  console.log(`|${header.map(() => "---").join("|")}|`);
  for (const r of rows) console.log(`| ${r.join(" | ")} |`);
}
