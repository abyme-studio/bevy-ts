// Renders `cargo bench` output as markdown tables in microseconds.
//   deno run --allow-read bench/bindings/rust/format-criterion.js bench/bindings/results/rust-criterion.txt
const text = await Deno.readTextFile(Deno.args[0]);
const t = new Map();
let bench = null, key = null;
for (const line of text.split("\n")) {
  const head = line.match(/^([a-z_]+)\/(\d+k)(?:\/([\d.]+%))?/);
  if (head) { bench = head[1]; key = head[3] ? `${head[2]}/${head[3]}` : head[2]; }
  const m = line.match(/time:\s+\[[\d.]+ \S+ ([\d.]+) (ns|µs|ms)/);
  if (m && bench) {
    const v = Number(m[1]);
    const us = m[2] === "ns" ? v / 1e3 : m[2] === "µs" ? v : v * 1e3;
    t.set(`${bench} ${key}`, us);
    bench = null;
  }
}
const fmt = (x) => x === undefined ? "" : x >= 1000 ? x.toFixed(0) : x >= 100 ? x.toFixed(1) : x >= 10 ? x.toFixed(2) : x.toFixed(3);
const N = ["1k", "10k", "100k"];
const F = ["0.1%", "1%", "10%", "100%"];

console.log("Per N, µs per frame:\n");
console.log(`| pass | ${N.join(" | ")} |`);
console.log(`|---|${N.map(() => "---").join("|")}|`);
for (const b of ["tick_scan", "tick_scan_shuffled", "system_all"]) {
  console.log(`| ${b} | ${N.map((n) => fmt(t.get(`${b} ${n}`))).join(" | ")} |`);
}
console.log("\nPer N and changed fraction, µs per frame (system_changed minus mark_k is the query's own cost):\n");
const K = N.flatMap((n) => F.map((f) => `${n}/${f}`));
console.log(`| pass | ${K.join(" | ")} |`);
console.log(`|---|${K.map(() => "---").join("|")}|`);
for (const b of ["patch_apply", "mark_k", "system_changed"]) {
  console.log(`| ${b} | ${K.map((k) => fmt(t.get(`${b} ${k}`))).join(" | ")} |`);
}
console.log(`| system_changed - mark_k | ${K.map((k) => { const a = t.get(`system_changed ${k}`), m = t.get(`mark_k ${k}`); return a === undefined || m === undefined ? "" : fmt(Math.max(0, a - m)); }).join(" | ")} |`);
