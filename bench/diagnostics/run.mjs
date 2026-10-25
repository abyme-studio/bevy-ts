// One bench run: a fresh copy of the project with this run's config and plugin list, the engine
// headless, and its `bench:` line with the load average.
//
//   node run.mjs <engine binary> '{"n":10000}' '["EntityCountDiagnosticsPlugin"]' [timeoutSec=300]
// Prints one JSON line: { config, plugins, main, frame, load, error }
import { spawn } from "node:child_process";
import { cpSync, mkdtempSync, rmSync, writeFileSync } from "node:fs";
import { createServer } from "node:net";
import { loadavg, tmpdir } from "node:os";
import { join, resolve } from "node:path";

const root = import.meta.dirname;
const bin = resolve(process.argv[2]);
const config = { n: 10000, ...JSON.parse(process.argv[3] ?? "{}") };
const plugins = JSON.parse(process.argv[4] ?? "[]");
const TIMEOUT = Number(process.argv[5] ?? 300) * 1000;

const dir = mkdtempSync(join(tmpdir(), "diagnostics-bench-"));
for (const p of ["src", "tsconfig.json"]) cpSync(join(root, p), join(dir, p), { recursive: true });
writeFileSync(join(dir, "src/config.ts"), `export const C = ${JSON.stringify(config)} as const;\n`);
writeFileSync(join(dir, "bevy-ts.json"), JSON.stringify({ diagnostics: plugins }));

const port = await new Promise((res, rej) => {
  const s = createServer();
  s.once("error", rej);
  s.listen(0, "127.0.0.1", () => {
    const { port } = s.address();
    s.close(() => res(port));
  });
});

const out = { config, plugins, main: null, frame: null, load: loadavg().map((x) => +x.toFixed(1)), error: null };
const engine = spawn(bin, ["dev", join(dir, "src/game.tsx"), "--headless", "--remote-port", String(port)], { cwd: dir, env: { ...process.env, NO_COLOR: "1" } });
let log = "";
let buf = "";
let done;
const finished = new Promise((r) => (done = r));
const onLine = (line) => {
  if (/generation \d+ failed|panicked/.test(line)) {
    out.error ??= line.slice(0, 600);
    done();
  }
  const m = line.match(/bench: main ([\d.]+) ms, frame ([\d.]+) ms/);
  if (m) {
    Object.assign(out, { main: +m[1], frame: +m[2] });
    done();
  }
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
