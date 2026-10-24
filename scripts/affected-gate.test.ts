// What affected-gate.sh runs for a change (docs/check-slots.md), from its --plan, in a fixture
// workspace of its own: a <- b <- c (c only as a dev-dependency), bevy-ts-cli <- a, and d alone.
//   deno test -A --no-check scripts/affected-gate.test.ts
import { assertEquals } from "jsr:@std/assert@1";

const root = new URL("..", import.meta.url).pathname;

const crates: Record<string, string> = {
  a: "",
  b: `[dependencies]\na = { path = "../a" }\n`,
  c: `[dev-dependencies]\nb = { path = "../b" }\n`,
  d: "",
  "bevy-ts-cli": `[dependencies]\na = { path = "../a" }\n`,
};

async function sh(dir: string, cmd: string, env: Record<string, string> = {}) {
  // The fixture's own base, never the LAND_BASE land runs the gate under.
  env = { LAND_BASE: "", ...env };
  const out = await new Deno.Command("bash", { args: ["-c", cmd], cwd: dir, env, stdout: "piped", stderr: "piped" }).output();
  const text = new TextDecoder().decode(out.stdout).trim();
  if (!out.success) throw new Error(`${cmd}: ${new TextDecoder().decode(out.stderr)}`);
  return text;
}

/** A git repo whose main holds the fixture workspace and the gate, checked out on a branch. */
async function fixture(): Promise<string> {
  const dir = await Deno.realPath(await Deno.makeTempDir());
  const members = Object.keys(crates).map((c) => `"crates/${c}"`).join(", ");
  await Deno.writeTextFile(`${dir}/Cargo.toml`, `[workspace]\nresolver = "3"\nmembers = [${members}]\n`);
  for (const [name, deps] of Object.entries(crates)) {
    await Deno.mkdir(`${dir}/crates/${name}/src`, { recursive: true });
    await Deno.writeTextFile(`${dir}/crates/${name}/Cargo.toml`, `[package]\nname = "${name}"\nversion = "0.1.0"\nedition = "2024"\n\n${deps}`);
    await Deno.writeTextFile(`${dir}/crates/${name}/src/lib.rs`, "");
  }
  await Deno.mkdir(`${dir}/scripts`);
  for (const f of ["affected-gate.sh", "slot.sh"]) await Deno.copyFile(`${root}scripts/${f}`, `${dir}/scripts/${f}`);
  await Deno.chmod(`${dir}/scripts/affected-gate.sh`, 0o755);
  await sh(dir, "git init -q -b main && git add -A && git -c user.name=t -c user.email=t@t commit -qm base && git checkout -qb work");
  return dir;
}

/** The plan for a branch that touches `paths`, committed or not. */
async function plan(paths: string[], env: Record<string, string> = {}) {
  const dir = await fixture();
  for (const p of paths) {
    await Deno.mkdir(`${dir}/${p}`.replace(/\/[^/]+$/, ""), { recursive: true });
    await Deno.writeTextFile(`${dir}/${p}`, "# changed\n", { append: true });
  }
  await sh(dir, "git add -A && git -c user.name=t -c user.email=t@t commit -qm change");
  return (await sh(dir, "scripts/affected-gate.sh --plan", env)).split("\n");
}

const all = "crates a b c d bevy-ts-cli";
const every = "steps clippy alone test deno npm protocol bevy openspec";
const aloneAll = "alone a b c d bevy-ts-cli";

Deno.test("a crate brings every workspace crate that depends on it", async () => {
  assertEquals(await plan(["crates/b/src/lib.rs"]), ["crates b c", "alone b", "steps clippy alone test"]);
  assertEquals(await plan(["crates/d/src/lib.rs"]), ["crates d", "alone d", "steps clippy alone test"]);
});

Deno.test("a crate the cli depends on brings the npm and bevy-ts/bevy steps", async () => {
  assertEquals(await plan(["crates/a/src/lib.rs"]), ["crates a b bevy-ts-cli c", "alone a", "steps clippy alone test npm bevy"]);
});

Deno.test("each changed crate is built alone, and every crate it reaches when a manifest changed", async () => {
  assertEquals(await plan(["crates/b/Cargo.toml"]), ["crates b c", "alone b c", "steps clippy alone test"]);
});

Deno.test("a path a step claims runs only that step", async () => {
  assertEquals(await plan(["openspec/specs/x/spec.md"]), ["crates", "steps openspec"]);
  assertEquals(await plan([".abyme/processes.ts"]), ["crates", "steps slot"]);
  assertEquals(await plan(["docs/x.md", "README.md", ".abyme/land.ts"]), ["crates", "steps"]);
});

Deno.test("a shared root or a path no step claims runs everything", async () => {
  for (const p of ["Cargo.lock", "protocol/x.json", ".cargo/config.toml", "elsewhere/x"]) {
    assertEquals(await plan([p]), [`full ${p}`, all, aloneAll, every], p);
  }
  // A script no rule names, with slot.test.ts, which checks every script.
  assertEquals(await plan(["scripts/x.sh"]), ["full scripts/x.sh", all, aloneAll, `${every} slot`]);
  assertEquals(await plan(["Cargo.lock", ".abyme/processes.ts"]), ["full Cargo.lock", all, aloneAll, `${every} slot`]);
  // --full, native-gate's daily run on main, runs both deno tests.
  const dir = await fixture();
  assertEquals((await sh(dir, "scripts/affected-gate.sh --full --plan")).split("\n"), ["full --full", all, aloneAll, `${every} slot affected`]);
});

