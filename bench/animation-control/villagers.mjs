// Change animation-control, tasks 1.1 and 6.2: agent town's 100 villagers, all spawned, at the fit,
// headless, CPU % and frame time over 300 frames (agent town's test/walk.mjs in frame-only mode).
// Agent town is copied from ~/dev/agent-town's HEAD into a temp folder and patched there, never in
// place: WALK_ANIM picks the arm, every unit's villager is spawned, and PlayAnimation takes the
// arm's field (only that one, so the pinned npm engine runs the arms it knows).
//
//   node bench/animation-control/villagers.mjs <engine> [arm ...]
//   arms: none (0 villagers), play, speed0, paused, hidden (Visibility::Hidden), hidden-cull
// 3 runs per arm, interleaved; 8 when an arm is within noise of play.
import { execFileSync, execSync, spawnSync } from "node:child_process";
import { mkdtempSync, readFileSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join, resolve } from "node:path";

const engine = process.argv[2] === "npm" ? "npm" : resolve(process.argv[2]);
const arms = process.argv.length > 3 ? process.argv.slice(3) : ["none", "play", "speed0", "paused", "hidden", "hidden-cull"];
const src = process.env.AGENT_TOWN ?? join(process.env.HOME, "dev/agent-town");
const rev = execFileSync("git", ["-C", src, "rev-parse", "--short", "HEAD"], { encoding: "utf8" }).trim();
const dir = mkdtempSync(join(tmpdir(), "anim-villagers-"));
execSync(`git -C ${src} archive HEAD | tar -x -C ${dir}`);
execSync("pnpm install --frozen-lockfile --silent", { cwd: dir, stdio: "inherit" });

const patch = (file, from, to) => {
  const p = join(dir, file);
  const s = readFileSync(p, "utf8");
  if (!s.includes(from)) throw new Error(`${file}: no ${JSON.stringify(from)}: agent town moved, update the patch`);
  writeFileSync(p, s.replace(from, to));
};
patch("test/walk.mjs", "`export const MEASURE = true;\\n` +", "`export const MEASURE = true;\\n` + `export const ANIM: string = ${JSON.stringify(process.env.WALK_ANIM ?? \"\")};\\n` +");
patch("src/fixture.ts", "export const MEASURE = false;", "export const MEASURE = false;\nexport const ANIM: string = \"\";");
patch("src/game.ts", 'import { CAMERA, CAMERA_AT, FAR_OFF, MEASURE, OPEN_CARD, PANE } from "./fixture.ts";', 'import { ANIM, CAMERA, CAMERA_AT, FAR_OFF, MEASURE, OPEN_CARD, PANE } from "./fixture.ts";');
patch("src/game.ts", "  const was = animating();", "  if (ANIM) for (const u of drawn().units) moving.add(u.agent);\n  const was = animating();");
patch("src/world.tsx", 'import { PROPS_OFF } from "./fixture.ts";', 'import { ANIM, PROPS_OFF } from "./fixture.ts";');
patch("src/world.tsx", '    <Visibility value="Inherited" />\n    <Transform translation={m.offset}', '    <Visibility value={ANIM.startsWith("hidden") ? "Hidden" : "Inherited"} />\n    <Transform translation={m.offset}');
patch(
  "src/world.tsx",
  "<PlayAnimation clip={m.clip} repeat={true} speed={speed} transition_secs={0.25} />",
  '<PlayAnimation clip={m.clip} speed={ANIM === "speed0" ? 0 : speed} transition_secs={0.25} {...(ANIM === "paused" ? { paused: true } : {})} {...(ANIM === "hidden-cull" ? { cull: true } : {})} />',
);

const view = { WALK_CAMERA: "fit", WALK_FAR: "1" };
const env = (arm) => (arm === "none" ? { ...view, WALK_AGENTS: "0" } : { ...view, WALK_AGENTS: "100", WALK_TOWN: "0", WALK_FRAME_ONLY: "1", WALK_ANIM: arm });
const run = (arm) => {
  const args = [join(dir, "test/walk.mjs"), ...(engine === "npm" ? [] : [engine])];
  const out = spawnSync("nice", ["-n", "10", "node", ...args], { env: { ...process.env, ...env(arm) }, encoding: "utf8" }).stdout;
  const m = out.match(/frame time at \d+ units: ([\d.]+) ms mean.*?cpu ([\d.]+) %$/m);
  if (!m) throw new Error(`no frame time for ${arm}:\n${out.slice(-2000)}`);
  return { ms: Number(m[1]), cpu: Number(m[2]) };
};
const stats = (xs) => {
  const mean = xs.reduce((a, b) => a + b, 0) / xs.length;
  return { mean, sd: Math.sqrt(xs.reduce((a, x) => a + (x - mean) ** 2, 0) / Math.max(1, xs.length - 1)) };
};
const runs = Object.fromEntries(arms.map((a) => [a, []]));
const round = () => {
  for (const a of arms) {
    const r = run(a);
    runs[a].push(r);
    console.log(`  ${a}: ${r.ms.toFixed(2)} ms, ${r.cpu.toFixed(1)} %`);
  }
};
for (let i = 0; i < 3; i++) round();
const near = (a, b) => {
  if (!runs[a] || !runs[b]) return false;
  const [x, y] = [a, b].map((n) => stats(runs[n].map((r) => r.cpu)));
  return Math.abs(x.mean - y.mean) < 2 * Math.max(x.sd, y.sd);
};
if (arms.some((a) => a !== "play" && a !== "none" && near("play", a))) for (let i = 3; i < 8; i++) round();
const fmt = (s, d) => `${s.mean.toFixed(d)} ± ${s.sd.toFixed(d)}`;
console.log(`\nagent town ${rev}, engine ${engine}, 100 villagers at the fit, headless, niced\n| arm | CPU % | frame ms | runs |\n| --- | --- | --- | --- |`);
for (const a of arms) console.log(`| ${a} | ${fmt(stats(runs[a].map((r) => r.cpu)), 1)} | ${fmt(stats(runs[a].map((r) => r.ms)), 2)} | ${runs[a].length} |`);
