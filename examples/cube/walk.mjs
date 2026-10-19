// Walks the six first-milestone demo steps (openspec/changes/bridge/design.md) against the real
// engine binary, headless, and prints what it saw with the reload latencies.
//
//   node examples/cube/walk.mjs [path/to/bevy-ts]    (default target/release/bevy-ts)
import { spawn } from "node:child_process";
import { cpSync, mkdtempSync, readFileSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join, resolve } from "node:path";

const root = resolve(import.meta.dirname, "../..");
const bin = resolve(process.argv[2] ?? join(root, "target/release/bevy-ts"));
const dir = mkdtempSync(join(tmpdir(), "bevy-ts-walk-"));
cpSync(join(root, "examples/cube/src"), join(dir, "src"), { recursive: true });
const game = join(dir, "src/game.ts");
const original = readFileSync(game, "utf8");
const PORT = 15931;
const INSPECT = 9349;
const results = [];
const note = (step, ok, detail) => {
  results.push({ step, ok, detail });
  console.log(`${ok ? "PASS" : "FAIL"} ${step}: ${detail}`);
};
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));

const engine = spawn(bin, ["dev", game, "--headless", "--remote-port", String(PORT), `--inspect=${INSPECT}`], {
  env: { ...process.env, NO_COLOR: "1" },
});
let terminal = "";
engine.stdout.on("data", (d) => (terminal += d));
engine.stderr.on("data", (d) => (terminal += d));

let id = 0;
async function rpc(method, params) {
  const res = await fetch(`http://127.0.0.1:${PORT}`, {
    method: "POST",
    body: JSON.stringify({ jsonrpc: "2.0", id: ++id, method, params }),
  });
  const body = await res.json();
  if (body.error) throw new Error(`${method}: ${body.error.message}`);
  return body.result;
}

