// Deno Web Workers are deno_core JsRuntimes on threads of their own: worker startup, memory per
// worker, a postMessage round trip, and one column loop split across workers over one
// SharedArrayBuffer (no copy). Run: deno run -A bench/threading/js/workers.js
const median = (v) => v.sort((a, b) => a - b)[v.length >> 1];
const src = `
const fns = []; for (let i = 0; i < 5000; i++) fns.push(new Function("a", "b", "let x = a * " + i + " + b; for (let k = 0; k < 3; k++) x += k; return x;"));
let pos, vel;
self.onmessage = (e) => {
  const m = e.data;
  if (m.t === "echo") return self.postMessage(m.b);
  if (m.t === "init") { pos = new Float32Array(m.pos, m.off * 4, m.len); vel = new Float32Array(m.vel, m.off * 4, m.len); return self.postMessage(0); }
  if (m.t === "run") { for (let i = 0; i < pos.length; i++) pos[i] += vel[i] * 0.016; return self.postMessage(1); }
};
self.postMessage("ready");`;
const url = URL.createObjectURL(new Blob([src], { type: "application/javascript" }));
const next = (w) => new Promise((r) => (w.onmessage = (e) => r(e.data)));
const spawn = async () => { const w = new Worker(url, { type: "module" }); await next(w); return w; };

const rss = () => Deno.memoryUsage().rss / 2 ** 20;
const starts = [];
for (let i = 0; i < 10; i++) { const t = performance.now(); const w = await spawn(); starts.push(performance.now() - t); w.terminate(); }
console.log(`worker spawn to ready (5000 compiled fns), median of 10: ${median(starts).toFixed(2)} ms`);

await new Promise((r) => setTimeout(r, 200));
const before = rss();
const ws = [];
for (let i = 0; i < 16; i++) ws.push(await spawn());
await new Promise((r) => setTimeout(r, 200));
console.log(`16 live workers add ${(rss() - before).toFixed(1)} MB RSS, ${((rss() - before) / 16).toFixed(1)} MB each`);

for (const size of [64, 65536]) {
  const b = new Uint8Array(size); const n = 5000; const t = performance.now();
  for (let i = 0; i < n; i++) { const p = next(ws[0]); ws[0].postMessage({ t: "echo", b }); await p; }
  console.log(`postMessage round trip with ${size} B: ${((performance.now() - t) * 1000 / n).toFixed(1)} us`);
}

const N = 4 << 20;
const posB = new SharedArrayBuffer(N * 4), velB = new SharedArrayBuffer(N * 4);
new Float32Array(velB).fill(1);
for (const k of [1, 2, 4, 8]) {
  const chunk = N / k;
  await Promise.all(ws.slice(0, k).map((w, i) => { const p = next(w); w.postMessage({ t: "init", pos: posB, vel: velB, off: i * chunk, len: chunk }); return p; }));
  const times = [];
  for (let f = 0; f < 45; f++) {
    const t = performance.now();
    await Promise.all(ws.slice(0, k).map((w) => { const p = next(w); w.postMessage({ t: "run" }); return p; }));
    if (f >= 5) times.push(performance.now() - t);
  }
  console.log(`pos += vel * dt over ${N} rows, ${k} worker(s): ${median(times).toFixed(2)} ms per frame`);
}
ws.forEach((w) => w.terminate());
