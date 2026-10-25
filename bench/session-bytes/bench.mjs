// The session-bytes bench: 3 runs (8 with --runs 8), one fresh headless engine each, every arm of
// project/game.ts in each. Appends one row per arm and run to results/<label>.jsonl and skips runs
// already there, so it resumes.
//
//   node bench.mjs <label> <engine binary> [--runs N]
import { spawn } from "node:child_process";
import { appendFileSync, cpSync, existsSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { createServer } from "node:net";
import { loadavg, tmpdir } from "node:os";
import { join, resolve } from "node:path";

const root = import.meta.dirname;
const args = process.argv.slice(2);
const runsAt = args.indexOf("--runs");
const RUNS = runsAt >= 0 ? Number(args.splice(runsAt, 2)[1]) : 3;
const [label, binArg] = args;
if (!label || !binArg) throw new Error("usage: node bench.mjs <label> <engine binary> [--runs N]");
const bin = resolve(binArg);
const file = join(root, "results", `${label}.jsonl`);
const TIMEOUT = 300_000;

const freePort = () =>
  new Promise((res, rej) => {
    const s = createServer();
    s.once("error", rej);
    s.listen(0, "127.0.0.1", () => {
      const { port } = s.address();
      s.close(() => res(port));
    });
  });

/** One engine: its `bench:` rows, or an error. */
async function run() {
  const dir = mkdtempSync(join(tmpdir(), "session-bytes-bench-"));
  cpSync(join(root, "project"), dir, { recursive: true });
  const load = loadavg().map((x) => +x.toFixed(1));
  const engine = spawn(bin, ["dev", join(dir, "game.ts"), "--headless", "--remote-port", String(await freePort())], { cwd: dir, env: { ...process.env, NO_COLOR: "1" } });
  const rows = [];
  let log = "";
  let buf = "";
  let error = null;
  let done;
  const finished = new Promise((r) => (done = r));
  const onLine = (line) => {
    if (/generation \d+ failed|panicked/.test(line)) {
      error ??= line.slice(0, 600);
      done();
    }
    const m = line.match(/bench: (\{.*\})/);
    if (m) rows.push({ ...JSON.parse(m[1]), load });
    if (/bench: done/.test(line)) done();
  };
  const feed = (d) => {
    log += d;
    buf += d;
    const lines = buf.split("\n");
    buf = lines.pop();
    for (const l of lines) onLine(l);
  };
  engine.stdout.on("data", feed);
  engine.stderr.on("data", feed);
  engine.on("exit", (code) => {
    error ??= `engine exited ${code}`;
    done();
  });
  const timer = setTimeout(() => {
    error = "timeout";
    done();
  }, TIMEOUT);
  await finished;
  clearTimeout(timer);
  engine.kill("SIGKILL");
  if (error) writeFileSync(join(root, "results", `error-${Date.now()}.log`), log);
  rmSync(dir, { recursive: true, force: true });
  return error ? [{ error, load }] : rows;
}

const have = existsSync(file) ? new Set(readFileSync(file, "utf8").trim().split("\n").filter(Boolean).map((l) => JSON.parse(l).run)) : new Set();
for (let k = 0; k < RUNS; k++) {
  if (have.has(k)) continue;
  const rows = await run();
  for (const r of rows) {
    appendFileSync(file, JSON.stringify({ label, run: k, at: new Date().toISOString(), ...r }) + "\n");
    console.log(`${label} | run ${k} | ${r.error ? `ERROR ${r.error}` : `${r.arm}: median ${r.median.toFixed(3)} ${r.unit}, p95 ${r.p95.toFixed(3)}`}`);
  }
  if (rows[0]?.error) break;
}
