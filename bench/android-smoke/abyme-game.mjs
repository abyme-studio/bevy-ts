// Agent town's worlds on the emulator (change android-native's proofs): run.sh plays a copy of
// agent town (AGENT_TOWN's HEAD, ~/dev/agent-town, exported and installed under the smoke's cache) against its own stand-in abymed, test/replay.mjs, on
// the frame test's world, so the phone never reaches a real abyme. The emulator reaches the
// host's 127.0.0.1 as 10.0.2.2, which the game is given as ABYME_URL and so also allows.
//
//   node bench/android-smoke/abyme-game.mjs town     agent town on test/frame.mjs's twelve agents
//   node bench/android-smoke/abyme-game.mjs learn    the learning universe on test/fixtures/learn-world.json
import { execFileSync, spawn } from "node:child_process";
import { cpSync, existsSync, mkdirSync, mkdtempSync, rmSync, readFileSync, symlinkSync, writeFileSync } from "node:fs";
import { homedir, tmpdir } from "node:os";
import { join, resolve } from "node:path";

const world = process.argv[2] ?? "town";
if (!["town", "learn"].includes(world)) throw new Error(`abyme-game.mjs: town or learn, not ${world}`);
const checkout = resolve(process.env.AGENT_TOWN ?? join(homedir(), "dev/agent-town"));
// Agent town's HEAD with its packages, kept per commit: nothing is installed into its checkout.
const head = execFileSync("git", ["-C", checkout, "rev-parse", "--short", "HEAD"], { encoding: "utf8" }).trim();
const town = join(process.env.BEVY_TS_SMOKE_CACHE ?? join(homedir(), ".cache/bevy-ts/android-smoke"), `agent-town-${head}`);
if (!existsSync(join(town, "node_modules"))) {
  rmSync(town, { recursive: true, force: true });
  mkdirSync(town, { recursive: true });
  execFileSync("bash", ["-c", `git -C "${checkout}" archive HEAD | tar -x -C "${town}"`], { stdio: "inherit" });
  execFileSync("pnpm", ["install", "--frozen-lockfile"], { cwd: town, stdio: "inherit" });
}
const here = import.meta.dirname;
const { replay, ownEnv } = await import(join(town, "test/replay.mjs"));
const { FRAME_AGENTS } = await import(join(town, "test/frame.mjs"));

// A copy, as the frame test makes, so nothing is written into agent town's checkout.
const dir = mkdtempSync(join(tmpdir(), `bevy-ts-smoke-${world}-`));
for (const p of ["src", "assets", "package.json", "tsconfig.json", "bevy-ts.json"]) cpSync(join(town, p), join(dir, p), { recursive: true });
symlinkSync(join(town, "node_modules"), join(dir, "node_modules"));

let abymed;
if (world === "learn") {
  const learn = JSON.parse(readFileSync(join(town, "test/fixtures/learn-world.json"), "utf8"));
  abymed = await replay({ world: { agents: [] }, learn: learn.world, state: { "agent-town.world": "learn" } });
  // At the fixture's `now`, so the moons due are the frame test's.
  const fixture = join(dir, "src/fixture.ts");
  writeFileSync(fixture, readFileSync(fixture, "utf8").replace("export const LEARN_NOW: number | null = null;", `export const LEARN_NOW: number | null = ${learn.now};`));
} else {
  abymed = await replay({ world: { agents: FRAME_AGENTS }, state: {} });
}
const url = abymed.url.replace("127.0.0.1", "10.0.2.2");
console.log(`abyme-game: ${world} against the stand-in abymed at ${abymed.url}, ${url} from the emulator`);

const name = world === "learn" ? "learning-universe" : "agent-town";
const run = spawn(join(here, "run.sh"), [name], {
  stdio: "inherit",
  env: {
    ...ownEnv,
    SMOKE_PROJECT: dir,
    ...(world === "learn" && { SMOKE_ENTRY: "src/learn/game.ts" }),
    SMOKE_ENV: JSON.stringify({ ABYME_URL: url }),
    SETTLE: process.env.SETTLE ?? "20",
  },
});
const code = await new Promise((r) => run.on("exit", (c) => r(c ?? 1)));
await abymed.close();
process.exit(code);
