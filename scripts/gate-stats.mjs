#!/usr/bin/env node
// Who used the machine's check slots, from gate-slot's history (docs/check-slots.md, "Slot use"):
// per repository, its runs, the slot hours they held, its share of all slot hours, and how many failed.
//   node scripts/gate-stats.mjs [hours (72)] [--repo <name>: per run name within that repository]
// The history is $ABYME_GATE/history.jsonl (/tmp/abyme-heavy), one line per ended run.
import fs from "node:fs";
import path from "node:path";

const args = process.argv.slice(2);
const at = args.indexOf("--repo");
const repo = at >= 0 ? args.splice(at, 2)[1] : null;
const hours = Number(args[0] ?? 72);
const file = path.join(process.env.ABYME_GATE ?? "/tmp/abyme-heavy", "history.jsonl");
const since = Date.now() / 1000 - hours * 3600;

const runs = fs
  .readFileSync(file, "utf8")
  .split("\n")
  .filter(Boolean)
  .map((l) => JSON.parse(l))
  .filter((r) => r.outcome === "ran" && r.took && r.ended >= since);

const key = (r) => (repo ? r.what : path.basename(r.owner ?? "?"));
const rows = new Map();
let total = 0;
for (const r of runs) {
  const h = (r.ended - Math.max(r.took, since)) / 3600;
  total += h;
  if (repo && path.basename(r.owner ?? "") !== repo) continue;
  const row = rows.get(key(r)) ?? { runs: 0, failed: 0, hours: 0 };
  row.runs += 1;
  row.failed += r.code === 0 ? 0 : 1;
  row.hours += h;
  rows.set(key(r), row);
}

const pct = (a, b) => (b ? `${Math.round((100 * a) / b)}%` : "-");
console.log(`last ${hours} h: ${total.toFixed(1)} slot hours${repo ? `, ${repo} by run` : ""}`);
console.log(["name", "runs", "failed", "fail", "hours", "share"].join("\t"));
for (const [name, r] of [...rows].sort((a, b) => b[1].hours - a[1].hours).slice(0, repo ? 30 : undefined)) {
  console.log([name, r.runs, r.failed, pct(r.failed, r.runs), r.hours.toFixed(1), pct(r.hours, total)].join("\t"));
}
