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

Deno.test("every process that runs cargo is heavy, or takes a bevy-ts slot itself", () => {
  for (const [name, p] of Object.entries(processes as Record<string, { cmd: string; heavy?: boolean }>)) {
    if (!runsCargo(p.cmd)) continue;
    // Heavy: abyme holds a slot while it runs. In bash, which has pipefail, as gated() writes it.
    if (p.heavy) assert(p.cmd.startsWith("bash -c '"), `${name}: ${p.cmd}`);
    // Else it wraps the step that builds (a server) or the whole run in gate-slot; any bevy-ts slot
    // name does, as the name only labels the run in the slots' queue.
    else assert(/scripts\/gate-slot run bevy-ts:/.test(p.cmd), `${name}: ${p.cmd}`);
  }
});

Deno.test("a heavy process runs in bash under the operator's shell", async () => {
  // abyme runs a heavy command as "$SHELL" -c "<cmd>"; gated() hands it to bash in both shells.
  const cmd = `bash -c ${"'set -o pipefail; echo ran | tee /dev/null; false | tee /dev/null'"}`;
  for (const shell of [["bash"], ["fish", "--no-config"]]) {
    const out = await new Deno.Command(shell[0], { args: [...shell.slice(1), "-c", cmd], stdout: "piped" }).output();
    assertEquals([new TextDecoder().decode(out.stdout).trim(), out.code], ["ran", 1], shell[0]);
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
    // Heavy, in bash, as gated() writes it: the operator's fish has no pipefail.
    const p = (processes as Record<string, { cmd: string; heavy?: boolean }>)[name];
    assert(p.heavy && /^bash -c '.*set -o pipefail/.test(cmd), `${name}: ${cmd}`);
  }
  const red = await new Deno.Command("bash", { args: ["-c", "set -o pipefail; false | tee /dev/null"] }).output();
  assertEquals(red.code, 1);
});

