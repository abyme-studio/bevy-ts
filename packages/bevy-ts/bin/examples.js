// `bevy-ts examples` and `bevy-ts new`: the launcher's own commands. They read the examples this
// package ships (copied in at pack time by scripts/sync-examples.mjs) and never start the engine.
import { spawnSync } from "node:child_process";
import { cpSync, existsSync, mkdirSync, readdirSync, readFileSync, statSync, writeFileSync } from "node:fs";
import { basename, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const root = fileURLToPath(new URL("../examples/", import.meta.url));
const version = JSON.parse(readFileSync(new URL("../package.json", import.meta.url), "utf8")).version;
// What `new` leaves out of the copy: the metadata, not the game.
const META = ["example.json", "thumbnail.png"];

/** The shipped examples, in the order to show them: `cube` first, then by id. */
export function list() {
  if (!existsSync(root)) return [];
  const out = [];
  for (const name of readdirSync(root)) {
    const file = `${root}${name}/example.json`;
    if (!existsSync(file)) continue;
    const { id, title, description, entry } = JSON.parse(readFileSync(file, "utf8"));
    const e = { id, title, description, entry };
    if (existsSync(`${root}${name}/thumbnail.png`)) e.thumbnail = `${root}${name}/thumbnail.png`;
    e.dir = `${root}${name}`;
    out.push(e);
  }
  return out.sort((a, b) => (a.id === "cube" ? -1 : b.id === "cube" ? 1 : a.id < b.id ? -1 : 1));
}

function refuse(msg, code = 1) {
  console.error(`bevy-ts: ${msg}`);
  return code;
}

export function examples(args) {
  const all = list().map(({ dir, ...e }) => e);
  if (args.includes("--json")) {
    process.stdout.write(`${JSON.stringify({ version, examples: all })}\n`);
  } else {
    for (const e of all) console.log(`${e.id.padEnd(10)} ${e.title}: ${e.description}`);
  }
  return 0;
}

const NEW_USAGE = "usage: bevy-ts new <dir> [--example <id>] [--no-install] [--json]";

export function create(args) {
  let dir;
  let id = "cube";
  let install = true;
  let json = false;
  for (let i = 0; i < args.length; i++) {
    const a = args[i];
    if (a === "--example") {
      id = args[++i];
      if (!id || id.startsWith("--")) return refuse(`--example takes an id\n${NEW_USAGE}`, 2);
    } else if (a === "--no-install") install = false;
    else if (a === "--json") json = true;
    else if (a.startsWith("--")) return refuse(`unknown option ${a}\n${NEW_USAGE}`, 2);
    else if (dir === undefined) dir = a;
    else return refuse(`unexpected argument ${a}\n${NEW_USAGE}`, 2);
  }
  if (dir === undefined) return refuse(`missing <dir>\n${NEW_USAGE}`, 2);

  // Everything is checked before the first write, so a refusal writes nothing.
  const all = list();
  const example = all.find((e) => e.id === id);
  if (!example) return refuse(`no example "${id}"; there are: ${all.map((e) => e.id).join(", ")}`);
  const target = resolve(dir);
  if (existsSync(target)) {
    if (!statSync(target).isDirectory()) return refuse(`${target} is a file`);
    if (readdirSync(target).length) return refuse(`${target} is not empty; bevy-ts new makes a project only in an empty or missing directory`);
  }

  try {
    mkdirSync(target, { recursive: true });
    for (const f of readdirSync(example.dir)) {
      if (!META.includes(f)) cpSync(`${example.dir}/${f}`, `${target}/${f}`, { recursive: true });
    }
    const name = basename(target).toLowerCase().replace(/[^a-z0-9._-]+/g, "-").replace(/^[._-]+/, "") || "game";
    const pkg = {
      name,
      private: true,
      type: "module",
      scripts: { dev: `bevy-ts dev ${example.entry}` },
      devDependencies: { "bevy-ts": version },
    };
    // The examples' compiler options, resolving bevy-ts from node_modules instead of the checkout.
    const tsconfig = {
      compilerOptions: {
        target: "es2023",
        module: "esnext",
        moduleResolution: "bundler",
        strict: true,
        noEmit: true,
        verbatimModuleSyntax: true,
        jsx: "react-jsx",
        jsxImportSource: "bevy-ts",
        allowImportingTsExtensions: true,
      },
      include: ["src", ".bevy-ts/types"],
    };
    writeFileSync(`${target}/package.json`, `${JSON.stringify(pkg, null, 2)}\n`);
    writeFileSync(`${target}/tsconfig.json`, `${JSON.stringify(tsconfig, null, 2)}\n`);
    writeFileSync(`${target}/.gitignore`, "node_modules/\n.bevy-ts/\n");
  } catch (e) {
    return refuse(e.message);
  }

  const pm = packageManager();
  const installed = install ? runInstall(pm, target) : false;
  if (json) {
    process.stdout.write(`${JSON.stringify({ dir: target, entry: example.entry, example: id, installed })}\n`);
  } else {
    console.log(`Made ${target} from ${id}${install && !installed ? "; the install failed" : ""}.`);
    const steps = [`cd ${shellQuote(dir)}`, ...(installed ? [] : [`${pm} install`]), `npx bevy-ts dev ${example.entry}`];
    console.log(`  ${steps.join(" && ")}`);
  }
  return install && !installed ? 1 : 0;
}

/** The package manager that ran bevy-ts (npm, pnpm, yarn or bun), npm otherwise. */
function packageManager() {
  const agent = (process.env.npm_config_user_agent ?? "").split("/")[0];
  return ["npm", "pnpm", "yarn", "bun"].includes(agent) ? agent : "npm";
}

/** The path as it pastes into a POSIX shell: single-quoted when it holds anything but safe characters. */
function shellQuote(p) {
  return /^[\w@%+=:,./-]+$/.test(p) ? p : `'${p.replaceAll("'", "'\\''")}'`;
}

/** Installs with `pm` in `dir`, its output on stderr. */
function runInstall(pm, dir) {
  console.error(`bevy-ts: ${pm} install in ${dir}`);
  const r = spawnSync(pm, ["install"], { cwd: dir, stdio: ["ignore", 2, 2], shell: process.platform === "win32" });
  if (r.error || r.status !== 0) {
    console.error(`bevy-ts: ${pm} install failed${r.error ? `: ${r.error.message}` : ""}; the project is in place, run it again there`);
    return false;
  }
  return true;
}
