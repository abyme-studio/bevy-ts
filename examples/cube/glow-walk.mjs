// Walks the script-material demo (openspec/changes/script-materials, task 1.7) against the real
// engine binary, headless: game::Glow on the cube example (entry src/glow.tsx), its shader in
// glow.wgsl, edited live.
//
//   node examples/cube/glow-walk.mjs [path/to/bevy-ts]    (default target/release/bevy-ts)
//
// The first frames are saved to target/glow-walk.png (and .2.png), to look at.
import { spawn } from "node:child_process";
import { cpSync, mkdtempSync, readFileSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join, resolve } from "node:path";

const root = resolve(import.meta.dirname, "../..");
const bin = resolve(process.argv[2] ?? join(root, "target/release/bevy-ts"));
const dir = mkdtempSync(join(tmpdir(), "bevy-ts-glow-"));
cpSync(join(root, "examples/cube/src"), join(dir, "src"), { recursive: true });
const game = join(dir, "src/glow.tsx");
const wgsl = join(dir, "src/glow.wgsl");
const original = readFileSync(wgsl, "utf8");
const PORT = 15932;
const results = [];
const note = (step, ok, detail) => {
  results.push({ step, ok, detail });
  console.log(`${ok ? "PASS" : "FAIL"} ${step}: ${detail}`);
};
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));

const engine = spawn(bin, ["dev", game, "--headless", "--remote-port", String(PORT)], {
  env: { ...process.env, NO_COLOR: "1", BEVY_TS_SCREENSHOT: join(root, "target/glow-walk.png") },
});
let terminal = "";
engine.stdout.on("data", (d) => (terminal += d));
engine.stderr.on("data", (d) => (terminal += d));

let id = 0;
async function rpc(method, params) {
  const res = await fetch(`http://127.0.0.1:${PORT}`, { method: "POST", body: JSON.stringify({ jsonrpc: "2.0", id: ++id, method, params }) });
  const body = await res.json();
  if (body.error) throw new Error(`${method}: ${body.error.message}`);
  return body.result;
}

function watch(method, params) {
  const messages = [];
  const ctrl = new AbortController();
  (async () => {
    try {
      const res = await fetch(`http://127.0.0.1:${PORT}`, { method: "POST", body: JSON.stringify({ jsonrpc: "2.0", id: ++id, method, params }), signal: ctrl.signal });
      const reader = res.body.getReader();
      let buf = "";
      for (;;) {
        const { value, done } = await reader.read();
        if (done) break;
        buf += new TextDecoder().decode(value);
        let i;
        while ((i = buf.indexOf("\n\n")) >= 0) {
          const chunk = buf.slice(0, i);
          buf = buf.slice(i + 2);
          const data = chunk.split("\n").filter((l) => l.startsWith("data:")).map((l) => l.slice(5)).join("");
          if (data) messages.push(JSON.parse(data).result);
        }
      }
    } catch {}
  })();
  return { messages, close: () => ctrl.abort() };
}

async function until(f, ms = 5000) {
  const start = performance.now();
  while (performance.now() - start < ms) {
    const v = await f();
    if (v) return v;
    await sleep(10);
  }
  return null;
}

async function glowEntity() {
  const snap = await rpc("bevy-ts/entities");
  return snap.entities.find((e) => e.name === "glow")?.entity;
}

try {
  const hello = await until(() => rpc("bevy-ts/hello").catch(() => null), 60000);
  if (!hello) throw new Error(`engine did not come up:\n${terminal}`);
  await until(async () => (await rpc("bevy-ts/hello")).generation >= 1, 20000);
  const gen = (await rpc("bevy-ts/hello")).generation;
  const reloads = watch("bevy-ts/reload+watch");
  const log = watch("bevy-ts/log+watch");
  await sleep(1500);

  // 1. The glow cube is there, from TSX, with no Rust.
  const e = await glowEntity();
  note("1 game::Glow on a cube", !!e && !/material game::Glow/.test(terminal), `entity ${e}; no material errors`);

  // 2. Save glow.wgsl: the shader is replaced, no generation, no respawn.
  const r0 = reloads.messages.length;
  writeFileSync(wgsl, original.replace("const FLOOR: f32 = 0.25;", "const FLOOR: f32 = 0.6;"));
  await sleep(800);
  const gen2 = (await rpc("bevy-ts/hello")).generation;
  const e2 = await glowEntity();
  note(
    "2 save glow.wgsl",
    gen2 === gen && e2 === e && reloads.messages.length === r0 && !/material game::Glow/.test(terminal),
    `generation ${gen} -> ${gen2}; same entity ${e2 === e}; reload reports ${reloads.messages.length - r0}`,
  );

  // 3. Save a type error on line 12: reported at glow.wgsl:12, the old shader kept.
  writeFileSync(wgsl, original.replace("params.color.rgb * glow, params.color.a", "params.color.rgb * glow, 1u"));
  const err = await until(() => log.messages.flatMap((m) => m.entries ?? [m]).find((x) => /game::Glow/.test(JSON.stringify(x))), 5000);
  const text = JSON.stringify(err);
  note("3 save a WGSL error", !!err && /glow\.wgsl/.test(text) && /"line":12\b/.test(text), text?.slice(0, 300));

  writeFileSync(wgsl, original);
  await sleep(500);
} catch (e) {
  note("walk", false, `${e.message}\n${terminal.slice(-2000)}`);
} finally {
  engine.kill();
}
const failed = results.filter((r) => !r.ok).length;
console.log(failed ? `\n${failed} step(s) failed` : "\nall steps pass");
process.exit(failed ? 1 : 0);
