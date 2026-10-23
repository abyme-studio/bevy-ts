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
const every = "steps clippy test deno npm protocol bevy openspec scripts";

Deno.test("a crate brings every workspace crate that depends on it", async () => {
  assertEquals(await plan(["crates/b/src/lib.rs"]), ["crates b c", "steps clippy test"]);
  assertEquals(await plan(["crates/d/src/lib.rs"]), ["crates d", "steps clippy test"]);
});

Deno.test("a crate the cli depends on brings the npm and bevy-ts/bevy steps", async () => {
  assertEquals(await plan(["crates/a/src/lib.rs"]), ["crates a b bevy-ts-cli c", "steps clippy test npm bevy"]);
});

Deno.test("a path a step claims runs only that step", async () => {
  assertEquals(await plan(["openspec/specs/x/spec.md"]), ["crates", "steps openspec"]);
  assertEquals(await plan([".abyme/processes.ts"]), ["crates", "steps scripts"]);
  assertEquals(await plan(["docs/x.md", "README.md"]), ["crates", "steps"]);
});

Deno.test("a shared root or a path no step claims runs everything", async () => {
  for (const p of ["Cargo.lock", "scripts/x.sh", "protocol/x.json", ".cargo/config.toml", "elsewhere/x"]) {
    assertEquals(await plan([p]), [`full ${p}`, all, every], p);
  }
});

Deno.test("LAND_BASE sets what the change is measured from", async () => {
  const dir = await fixture();
  await Deno.writeTextFile(`${dir}/crates/d/src/lib.rs`, "// one\n");
  await sh(dir, "git -c user.name=t -c user.email=t@t commit -qam one");
  await Deno.writeTextFile(`${dir}/crates/b/src/lib.rs`, "// two\n");
  await sh(dir, "git -c user.name=t -c user.email=t@t commit -qam two");
  assertEquals((await sh(dir, "scripts/affected-gate.sh --plan")).split("\n"), ["crates b c d", "steps clippy test"]);
  assertEquals((await sh(dir, "scripts/affected-gate.sh --plan", { LAND_BASE: "HEAD~1" })).split("\n"), ["crates b c", "steps clippy test"]);
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
