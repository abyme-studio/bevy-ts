// A table from results/<label>.jsonl: mean ± sd main-world and frame time per arm and size, and
// each arm's main-world cost over the no-plugin arm.
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
const fmt = ([m, sd]) => (Number.isFinite(m) ? `${m.toFixed(3)} ± ${sd.toFixed(3)}` : "-");
const groups = new Map();
for (const r of rows) {
  const k = `${r.plugins[0] ?? "none"}|${r.config.n}`;
  if (!groups.has(k)) groups.set(k, []);
  groups.get(k).push(r);
}
console.log("| plugin | entities | runs | main ms | over none, ms | frame ms | load 1m | errors |");
console.log("|---|---|---|---|---|---|---|---|");
for (const [k, rs] of groups) {
  const [arm, n] = k.split("|");
  const ok = rs.filter((r) => !r.error);
  const main = stat(ok.map((r) => r.main));
  const none = stat((groups.get(`none|${n}`) ?? []).filter((r) => !r.error).map((r) => r.main));
  const over = arm === "none" ? "-" : (main[0] - none[0]).toFixed(3);
  const err = [...new Set(rs.filter((r) => r.error).map((r) => r.error.slice(0, 60)))].join("; ");
  console.log(`| ${arm} | ${Number(n).toLocaleString("en")} | ${rs.length} | ${fmt(main)} | ${over} | ${fmt(stat(ok.map((r) => r.frame)))} | ${fmt(stat(rs.map((r) => r.load?.[0]).filter(Number.isFinite)))} | ${err} |`);
}