/** Opens a +watch stream; returns { messages, close }. */
function watch(method, params) {
  const messages = [];
  const ctrl = new AbortController();
  (async () => {
    try {
      const res = await fetch(`http://127.0.0.1:${PORT}`, {
        method: "POST",
        body: JSON.stringify({ jsonrpc: "2.0", id: ++id, method, params }),
        signal: ctrl.signal,
      });
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
          if (data) messages.push({ at: performance.now(), msg: JSON.parse(data).result });
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
    await sleep(5);
  }
  return null;
}

async function cube() {
  const snap = await rpc("bevy-ts/entities");
  const movers = snap.entities.filter((e) => e.components.includes("game::Mover"));
  return { count: movers.length, entity: movers[0]?.entity };
}

const TRANSFORM = "bevy_transform::components::transform::Transform";
async function components(entity) {
  return (await rpc("bevy-ts/get_components", { entity, components: ["game::Mover", TRANSFORM] })).components;
}

async function saveAndWait(src, reloads) {
  const before = reloads.messages.length;
  const t0 = performance.now();
  writeFileSync(game, src);
  const done = await until(() =>
    reloads.messages.slice(before).find((m) => m.msg.reports.some((r) => r.status === "applied" || r.status === "failed")),
  );
  const report = done?.msg.reports.find((r) => r.status !== "compiling");
  return { ms: done ? done.at - t0 : null, report };
}

try {
  const hello = await until(() => rpc("bevy-ts/hello").catch(() => null), 30000);
  if (!hello) throw new Error(`engine did not come up:\n${terminal}`);
  await until(async () => (await rpc("bevy-ts/hello")).generation >= 1, 10000);
  const reloads = watch("bevy-ts/reload+watch");
  await sleep(300);

  // 1. The cube bobs.
  const c = await cube();
  const y0 = (await components(c.entity))[TRANSFORM].translation[1];
  await sleep(250);
  const y1 = (await components(c.entity))[TRANSFORM].translation[1];
  note("1 window and bobbing cube", c.count === 1 && y0 !== y1, `one cube, y ${y0.toFixed(3)} -> ${y1.toFixed(3)} (headless, offscreen target)`);

  // 2. Move x too.
  const xSrc = original.replace(
    "t.translation.y = Math.sin(time.elapsed_secs * mover.speed) * mover.amplitude;",
    "t.translation.y = Math.sin(time.elapsed_secs * mover.speed) * mover.amplitude;\n    t.translation.x = Math.cos(time.elapsed_secs * mover.speed);",
  );
  const before2 = (await components(c.entity))[TRANSFORM].translation;
  const r2 = await saveAndWait(xSrc, reloads);
  await sleep(100);
  const after2 = (await components(c.entity))[TRANSFORM].translation;
  const c2 = await cube();
  note(
    "2 edit bob to move x",
    r2.report?.status === "applied" && r2.ms < 200 && c2.count === 1 && c2.entity === c.entity && after2[0] !== 0 && Math.abs(after2[1] - before2[1]) < 2.1,
    `applied gen ${r2.report?.generation} in ${r2.ms?.toFixed(1)} ms from save (engine ${r2.report?.durationMs.toFixed(1)} ms); same entity, x=${after2[0].toFixed(2)}, still one cube`,
  );

  // 3. Add phase.
  const phaseSrc = xSrc
    .replace("amplitude: f32 = 2;", "amplitude: f32 = 2;\n  phase: f32 = 0;")
    .replace("Math.sin(time.elapsed_secs * mover.speed)", "Math.sin(time.elapsed_secs * mover.speed + mover.phase)");
  const r3 = await saveAndWait(phaseSrc, reloads);
  const m3 = (await components(c.entity))["game::Mover"];
  const migrated = r3.report?.migrations?.[0];
  note(
    "3 add phase: f32",
    migrated?.component === "game::Mover" && migrated.entities === 1 && m3.speed === 1 && m3.amplitude === 2 && m3.phase === 0,
    `"${migrated?.component} migrated on ${migrated?.entities} entities" (also in the terminal: ${terminal.includes("game::Mover migrated on 1 entities")}); speed ${m3.speed}, amplitude ${m3.amplitude}, phase ${m3.phase}; ${r3.ms?.toFixed(1)} ms`,
  );

  // 4. Mutate amplitude; the watch streams it.
  const w = watch("bevy-ts/get_components+watch", { entity: c.entity, components: ["game::Mover"] });
  await until(() => w.messages.length >= 1);
  const t4 = performance.now();
  await rpc("bevy-ts/mutate_components", { entity: c.entity, component: "game::Mover", path: ".amplitude", value: 4 });
  const streamed = await until(() => w.messages.find((m) => m.msg.components["game::Mover"]?.amplitude === 4));
  await sleep(300);
  const ys = [];
  for (let i = 0; i < 20; i++) {
    ys.push((await components(c.entity))[TRANSFORM].translation[1]);
    await sleep(40);
  }
  w.close();
  note(
    "4 mutate .amplitude = 4",
    !!streamed && Math.max(...ys.map(Math.abs)) > 2.05,
    `watch streamed amplitude 4 after ${(streamed.at - t4).toFixed(1)} ms; |y| reaches ${Math.max(...ys.map(Math.abs)).toFixed(2)} (> 2)`,
  );

  // 5. A syntax error.
  const r5 = await saveAndWait(phaseSrc.replace("q.each((mover, t) => {", "q.each((mover, t) => {{"), reloads);
  const y5a = (await components(c.entity))[TRANSFORM].translation[1];
  await sleep(200);
  const y5b = (await components(c.entity))[TRANSFORM].translation[1];
  const loc = r5.report?.errors?.[0]?.location;
  note(
    "5 save a syntax error",
    r5.report?.status === "failed" && !!loc && y5a !== y5b && terminal.includes("game.ts:"),
    `reload+watch: failed at ${loc?.file}:${loc?.line}:${loc?.column}; the cube keeps moving (${y5a.toFixed(2)} -> ${y5b.toFixed(2)})`,
  );
  writeFileSync(game, phaseSrc);
  await sleep(300);

  // 6. Break in bob over CDP, then again after a save.
  const list = await (await fetch(`http://127.0.0.1:${INSPECT}/json/list`)).json();
  const ws = new WebSocket(list[0].webSocketDebuggerUrl);
  await new Promise((r) => ws.addEventListener("open", r));
  const scripts = new Map();
  const paused = [];
  let cdpId = 0;
  const send = (method, params = {}) => ws.send(JSON.stringify({ id: ++cdpId, method, params }));
  ws.addEventListener("message", (ev) => {
    const m = JSON.parse(ev.data);
    if (m.method === "Debugger.scriptParsed") scripts.set(m.params.scriptId, m.params);
    if (m.method === "Debugger.paused") {
      const loc = m.params.callFrames[0].location;
      paused.push({ at: performance.now(), url: scripts.get(loc.scriptId)?.url, line: loc.lineNumber });
      setTimeout(() => send("Debugger.resume"), 300);
    }
  });
  send("Debugger.enable");
  await until(() => [...scripts.values()].some((s) => s.url.includes("game.ts")));
  const live = [...scripts.values()].filter((s) => s.url.includes("game.ts")).pop();
  // Map the TS line of the y assignment to the compiled line, as an editor does from the map.
  const src = await new Promise((r) => {
    const want = ++cdpId;
    const h = (ev) => {
      const m = JSON.parse(ev.data);
      if (m.id === want) {
        ws.removeEventListener("message", h);
        r(m.result.scriptSource);
      }
    };
    ws.addEventListener("message", h);
    ws.send(JSON.stringify({ id: want, method: "Debugger.getScriptSource", params: { scriptId: live.scriptId } }));
  });
  const line = src.split("\n").findIndex((l) => l.includes("t.translation.y ="));
  send("Debugger.setBreakpointByUrl", { urlRegex: "game\\.ts(\\?gen=\\d+)?$", lineNumber: line });
  const hit1 = await until(() => paused[0]);
  // While paused the frame is blocked: the protocol answers only after resume.
  const t6 = performance.now();
  await rpc("bevy-ts/hello");
  const frozenFor = performance.now() - t6;
  await sleep(400);
  const r6 = await saveAndWait(phaseSrc + "\n// unrelated\n", reloads);
  const hit2 = await until(() => paused.find((p) => p.url?.includes(`gen=`) && p.url !== hit1.url));
  send("Debugger.removeBreakpoint", { breakpointId: "1" });
  send("Debugger.disable");
  ws.close();
  note(
    "6 break in bob over the inspector",
    !!hit1 && !!hit2 && frozenFor > 100,
    `paused at ${hit1?.url?.split("/").pop()} line ${hit1?.line}; the frame waited ${frozenFor.toFixed(0)} ms; after a save (gen ${r6.report?.generation}) it hit again at ${hit2?.url?.split("/").pop()} on the same session`,
  );
} catch (e) {
  note("walk", false, e.stack);
} finally {
  engine.kill();
  const ok = results.every((r) => r.ok);
  console.log(`\n${ok ? "all six steps pass" : "FAILED"}`);
  process.exitCode = ok ? 0 : 1;
}
