// A table from results/<label>.jsonl: each arm's main-world time (mean ± sd, median), its cost over
// `none` by median, and that cost per line (1,000 lines a frame); and the time inside the drawing
// system (`sys`, median), per call: the calls' own cost, free of frame noise.
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
  if (!groups.has(r.config.arm)) groups.set(r.config.arm, []);
  groups.get(r.config.arm).push(r);
}
const none = median((groups.get("none") ?? []).filter((r) => !r.error).map((r) => r.main));
console.log("| arm | runs | main ms | main median | over none, ms | ns per line | sys ms | sys ns per call | load 1m | errors |");
console.log("|---|---|---|---|---|---|---|---|---|---|");
for (const [arm, rs] of groups) {
  const ok = rs.filter((r) => !r.error);
  const med = median(ok.map((r) => r.main));
  const over = arm === "none" ? NaN : med - none;
  const sys = median(ok.map((r) => r.sys).filter(Number.isFinite));
  const err = [...new Set(rs.filter((r) => r.error).map((r) => r.error.slice(0, 60)))].join("; ");
  console.log(`| ${arm} | ${rs.length} | ${fmt(stat(ok.map((r) => r.main)))} | ${med.toFixed(3)} | ${Number.isFinite(over) ? over.toFixed(3) : "-"} | ${Number.isFinite(over) ? Math.round(over * 1000) : "-"} | ${Number.isFinite(sys) ? sys.toFixed(4) : "-"} | ${arm === "none" || !Number.isFinite(sys) ? "-" : Math.round(sys * 1000)} | ${fmt(stat(rs.map((r) => r.load?.[0]).filter(Number.isFinite)))} | ${err} |`);
}
