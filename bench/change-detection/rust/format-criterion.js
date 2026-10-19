// Renders `cargo bench | grep -E "^[a-z_]+/|time:"` output as a markdown table in ms.
//   deno run --allow-read bench/change-detection/rust/format-criterion.js bench/change-detection/results/rust-criterion.txt
const text = await Deno.readTextFile(Deno.args[0]);
const lines = text.split("\n");
const t = new Map();
let bench = null, key = null;
for (const line of lines) {
  const head = line.match(/^([a-z_]+)\/(\d+k)\/([\d.]+%)/);
  if (head) { bench = head[1]; key = `${head[2]}/${head[3]}`; }
  const m = line.match(/time:\s+\[[\d.]+ \S+ ([\d.]+) (ns|µs|ms)/);
  if (m && bench) {
    const v = Number(m[1]);
    const ms = m[2] === "ns" ? v / 1e6 : m[2] === "µs" ? v / 1e3 : v;
    t.set(`${bench} ${key}`, ms);
    bench = null;
  }
}
const B = ["mark_all", "bits_to_ticks", "shadow_copy", "shadow_diff", "shadow_diff_pages", "apply_cmds", "changed_scan", "hash_column"];
const K = ["10k/0.3%", "10k/10%", "10k/100%", "100k/0.3%", "100k/10%", "100k/100%", "1000k/0.3%", "1000k/10%", "1000k/100%"];
const fmt = (x) => x === undefined ? "" : x >= 10 ? x.toFixed(1) : x >= 1 ? x.toFixed(2) : x >= 0.01 ? x.toFixed(3) : x.toFixed(4);
console.log(`| pass | ${K.map((k) => k.replace("1000k", "1M")).join(" | ")} |`);
console.log(`|---|${K.map(() => "---").join("|")}|`);
for (const b of B) console.log(`| ${b} | ${K.map((k) => fmt(t.get(`${b} ${k}`))).join(" | ")} |`);
