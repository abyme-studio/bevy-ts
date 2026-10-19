// Iteration forms for a Bevy-shaped TS query, over the same flyweight accessors bridge uses.
// `deno run --v8-flags=--expose-gc bench/dx/iter-forms.js`
//
// A query of [Enemy (read), Transform (write)] over N rows in one table. Each form moves
// translation by speed * dt and stores the row's change tick, as bridge decision 6 does.

const N = Number(Deno.args[0] ?? 1_000_000);
const RUNS = 60;

// Columns: Enemy { speed: f32 } (4 bytes), Transform as 10 f32 (40 bytes), ticks u32.
const enemyF32 = new Float32Array(N);
const tf = new Float32Array(N * 10);
const ticks = new Uint32Array(N);
for (let i = 0; i < N; i++) enemyF32[i] = 1 + (i % 7);

class EnemyRef {
  constructor() { this.row = 0; }
  get speed() { return enemyF32[this.row]; }
}
class Vec3View {
  constructor(base) { this.row = 0; this.base = base; }
  get x() { return tf[this.row * 10 + this.base]; }
  set x(v) { tf[this.row * 10 + this.base] = v; ticks[this.row] = TICK; }
  get y() { return tf[this.row * 10 + this.base + 1]; }
  set y(v) { tf[this.row * 10 + this.base + 1] = v; ticks[this.row] = TICK; }
  get z() { return tf[this.row * 10 + this.base + 2]; }
  set z(v) { tf[this.row * 10 + this.base + 2] = v; ticks[this.row] = TICK; }
  // glam-like in-place op, allocation-free
  addScaled(x, y, z, s) {
    const o = this.row * 10 + this.base;
    tf[o] += x * s; tf[o + 1] += y * s; tf[o + 2] += z * s;
    ticks[this.row] = TICK;
    return this;
  }
}
class TransformMut {
  constructor() { this._row = 0; this.translation = new Vec3View(0); }
  set row(r) { this._row = r; this.translation.row = r; }
}
let TICK = 1;

const enemy = new EnemyRef();
const t = new TransformMut();
const tuple = [0, enemy, t];

// A. q.each((e, enemy, t) => ...)
function each(f) {
  for (let i = 0; i < N; i++) { enemy.row = i; t.row = i; f(i, enemy, t); }
}
// B. for (const [e, enemy, t] of q): one iterator object and one result object per run,
//    the same tuple array every row.
function iter() {
  let i = -1;
  const res = { value: tuple, done: false };
  return {
    next() {
      i++;
      if (i >= N) { res.done = true; res.value = undefined; return res; }
      enemy.row = i; t.row = i; tuple[0] = i;
      return res;
    },
    [Symbol.iterator]() { return this; },
  };
}
// C. for (const i of q.rows()) with accessors re-pointed: rows() yields the row index only
//    and the accessors are params captured once.
// D. raw typed arrays (q.tables), the floor.

const forms = {
  "A each(cb)": (dt) => each((e, en, tr) => { tr.translation.y += en.speed * dt; }),
  "B for-of destructured": (dt) => { for (const [e, en, tr] of iter()) tr.translation.y += en.speed * dt; },
  "B' for-of, no destructure": (dt) => { for (const row of iter()) row[2].translation.y += row[1].speed * dt; },
  "A+ each, glam-like addScaled": (dt) => each((e, en, tr) => { tr.translation.addScaled(0, en.speed, 0, dt); }),
  "D raw arrays": (dt) => { for (let i = 0; i < N; i++) { tf[i * 10 + 1] += enemyF32[i] * dt; ticks[i] = TICK; } },
};

const results = [];
for (const [name, f] of Object.entries(forms)) {
  for (let w = 0; w < 10; w++) f(0.016);
  const times = [];
  globalThis.gc?.();
  const heap0 = Deno.memoryUsage().heapUsed;
  for (let r = 0; r < RUNS; r++) {
    TICK++;
    const t0 = performance.now();
    f(0.016);
    times.push(performance.now() - t0);
  }
  const heap1 = Deno.memoryUsage().heapUsed;
  times.sort((a, b) => a - b);
  results.push({ name, median: times[RUNS >> 1], p90: times[Math.floor(RUNS * 0.9)], heapKB: Math.round((heap1 - heap0) / 1024) });
}
console.log(`N=${N}, ${RUNS} runs, ${navigator.userAgent}`);
console.log("| form | median ms | p90 ms | heap growth KB over runs |");
console.log("|---|---|---|---|");
for (const r of results) console.log(`| ${r.name} | ${r.median.toFixed(3)} | ${r.p90.toFixed(3)} | ${r.heapKB} |`);
