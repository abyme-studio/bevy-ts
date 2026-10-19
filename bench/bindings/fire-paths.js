// Scene bindings against a TS system: the JS half of each path, per frame.
//
//   deno run bench/bindings/fire-paths.js [rows=1000,10000,100000] [fracs=0.001,0.01,0.1,1]
//   node bench/bindings/fire-paths.js
//   deno run --v8-flags=--jitless bench/bindings/fire-paths.js
//
// Model: N entities, each with Alarm { on, level } and Light { intensity, range, radius, pad }
// in one table (f32 columns, a Uint32Array of Bevy change ticks beside each). Each frame k of the
// N alarms changed. Two ways to set every light's intensity from its alarm:
//
//   binding  `<PointLight intensity={() => alarm().on ? 8 : 2} />` per entity (tsx design decision
//            3 and 9). Rust checked the N subscriptions' ticks, found k moved, and made one host
//            call carrying k (subscription id, 8 bytes). JS decodes each value into a snapshot,
//            sets the subscription's signal, runs the effects that read it (Solid-style, deps
//            re-tracked per run), and each effect that produced a new value appends one 12-byte
//            patch op (node, component, field, f32). Measured: the host call's JS body.
//            Two forms: a fresh snapshot object per delivered value (what a naive useComponent
//            does; the old snapshot becomes garbage) and a Float32Array slot per subscription.
//   system   a TS system over the Alarm/Light table (bridge design decisions 5 and 6): one call
//            per frame, a loop over N rows reading the Alarm tick, writing intensity through the
//            flyweight accessor setter, which stores the tick. Measured: the loop.
//   system-all  the same system with no tick check, writing every row every frame.
//
// Also measured per N: mounting N bindings (signal + effect + closure) and the heap they keep.
//
// Output: markdown tables, median per frame, in microseconds.

const TICK = 1_234_567;
const A_STRIDE = 2; // Alarm f32 slots
const L_STRIDE = 4; // Light f32 slots

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

const heapUsed = () => {
  if (typeof Deno !== "undefined") return Deno.memoryUsage().heapUsed;
  if (typeof process !== "undefined") return process.memoryUsage().heapUsed;
  return 0;
};
const gc = () => {
  if (typeof globalThis.gc === "function") globalThis.gc();
};

const ROWS = (argv[0] ?? "1000,10000,100000").split(",").map(Number);
const FRACS = (argv[1] ?? "0.001,0.01,0.1,1").split(",").map(Number);
const BUDGET_MS = Number(argv[2] ?? 60); // per cell, split over samples

let seed = 0x9e3779b9;
function rnd() {
  seed ^= seed << 13; seed >>>= 0;
  seed ^= seed >>> 17;
  seed ^= seed << 5; seed >>>= 0;
  return seed / 4294967296;
}
function shuffled(n) {
  const a = new Uint32Array(n);
  for (let i = 0; i < n; i++) a[i] = i;
  for (let i = n - 1; i > 0; i--) {
    const j = Math.floor(rnd() * (i + 1));
    const t = a[i]; a[i] = a[j]; a[j] = t;
  }
  return a;
}

// ---- reactive core (the shape of tsx decision 3: signals, effects, re-tracked deps) ----

let observer = null;
const queue = [];

class Signal {
  constructor(v) { this.v = v; this.subs = new Set(); }
  get() {
    if (observer !== null) { observer.deps.push(this); this.subs.add(observer); }
    return this.v;
  }
  set(v) {
    if (v === this.v) return;
    this.v = v;
    for (const e of this.subs) if (!e.queued) { e.queued = true; queue.push(e); }
  }
}

class Effect {
  constructor(fn) { this.fn = fn; this.deps = []; this.queued = false; this.run(); }
  run() {
    const deps = this.deps;
    for (let i = 0; i < deps.length; i++) deps[i].subs.delete(this);
    deps.length = 0;
    const prev = observer;
    observer = this;
    try { this.fn(); } finally { observer = prev; }
  }
}

function flush() {
  for (let i = 0; i < queue.length; i++) { const e = queue[i]; e.queued = false; e.run(); }
  queue.length = 0;
}

// ---- patch op stream: u32 node, u16 component, u16 field, f32 value ----

class Ops {
  constructor(cap) {
    this.buf = new ArrayBuffer(cap * 12);
    this.u32 = new Uint32Array(this.buf);
    this.f32 = new Float32Array(this.buf);
    this.n = 0;
  }
  patchF32(node, comp, field, v) {
    const i = this.n * 3;
    this.u32[i] = node;
    this.u32[i + 1] = (comp << 16) | field;
    this.f32[i + 2] = v;
    this.n++;
  }
}

