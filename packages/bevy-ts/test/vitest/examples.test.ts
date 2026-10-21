import { execFileSync, spawnSync } from "node:child_process";
import { existsSync, mkdirSync, mkdtempSync, readdirSync, readFileSync, symlinkSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { fileURLToPath } from "node:url";
import { expect, test } from "vitest";

// `pnpm test` runs scripts/sync-examples.mjs first, so examples/ holds what the package ships.
const pkg = fileURLToPath(new URL("../../", import.meta.url));
const launcher = join(pkg, "bin/bevy-ts.js");
const version = JSON.parse(readFileSync(join(pkg, "package.json"), "utf8")).version;

// A binary that does not exist: the launcher's own commands must never reach for one.
const env = { ...process.env, BEVY_TS_BINARY: "/nonexistent/bevy-ts" };
const run = (...args: string[]) => spawnSync("node", [launcher, ...args], { encoding: "utf8", env });
const scratch = () => mkdtempSync(join(tmpdir(), "bevy-ts-new-"));

test("examples --json lists the shipped examples, cube first, with no engine", () => {
  const r = run("examples", "--json");
  expect(r.status).toBe(0);
  const out = JSON.parse(r.stdout);
  expect(out.version).toBe(version);
  expect(out.examples.map((e: { id: string }) => e.id)).toEqual(["cube", "arena"]);
  for (const e of out.examples) {
    expect(Object.keys(e).sort()).toEqual(["description", "entry", "id", "title"]);
  }
  expect(out.examples[0].entry).toBe("src/game.ts");
  expect(out.examples[1].entry).toBe("src/game.tsx");
});

test.each(["cube", "arena"])("new --example %s makes a project that type-checks against the package", (id) => {
  const dir = join(scratch(), "my-game");
  const r = run("new", dir, "--example", id, "--no-install", "--json");
  expect(r.status, r.stderr).toBe(0);
  const out = JSON.parse(r.stdout);
  const entry = id === "cube" ? "src/game.ts" : "src/game.tsx";
  expect(out).toEqual({ dir, entry, example: id, installed: false });

  expect(existsSync(join(dir, entry))).toBe(true);
  expect(existsSync(join(dir, "example.json"))).toBe(false);
  expect(existsSync(join(dir, "WALK.md"))).toBe(false);
  const p = JSON.parse(readFileSync(join(dir, "package.json"), "utf8"));
  expect(p).toMatchObject({ name: "my-game", private: true, type: "module", scripts: { dev: `bevy-ts dev ${entry}` } });
  expect(p.devDependencies).toEqual({ "bevy-ts": version });
  const ts = JSON.parse(readFileSync(join(dir, "tsconfig.json"), "utf8"));
  expect(ts.compilerOptions.paths).toBeUndefined();
  expect(readFileSync(join(dir, ".gitignore"), "utf8")).toBe("node_modules/\n.bevy-ts/\n");

  // Linked as npm would install it: tsc resolves bevy-ts from node_modules.
  mkdirSync(join(dir, "node_modules"));
  symlinkSync(pkg, join(dir, "node_modules/bevy-ts"), "dir");
  const tsc = spawnSync(join(pkg, "node_modules/.bin/tsc"), ["-p", dir], { encoding: "utf8" });
  expect(tsc.status, tsc.stdout + tsc.stderr).toBe(0);
});

test("new refuses a directory with a file in it and writes nothing", () => {
  const dir = scratch();
  writeFileSync(join(dir, "notes.txt"), "mine");
  const r = run("new", dir, "--json");
  expect(r.status).toBe(1);
  expect(r.stderr).toMatch(/not empty/);
  expect(r.stdout).toBe("");
  expect(readdirSync(dir)).toEqual(["notes.txt"]);
});

test("new refuses an unknown example, names the known ones and writes nothing", () => {
  const dir = join(scratch(), "game");
  const r = run("new", dir, "--example", "nope", "--json");
  expect(r.status).toBe(1);
  expect(r.stderr).toMatch(/cube, arena/);
  expect(r.stdout).toBe("");
  expect(existsSync(dir)).toBe(false);
});

test("new reports a failed install, leaves the project and exits 1", () => {
  const dir = join(scratch(), "game");
  // No package manager on PATH: the install cannot run.
  const r = spawnSync(process.execPath, [launcher, "new", dir, "--json"], {
    encoding: "utf8",
    env: { ...env, PATH: "/nonexistent", npm_config_user_agent: "npm/11 node/24" },
  });
  expect(r.status).toBe(1);
  expect(JSON.parse(r.stdout)).toEqual({ dir, entry: "src/game.ts", example: "cube", installed: false });
  expect(existsSync(join(dir, "src/game.ts"))).toBe(true);
});

test("the packed package holds cube and arena and nothing else from examples/", () => {
  const out = execFileSync("npm", ["pack", "--dry-run", "--json"], { cwd: pkg, encoding: "utf8", stdio: ["ignore", "pipe", "ignore"] });
  const files: string[] = JSON.parse(out.slice(out.indexOf("[")))[0].files.map((f: { path: string }) => f.path);
  const examples = files.filter((f) => f.startsWith("examples/"));
  expect(examples).toContain("examples/cube/src/game.ts");
  expect(examples).toContain("examples/cube/example.json");
  expect(examples).toContain("examples/arena/src/game.tsx");
  expect(examples).toContain("examples/arena/assets/levels/arena.bsn");
  for (const f of examples) {
    expect(f).not.toMatch(/rapier|thrusters|WALK\.md|walk\.mjs|tsconfig\.json/);
  }
});

test("--example followed by a flag is a usage error, not an id", () => {
  const dir = join(scratch(), "g");
  const r = run("new", dir, "--example", "--json");
  expect(r.status).toBe(2);
  expect(r.stderr).toMatch(/--example takes an id/);
  expect(existsSync(dir)).toBe(false);
});

test("the next step names the package manager that ran new and quotes the dir", () => {
  const dir = join(scratch(), "my game");
  const r = spawnSync("node", [launcher, "new", dir, "--no-install"], {
    encoding: "utf8",
    env: { ...env, npm_config_user_agent: "pnpm/11.3.0 npm/? node/v24" },
  });
  expect(r.status, r.stderr).toBe(0);
  expect(r.stdout).toContain(`cd '${dir}' && pnpm install && npx bevy-ts dev src/game.ts`);
});

test("sync refuses an example whose entry would not ship", () => {
  const from = scratch();
  mkdirSync(join(from, "loose"));
  writeFileSync(join(from, "loose/main.ts"), "");
  writeFileSync(join(from, "loose/example.json"), JSON.stringify({ id: "loose", title: "L", description: "D", entry: "main.ts" }));
  const r = spawnSync("node", [join(pkg, "scripts/sync-examples.mjs"), "--from", from, "--to", join(scratch(), "out")], { encoding: "utf8" });
  expect(r.status).toBe(1);
  expect(r.stderr).toMatch(/entry main\.ts is not under src\/, assets\//);
});
