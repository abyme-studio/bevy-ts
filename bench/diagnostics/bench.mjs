// Every arm at 10k and 100k entities, 3 runs each (8 with --runs 8), one fresh engine per run
// (run.mjs): no plugin, then each diagnostics plugin alone. Appends to results/<label>.jsonl and
// skips runs already there, so it resumes.
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
const ARMS = [
  "none",
  "FrameTimeDiagnosticsPlugin",
  "EntityCountDiagnosticsPlugin",
  "SystemInformationDiagnosticsPlugin",
  "RenderDiagnosticsPlugin",
  "MeshAllocatorDiagnosticPlugin",
  "RenderAssetDiagnosticPlugin<RenderMesh>",
  "ScriptStageDiagnosticsPlugin",
];
const SIZES = [1e4, 1e5];
const TIMEOUT = 300;
const only = filter ? new RegExp(filter) : null;
const rows = existsSync(file) ? readFileSync(file, "utf8").trim().split("\n").filter(Boolean).map((l) => JSON.parse(l)) : [];
const arm = (r) => r.plugins[0] ?? "none";
for (const a of ARMS) {
  if (only && !only.test(a)) continue;
  for (const n of SIZES) {
    const have = rows.filter((r) => arm(r) === a && r.config.n === n);
    for (let k = have.length; k < RUNS; k++) {
      const plugins = a === "none" ? [] : [a];
      const r = spawnSync("node", [join(root, "run.mjs"), bin, JSON.stringify({ n }), JSON.stringify(plugins), String(TIMEOUT)], { encoding: "utf8", timeout: (TIMEOUT + 60) * 1000 });
      let out;
      try {
        out = JSON.parse(r.stdout.trim().split("\n").pop());
      } catch {
        out = { config: { n }, plugins, error: `no result: ${r.error ?? ""} ${r.stderr?.slice(-500)}` };
      }
      const row = { label, run: k, at: new Date().toISOString(), ...out };
      appendFileSync(file, JSON.stringify(row) + "\n");
      rows.push(row);
      console.log(`${label} | ${a} ${n} | run ${k}: main ${out.main ?? "-"} ms, frame ${out.frame ?? "-"} ms; load ${out.load?.[0]}${out.error ? `; ERROR ${out.error}` : ""}`);
      if (out.error) break;
    }
  }
}