// ---- accessors (bridge decision 5, V8 class form) ----

class AlarmAcc {
  constructor(f32) { this.f32 = f32; this.b = 0; }
  get on() { return this.f32[this.b]; }
  get level() { return this.f32[this.b + 1]; }
}
class LightAcc {
  constructor(f32, ticks) { this.f32 = f32; this.ticks = ticks; this.b = 0; this.r = 0; }
  get intensity() { return this.f32[this.b]; }
  set intensity(v) { this.f32[this.b] = v; this.ticks[this.r] = TICK; }
}

function makeWorld(n) {
  const alarm = new Float32Array(n * A_STRIDE);
  const alarmTicks = new Uint32Array(n);
  const light = new Float32Array(n * L_STRIDE);
  const lightTicks = new Uint32Array(n);
  for (let r = 0; r < n; r++) {
    alarm[r * A_STRIDE] = rnd() < 0.5 ? 1 : 0;
    alarm[r * A_STRIDE + 1] = rnd();
    light[r * L_STRIDE] = alarm[r * A_STRIDE] ? 8 : 2;
    alarmTicks[r] = TICK - 1000;
  }
  return { n, alarm, alarmTicks, light, lightTicks, order: shuffled(n) };
}

// ---- the binding path ----

function mountBindings(w) {
  const n = w.n;
  const signals = new Array(n);
  const effects = new Array(n);
  const last = new Float32Array(n);
  const ops = new Ops(n);
  for (let i = 0; i < n; i++) {
    const node = 0x1000 + i;
    const b = i * A_STRIDE;
    const sig = new Signal({ on: w.alarm[b], level: w.alarm[b + 1] });
    signals[i] = sig;
    effects[i] = new Effect(() => {
      const v = sig.get().on ? 8 : 2;
      if (v !== last[i]) { last[i] = v; ops.patchF32(node, 7, 0, v); }
    });
  }
  ops.n = 0;
  return { signals, effects, last, ops };
}

// The same bindings with the subscription's value kept in a Float32Array slot instead of a fresh
// object per delivery: the signal only versions, the effect reads the slot. No allocation per fire.
function mountBindingsTyped(w) {
  const n = w.n;
  const vals = new Float32Array(n * A_STRIDE);
  vals.set(w.alarm);
  const signals = new Array(n);
  const effects = new Array(n);
  const last = new Float32Array(n);
  const ops = new Ops(n);
  for (let i = 0; i < n; i++) {
    const node = 0x1000 + i;
    const b = i * A_STRIDE;
    const sig = new Signal(0);
    signals[i] = sig;
    effects[i] = new Effect(() => {
      sig.get();
      const v = vals[b] ? 8 : 2;
      if (v !== last[i]) { last[i] = v; ops.patchF32(node, 7, 0, v); }
    });
  }
  ops.n = 0;
  return { signals, effects, last, ops, vals };
}

function bindingFrameTyped(m, d, flipped) {
  const { ids, bytes, k } = d;
  const { signals, vals } = m;
  m.ops.n = 0;
  for (let j = 0; j < k; j++) {
    const b = j * A_STRIDE, s = ids[j], vb = s * A_STRIDE;
    vals[vb] = flipped ? 1 - bytes[b] : bytes[b];
    vals[vb + 1] = bytes[b + 1];
    const sig = signals[s];
    sig.set(sig.v + 1);
  }
  flush();
  return m.ops.n;
}

// Rust's delivery: k changed subscriptions, as (sub id) + (8 bytes of Alarm).
function makeDelivery(w, k, flip) {
  const ids = new Uint32Array(k);
  const bytes = new Float32Array(k * A_STRIDE);
  for (let j = 0; j < k; j++) {
    const r = w.order[j];
    ids[j] = r;
    const on = w.alarm[r * A_STRIDE];
    bytes[j * A_STRIDE] = flip ? 1 - on : on;
    bytes[j * A_STRIDE + 1] = rnd();
  }
  return { ids, bytes, k };
}

// The host call body. `flip` alternates the delivered value so every effect produces a patch.
function bindingFrame(m, d, flipped) {
  const { ids, bytes, k } = d;
  const signals = m.signals;
  m.ops.n = 0;
  for (let j = 0; j < k; j++) {
    const b = j * A_STRIDE;
    const on = flipped ? 1 - bytes[b] : bytes[b];
    signals[ids[j]].set({ on, level: bytes[b + 1] });
  }
  flush();
  return m.ops.n;
}

