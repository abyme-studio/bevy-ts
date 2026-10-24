// One bench run: a fresh copy of the project with this run's src/config.ts, the engine headless,
// and its `bench:` line with the load average.
//
//   node run.mjs <engine binary> '{"n":10000,"arm":"grow"}' [timeoutSec=300]
// Prints one JSON line: { config, ms, frames, longest, rows, load, error }
import { spawn } from "node:child_process";
import { cpSync, mkdtempSync, rmSync, writeFileSync } from "node:fs";
import { createServer } from "node:net";
import { loadavg, tmpdir } from "node:os";
import { join, resolve } from "node:path";

const root = import.meta.dirname;
const bin = resolve(process.argv[2]);
const config = { n: 1000, arm: "grow", ...JSON.parse(process.argv[3] ?? "{}") };
const TIMEOUT = Number(process.argv[4] ?? 300) * 1000;

const dir = mkdtempSync(join(tmpdir(), "tsx-for-"));
for (const p of ["src", "tsconfig.json", "bevy-ts.json"]) cpSync(join(root, p), join(dir, p), { recursive: true });
writeFileSync(join(dir, "src/config.ts"), `export const C = ${JSON.stringify(config)} as const;\n`);

const port = await new Promise((res, rej) => {
  const s = createServer();
  s.once("error", rej);
  s.listen(0, "127.0.0.1", () => {
    const { port } = s.address();
    s.close(() => res(port));
  });
});

const out = { config, ms: null, frames: null, longest: null, rows: null, load: loadavg().map((x) => +x.toFixed(1)), error: null };
const engine = spawn(bin, ["dev", join(dir, "src/game.tsx"), "--headless", "--remote-port", String(port)], { cwd: dir, env: { ...process.env, NO_COLOR: "1" } });
let log = "";
let buf = "";
let done;
const finished = new Promise((r) => (done = r));
const onLine = (line) => {
  if (/generation \d+ failed|panicked|error:/i.test(line) && !out.error) out.error = line.slice(0, 600);
  const m = line.match(/bench: \S+ \d+ ([\d.]+) ms, (\d+) frames, longest ([\d.]+) ms, rows (\d+)/);
  if (m) {
    Object.assign(out, { ms: +m[1], frames: +m[2], longest: +m[3], rows: +m[4] });
    done();
  }
  if (/generation \d+ failed|panicked/.test(line)) done();
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
  out.error ??= `engine exited ${code}`;
  done();
});
const timer = setTimeout(() => {
  out.error = "timeout";
  done();
}, TIMEOUT);
await finished;
clearTimeout(timer);
engine.kill("SIGKILL");
if (out.error) {
  out.logFile = join(root, "results", `error-${Date.now()}.log`);
  writeFileSync(out.logFile, log);
}
rmSync(dir, { recursive: true, force: true });
console.log(JSON.stringify(out));
process.exit(0);
