// A table from results/<label>.jsonl: mean ± sd and median main-world time, and mean frame time,
// per arm and size, and each arm's main-world cost over the no-plugin arm by median: a loaded
// machine's outlier runs move a mean by more than a plugin costs.
//   node summary.mjs <label>
import { readFileSync } from "node:fs";
import { join } from "node:path";
const label = process.argv[2] ?? "after";
const rows = readFileSync(join(import.meta.dirname, "results", `${label}.jsonl`), "utf8").trim().split("\n").filter(Boolean).map((l) => JSON.parse(l));
const stat = (xs) => {
  if (!xs.length) return [NaN, NaN];
  const m = xs.reduce((a, b) => a + b, 0) / xs.length;
  return [m, Math.sqrt(xs.reduce((a, x) => a + (x - m) ** 2, 0) / Math.max(1, xs.length - 1))];
};
const median = (xs) => {
  const s = [...xs].sort((a, b) => a - b);
  return s.length ? (s.length % 2 ? s[(s.length - 1) / 2] : (s[s.length / 2 - 1] + s[s.length / 2]) / 2) : NaN;
};
const fmt = ([m, sd]) => (Number.isFinite(m) ? `${m.toFixed(3)} ± ${sd.toFixed(3)}` : "-");
const groups = new Map();
for (const r of rows) {
  const k = `${r.plugins[0] ?? "none"}|${r.config.n}`;
  if (!groups.has(k)) groups.set(k, []);
  groups.get(k).push(r);
}
console.log("| plugin | entities | runs | main ms | main median | over none, ms (median) | frame ms | load 1m | errors |");
console.log("|---|---|---|---|---|---|---|---|---|");
for (const [k, rs] of groups) {
  const [arm, n] = k.split("|");
  const ok = rs.filter((r) => !r.error);
  const main = stat(ok.map((r) => r.main));
  const med = median(ok.map((r) => r.main));
  const none = median((groups.get(`none|${n}`) ?? []).filter((r) => !r.error).map((r) => r.main));
  const over = arm === "none" ? "-" : (med - none).toFixed(3);
  const err = [...new Set(rs.filter((r) => r.error).map((r) => r.error.slice(0, 60)))].join("; ");
  console.log(`| ${arm} | ${Number(n).toLocaleString("en")} | ${rs.length} | ${fmt(main)} | ${med.toFixed(3)} | ${over} | ${fmt(stat(ok.map((r) => r.frame)))} | ${fmt(stat(rs.map((r) => r.load?.[0]).filter(Number.isFinite)))} | ${err} |`);
}
