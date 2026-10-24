// A table from results/*.jsonl: mean ± sd of each label, arm and size.
//   node summary.mjs [label...]
import { readdirSync, readFileSync } from "node:fs";
import { join } from "node:path";
const dir = join(import.meta.dirname, "results");
const labels = process.argv.slice(2).length ? process.argv.slice(2) : readdirSync(dir).filter((f) => f.endsWith(".jsonl")).map((f) => f.slice(0, -6));
const stat = (xs) => {
  if (!xs.length) return "-";
  const m = xs.reduce((a, b) => a + b, 0) / xs.length;
  const sd = Math.sqrt(xs.reduce((a, x) => a + (x - m) ** 2, 0) / Math.max(1, xs.length - 1));
  const f = (x) => (m >= 100 ? x.toFixed(0) : x.toFixed(1));
  return `${f(m)} ± ${f(sd)}`;
};
console.log("| label | arm | rows | runs | ms | frames | longest frame ms | load 1m | errors |");
console.log("|---|---|---|---|---|---|---|---|---|");
for (const label of labels) {
  const rows = readFileSync(join(dir, `${label}.jsonl`), "utf8").trim().split("\n").filter(Boolean).map((l) => JSON.parse(l));
  const groups = new Map();
  for (const r of rows) {
    const k = `${r.config.arm}|${r.config.n}`;
    if (!groups.has(k)) groups.set(k, []);
    groups.get(k).push(r);
  }
  for (const [k, rs] of groups) {
    const [arm, n] = k.split("|");
    const ok = rs.filter((r) => !r.error);
    const err = [...new Set(rs.filter((r) => r.error).map((r) => r.error.slice(0, 60)))];
    console.log(
      `| ${label} | ${arm} | ${Number(n).toLocaleString("en")} | ${rs.length} | ${stat(ok.map((r) => r.ms))} | ${stat(ok.map((r) => r.frames))} | ${stat(ok.map((r) => r.longest))} | ${stat(rs.map((r) => r.load?.[0]).filter(Number.isFinite))} | ${err.join("; ")} |`,
    );
  }
}