Deno.test("a script runs its own step and slot.test.ts, not everything", async () => {
  assertEquals(await plan(["scripts/npm-await.sh"]), ["crates", "steps slot"]);
  assertEquals(await plan(["scripts/target-bench.sh"]), ["crates", "steps slot"]);
  assertEquals(await plan(["scripts/prune-tests.sh"]), ["crates", "steps slot"]);
  assertEquals(await plan(["scripts/linux-fonts.sh"]), ["crates", "steps slot"]);
  assertEquals(await plan(["scripts/gen-bevy-types.sh"]), ["crates", "steps bevy slot"]);
  assertEquals(await plan(["scripts/gen-protocol.sh"]), ["crates", "steps protocol slot"]);
  assertEquals(await plan(["scripts/affected-gate.sh", "scripts/affected-gate.test.ts"]), ["crates", "steps slot affected"]);
});

Deno.test("each deno test runs only for what it tests", async () => {
  assertEquals(await plan(["scripts/affected-gate.test.ts"]), ["crates", "steps affected"]);
  assertEquals(await plan(["scripts/slot.sh"]), ["crates", "steps slot"]);
  assertEquals(await plan(["crates/d/src/lib.rs", ".abyme/teams.ts"]), ["crates d", "alone d", "steps clippy alone test"]);
});

Deno.test("LAND_BASE sets what the change is measured from", async () => {
  const dir = await fixture();
  await Deno.writeTextFile(`${dir}/crates/d/src/lib.rs`, "// one\n");
  await sh(dir, "git -c user.name=t -c user.email=t@t commit -qam one");
  await Deno.writeTextFile(`${dir}/crates/b/src/lib.rs`, "// two\n");
  await sh(dir, "git -c user.name=t -c user.email=t@t commit -qam two");
  assertEquals((await sh(dir, "scripts/affected-gate.sh --plan")).split("\n"), ["crates b c d", "alone b d", "steps clippy alone test"]);
  assertEquals((await sh(dir, "scripts/affected-gate.sh --plan", { LAND_BASE: "HEAD~1" })).split("\n"), ["crates b c", "alone b", "steps clippy alone test"]);
});

Deno.test("a gate that cannot start exits 75", async () => {
  const dir = await fixture();
  const run = (env: Record<string, string>) =>
    new Deno.Command(`${dir}/scripts/affected-gate.sh`, { args: ["--plan"], cwd: dir, env, stdout: "null", stderr: "null" }).output();
  assertEquals((await run({ LAND_BASE: "no-such-ref" })).code, 75);
  assertEquals((await run({ PATH: "/usr/bin:/bin" })).code, 75);
});

Deno.test("a bad LAND_BASE exits 75 before it waits for a slot", async () => {
  const dir = await fixture();
  const home = await Deno.makeTempDir();
  await Deno.mkdir(`${home}/dev/abyme/scripts`, { recursive: true });
  await Deno.writeTextFile(`${home}/dev/abyme/scripts/gate-slot`, "#!/bin/sh\necho asked\nexit 1\n");
  await Deno.chmod(`${home}/dev/abyme/scripts/gate-slot`, 0o755);
  const env = { HOME: home, ABYME_GATE_HELD: "", LAND_BASE: "no-such-ref" };
  const out = await new Deno.Command(`${dir}/scripts/affected-gate.sh`, { cwd: dir, env, stdout: "piped", stderr: "null" }).output();
  assertEquals([out.code, new TextDecoder().decode(out.stdout)], [75, ""]);
});

Deno.test("a failed step's log is kept past the next run, the newest 10", async () => {
  const dir = await fixture();
  // A stand-in openspec that fails, as the one step this change runs.
  const bin = await Deno.makeTempDir();
  await Deno.writeTextFile(`${bin}/openspec`, "#!/bin/sh\necho 'the flaky one'\nexit 1\n");
  await Deno.chmod(`${bin}/openspec`, 0o755);
  await Deno.mkdir(`${dir}/openspec`);
  await Deno.writeTextFile(`${dir}/openspec/x.md`, "x\n");
  await Deno.writeTextFile(`${dir}/.gitignore`, "target/\n");
  await sh(dir, "git add -A && git -c user.name=t -c user.email=t@t commit -qm spec");
  const logs = `${dir}/target/affected-gate`;
  await Deno.mkdir(logs, { recursive: true });
  for (let i = 0; i < 12; i++) await Deno.writeTextFile(`${logs}/failed-2000010${i % 10}T00000${i}Z-old${i}.log`, "old\n");
  const env = { LAND_BASE: "", ABYME_GATE_HELD: "1", PATH: `${bin}:${Deno.env.get("PATH")}` };
  const out = await new Deno.Command(`${dir}/scripts/affected-gate.sh`, { cwd: dir, env, stdout: "piped", stderr: "piped" }).output();
  const text = new TextDecoder().decode(out.stdout);
  assertEquals(out.code, 1);
  const kept = text.match(/failed logs kept: (\S+-spec\.log)/)?.[1];
  assertEquals(kept !== undefined, true, text);
  assertEquals(await Deno.readTextFile(`${dir}/${kept}`), "the flaky one\n");
  const failed = [...Deno.readDirSync(logs)].filter((f) => f.name.startsWith("failed-")).map((f) => f.name);
  assertEquals(failed.length, 10);
  assertEquals(failed.some((f) => `target/affected-gate/${f}` === kept), true);
});
