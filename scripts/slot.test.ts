// That every cargo run takes a check slot (docs/check-slots.md): each process in .abyme/processes.ts
// and each script, against a fake gate-slot that prints what it was asked to run.
//   deno test -A --no-check scripts/slot.test.ts
import { assert, assertEquals } from "jsr:@std/assert@1";
import processes from "../.abyme/processes.ts";

const root = new URL("..", import.meta.url).pathname;
const runsCargo = (cmd: string) => /\bcargo\b|scripts\/[\w-]+\.sh/.test(cmd);

/** A home whose dev/abyme/scripts/gate-slot prints its arguments as JSON and exits 1. */
async function fakeHome(): Promise<string> {
  const home = await Deno.makeTempDir();
  await Deno.mkdir(`${home}/dev/abyme/scripts`, { recursive: true });
  const fake = `${home}/dev/abyme/scripts/gate-slot`;
  await Deno.writeTextFile(fake, `#!/usr/bin/env python3\nimport json, sys\nprint(json.dumps(sys.argv[1:]))\nsys.exit(1)\n`);
  await Deno.chmod(fake, 0o755);
  return home;
}

async function run(cmd: string[], env: Record<string, string>) {
  const out = await new Deno.Command(cmd[0], { args: cmd.slice(1), cwd: root, env, stdout: "piped", stderr: "piped" }).output();
  return new TextDecoder().decode(out.stdout).trim();
}

const asked = (out: string): string[] => JSON.parse(out.split("\n")[0]);

Deno.test("every process that runs cargo takes a bevy-ts slot first", async () => {
  const home = await fakeHome();
  const env = { HOME: home, ABYME_GATE_HELD: "" };
  for (const [name, { cmd }] of Object.entries(processes)) {
    if (!runsCargo(cmd)) continue;
    // In bash and in fish, the operator's shell: the fake runs first, so nothing else does.
    for (const shell of [["bash"], ["fish", "--no-config"]]) {
      const args = asked(await run([...shell, "-c", cmd], env));
      // Any bevy-ts slot name does: the name only labels the run in the slots' queue.
      assertEquals([args[0], args[2]], ["run", "--"], `${name} in ${shell[0]}`);
      assert(/^bevy-ts:[\w.-]+$/.test(args[1]), `${name} in ${shell[0]}: slot ${args[1]}`);
      assert(runsCargo(args.slice(3).join(" ")), `${name} in ${shell[0]}: ${args}`);
    }
  }
});

Deno.test("every script that runs cargo takes its slot first", async () => {
  const home = await fakeHome();
  for await (const f of Deno.readDir(`${root}/scripts`)) {
    if (!f.name.endsWith(".sh") || f.name === "slot.sh") continue;
    const text = await Deno.readTextFile(`${root}/scripts/${f.name}`);
    if (!/^[^#]*\bcargo /m.test(text)) continue;
    const script = `${root}scripts/${f.name}`;
    const args = asked(await run([script, "--check"], { HOME: home, ABYME_GATE_HELD: "" }));
    assertEquals(args, ["run", `bevy-ts:${f.name.replace(/\.sh$/, "")}`, "--", script, "--check"]);
  }
});

Deno.test("a script under a held slot, or with no gate-slot, takes none", async () => {
  const home = await fakeHome();
  const dir = await Deno.makeTempDir();
  await Deno.copyFile(`${root}/scripts/slot.sh`, `${dir}/slot.sh`);
  await Deno.writeTextFile(`${dir}/probe.sh`, `#!/usr/bin/env bash\nsource "$(dirname "$0")/slot.sh"\necho ran "$@"\n`);
  await Deno.chmod(`${dir}/probe.sh`, 0o755);
  const probe = [`${dir}/probe.sh`, "a"];
  assertEquals(asked(await run(probe, { HOME: home, ABYME_GATE_HELD: "" })), ["run", "bevy-ts:probe", "--", `${dir}/probe.sh`, "a"]);
  assertEquals(await run(probe, { HOME: home, ABYME_GATE_HELD: "1" }), "ran a");
  assertEquals(await run(probe, { HOME: dir, ABYME_GATE_HELD: "" }), "ran a");
});

Deno.test("a gate that pipes its output exits with the gate's status, not tee's", async () => {
  // land reads a check's exit code: a pipe without pipefail turns a red gate green.
  for (const name of ["native-gate", "affected-gate", "levels-gate", "systems-gate", "test", "clippy"]) {
    const { cmd } = (processes as Record<string, { cmd: string }>)[name];
    // In bash, as gated() runs it: the operator's fish has no pipefail.
    assert(/ -- bash -c '.*set -o pipefail/.test(cmd), `${name}: ${cmd}`);
  }
  const red = await new Deno.Command("bash", { args: ["-c", "set -o pipefail; false | tee /dev/null"] }).output();
  assertEquals(red.code, 1);
});

