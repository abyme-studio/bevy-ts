// Every arm, 3 runs each (8 with --runs 8), one fresh engine per run (run.mjs). Appends to
// results/<label>.jsonl and skips runs already there, so it resumes.
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
const ARMS = ["none", "keys", "lines_hex", "lines_srgb", "lines_array", "strip_f32"];
const TIMEOUT = 300;
const only = filter ? new RegExp(filter) : null;
const rows = existsSync(file) ? readFileSync(file, "utf8").trim().split("\n").filter(Boolean).map((l) => JSON.parse(l)) : [];
for (const arm of ARMS) {
  if (only && !only.test(arm)) continue;
  const have = rows.filter((r) => r.config.arm === arm);
  for (let k = have.length; k < RUNS; k++) {
    const r = spawnSync("node", [join(root, "run.mjs"), bin, JSON.stringify({ arm }), String(TIMEOUT)], { encoding: "utf8", timeout: (TIMEOUT + 60) * 1000 });
    let out;
    try {
      out = JSON.parse(r.stdout.trim().split("\n").pop());
    } catch {
      out = { config: { arm }, error: `no result: ${r.error ?? ""} ${r.stderr?.slice(-500)}` };
    }
    const row = { label, run: k, at: new Date().toISOString(), ...out };
    appendFileSync(file, JSON.stringify(row) + "\n");
    rows.push(row);
    console.log(`${label} | ${arm} | run ${k}: main ${out.main ?? "-"} ms, frame ${out.frame ?? "-"} ms; load ${out.load?.[0]}${out.error ? `; ERROR ${out.error}` : ""}`);
    if (out.error) break;
  }
}