// Same delivery, but `on` never differs: effects run, nothing is patched.
function bindingFrameSame(m, d) {
  const { ids, bytes, k } = d;
  const signals = m.signals;
  m.ops.n = 0;
  for (let j = 0; j < k; j++) {
    const b = j * A_STRIDE;
    const cur = signals[ids[j]].v;
    signals[ids[j]].set({ on: cur.on, level: bytes[b + 1] });
  }
  flush();
  return m.ops.n;
}

// ---- the system path ----

function markChanged(w, k, tick) {
  for (let j = 0; j < k; j++) w.alarmTicks[w.order[j]] = tick;
}

function systemChanged(w, lastRun, a, l) {
  const n = w.n, ticks = w.alarmTicks;
  let wrote = 0;
  for (let r = 0; r < n; r++) {
    if (ticks[r] > lastRun) {
      a.b = r * A_STRIDE; l.b = r * L_STRIDE; l.r = r;
      l.intensity = a.on ? 8 : 2;
      wrote++;
    }
  }
  return wrote;
}

function systemAll(w, a, l) {
  const n = w.n;
  for (let r = 0; r < n; r++) {
    a.b = r * A_STRIDE; l.b = r * L_STRIDE; l.r = r;
    l.intensity = a.on ? 8 : 2;
  }
  return n;
}

// ---- harness ----

function median(xs) { xs.sort((x, y) => x - y); return xs[xs.length >> 1]; }

// Runs `frame()` repeatedly; returns median microseconds per frame.
function measure(frame) {
  // calibrate frames per sample so a sample is ~1 ms
  let per = 1;
  for (;;) {
    const t0 = now();
    for (let i = 0; i < per; i++) frame(i);
    const dt = now() - t0;
    if (dt >= 1 || per >= 1 << 20) break;
    per *= 2;
  }
  const samples = [];
  const end = now() + BUDGET_MS;
  while (now() < end || samples.length < 7) {
    const t0 = now();
    for (let i = 0; i < per; i++) frame(i);
    samples.push(((now() - t0) / per) * 1000);
  }
  return median(samples);
}

const fmt = (x) => x >= 1000 ? x.toFixed(0) : x >= 100 ? x.toFixed(1) : x >= 10 ? x.toFixed(2) : x.toFixed(3);

console.log(`engine: ${engine}`);
console.log(`budget per cell: ${BUDGET_MS} ms; medians in µs per frame\n`);

const mountRows = [];
const fireRows = [];
for (const n of ROWS) {
  const w = makeWorld(n);

  gc();
  const h0 = heapUsed();
  const t0 = now();
  const m = mountBindings(w);
  const mountMs = now() - t0;
  gc();
  const h1 = heapUsed();
  mountRows.push({ n, mountMs, bytesPer: (h1 - h0) / n });

  const a = new AlarmAcc(w.alarm);
  const l = new LightAcc(w.light, w.lightTicks);
  const mt = mountBindingsTyped(w);

  for (const frac of FRACS) {
    const k = Math.max(1, Math.round(n * frac));
    const d = makeDelivery(w, k, false);

    const bind = measure((i) => bindingFrame(m, d, (i & 1) === 1));
    const bindTyped = measure((i) => bindingFrameTyped(mt, d, (i & 1) === 1));
    const bindSame = measure(() => bindingFrameSame(m, d));

    // k alarms carry a tick newer than the system's last run; the system scans N and writes k.
    w.alarmTicks.fill(TICK - 1000);
    markChanged(w, k, TICK);
    const sys = measure(() => systemChanged(w, TICK - 1, a, l));
    const sysAll = measure(() => systemAll(w, a, l));

    fireRows.push({ n, frac, k, bind, bindTyped, bindSame, sys, sysAll });
  }
}

console.log("| N bindings | mount ms | heap bytes per binding |");
console.log("|---|---|---|");
for (const r of mountRows) console.log(`| ${r.n} | ${r.mountMs.toFixed(1)} | ${r.bytesPer.toFixed(0)} |`);
console.log();
console.log("| N | changed k | binding, snapshot objects: k fire, k patch | binding, typed slots: k fire, k patch | binding, snapshot: k fire, same value | system: scan N, write k | system-all: write N |");
console.log("|---|---|---|---|---|---|---|");
for (const r of fireRows) {
  console.log(`| ${r.n} | ${r.k} (${r.frac * 100}%) | ${fmt(r.bind)} | ${fmt(r.bindTyped)} | ${fmt(r.bindSame)} | ${fmt(r.sys)} | ${fmt(r.sysAll)} |`);
}
