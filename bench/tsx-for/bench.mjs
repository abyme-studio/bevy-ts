// Every arm at 1k, 10k and 100k, 3 runs each (8 with --runs 8), one fresh engine per run (run.mjs).
// Appends to results/<label>.jsonl and skips runs already there, so it resumes. A size that times
// out is not run again, nor any larger size of that arm.
//
//   node bench.mjs <label> <engine binary> [arm-filter-regex] [--runs N]
import { spawnSync } from "node:child_process";
import { appendFileSync, existsSync, readFileSync } from "node:fs";
import { join } from "node:path";

const root = import.meta.dirname;
const args = process.argv.slice(2);
const runsAt = args.indexOf("--runs");
const RUNS = runsAt >= 0 ? Number(args.splice(runsAt, 2)[1]) : 3;
const [label, bin, filter] = args;
if (!label || !bin) throw new Error("usage: node bench.mjs <label> <engine binary> [arm-filter] [--runs N]");
const file = join(root, "results", `${label}.jsonl`);
const ARMS = ["grow", "grow1k", "reverse", "clear", "spawn"];
const SIZES = [1e3, 1e4, 1e5];
const TIMEOUT = 600;
const only = filter ? new RegExp(filter) : null;
const rows = existsSync(file) ? readFileSync(file, "utf8").trim().split("\n").filter(Boolean).map((l) => JSON.parse(l)) : [];
const have = (arm, n) => rows.filter((r) => r.config.arm === arm && r.config.n === n);
for (const arm of ARMS) {
  if (only && !only.test(arm)) continue;
  for (const n of SIZES) {
    if (have(arm, n).some((r) => r.error)) break;
    let failed = false;
    for (let k = have(arm, n).length; k < RUNS && !failed; k++) {
      const r = spawnSync("node", [join(root, "run.mjs"), bin, JSON.stringify({ n, arm }), String(TIMEOUT)], { encoding: "utf8", timeout: (TIMEOUT + 60) * 1000 });
      let out;
      try {
        out = JSON.parse(r.stdout.trim().split("\n").pop());
      } catch {
        out = { config: { n, arm }, error: `no result: ${r.error ?? ""} ${r.stderr?.slice(-500)}` };
      }
      const row = { label, run: k, at: new Date().toISOString(), ...out };
      appendFileSync(file, JSON.stringify(row) + "\n");
      rows.push(row);
      console.log(`${label} | ${arm} ${n} | run ${k}: ${out.ms ?? "-"} ms, ${out.frames ?? "-"} frames, longest ${out.longest ?? "-"} ms; load ${out.load?.[0]}${out.error ? `; ERROR ${out.error}` : ""}`);
      failed = !!out.error;
    }
    if (failed) break;
  }
}
