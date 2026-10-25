// A table from results/<label>.jsonl: each arm's median (of the runs' medians), worst p95, and the
// pass lines (put64k under 50 µs a call, read10k under 2 ms a frame).
//   node summary.mjs <label>
import { readFileSync } from "node:fs";
import { join } from "node:path";
const label = process.argv[2] ?? "after";
const rows = readFileSync(join(import.meta.dirname, "results", `${label}.jsonl`), "utf8").trim().split("\n").filter(Boolean).map((l) => JSON.parse(l));
const median = (xs) => {
  const s = [...xs].sort((a, b) => a - b);
  return s.length ? (s.length % 2 ? s[(s.length - 1) / 2] : (s[s.length / 2 - 1] + s[s.length / 2]) / 2) : NaN;
};
const PASS = { put64k: 50, read10k: 2 };
const arms = new Map();
for (const r of rows.filter((r) => !r.error)) {
  if (!arms.has(r.arm)) arms.set(r.arm, []);
  arms.get(r.arm).push(r);
}
console.log("| arm | unit | runs | median | p95 (worst run) | loop mean | pass |");
console.log("|---|---|---|---|---|---|---|");
for (const [arm, rs] of arms) {
  const m = median(rs.map((r) => r.median));
  const p95 = Math.max(...rs.map((r) => r.p95));
  const loop = rs[0].loopMean === undefined ? "-" : median(rs.map((r) => r.loopMean)).toFixed(3);
  const pass = PASS[arm] === undefined ? "-" : `${Math.max(...rs.map((r) => r.median)) < PASS[arm] ? "yes" : "NO"} (< ${PASS[arm]})`;
  console.log(`| ${arm} | ${rs[0].unit} | ${rs.length} | ${m.toFixed(3)} | ${p95.toFixed(3)} | ${loop} | ${pass} |`);
}
const errors = rows.filter((r) => r.error);
if (errors.length) console.log(`\nerrors: ${errors.map((r) => r.error.slice(0, 80)).join("; ")}`);
