// Copies each example that ships (its directory under examples/ holds an `example.json`) into
// `examples/` here, for `bevy-ts new` and `bevy-ts examples`. Runs as `prepack`; the copy is
// gitignored. Fails on an example that declares itself but needs a project engine (anything under
// `native` in its bevy-ts.json): those ship once bevy-ts's crates are on crates.io.
import { cpSync, existsSync, mkdirSync, readdirSync, readFileSync, rmSync } from "node:fs";
import { fileURLToPath } from "node:url";

const here = (p) => fileURLToPath(new URL(p, import.meta.url));
// `--from <dir> --to <dir>` override the two, for tests.
const arg = (flag) => {
  const i = process.argv.indexOf(flag);
  return i < 0 ? undefined : `${process.argv[i + 1].replace(/\/$/, "")}/`;
};
const source = arg("--from") ?? here("../../../examples/");
const out = arg("--to") ?? here("../examples/");
// What a project needs from an example, and the metadata. Walks, WALK.md and tsconfig.json stay.
const SHIPPED = ["src", "assets", "bevy-ts.json", "README.md", "thumbnail.png", "example.json"];

/** bevy-ts.json allows `//` and block comments. */
function parseJsonc(text) {
  let s = "";
  for (let i = 0; i < text.length; i++) {
    const c = text[i];
    if (c === '"') {
      let j = i + 1;
      while (j < text.length && text[j] !== '"') j += text[j] === "\\" ? 2 : 1;
      s += text.slice(i, j + 1);
      i = j;
    } else if (c === "/" && text[i + 1] === "/") {
      while (i < text.length && text[i] !== "\n") i++;
      s += "\n";
    } else if (c === "/" && text[i + 1] === "*") {
      i = text.indexOf("*/", i + 2) + 1;
      if (i === 0) break;
    } else s += c;
  }
  return JSON.parse(s);
}

function fail(msg) {
  console.error(`sync-examples: ${msg}`);
  process.exit(1);
}

const ids = new Set();
const shipped = [];
for (const name of readdirSync(source).sort()) {
  const dir = `${source}${name}/`;
  if (!existsSync(`${dir}example.json`)) continue;
  const meta = JSON.parse(readFileSync(`${dir}example.json`, "utf8"));
  for (const key of ["id", "title", "description", "entry"]) {
    if (typeof meta[key] !== "string" || !meta[key]) fail(`examples/${name}/example.json needs "${key}"`);
  }
  if (!/^[a-z0-9-]+$/.test(meta.id)) fail(`examples/${name}: id "${meta.id}" is not [a-z0-9-]+`);
  if (ids.has(meta.id)) fail(`examples/${name}: id "${meta.id}" is taken`);
  ids.add(meta.id);
  if (!existsSync(`${dir}${meta.entry}`)) fail(`examples/${name}: entry ${meta.entry} does not exist`);
  if (!SHIPPED.some((f) => meta.entry.startsWith(`${f}/`))) {
    fail(`examples/${name}: entry ${meta.entry} is not under ${SHIPPED.filter((f) => !f.includes(".")).join("/, ")}/, so it would not ship`);
  }
  if (existsSync(`${dir}bevy-ts.json`)) {
    const native = parseJsonc(readFileSync(`${dir}bevy-ts.json`, "utf8")).native;
    if (native && Object.keys(native).length) {
      fail(`examples/${name} declares itself in example.json but needs a project engine (native in bevy-ts.json); it cannot ship`);
    }
  }
  shipped.push({ name, id: meta.id, dir });
}

rmSync(out, { recursive: true, force: true });
for (const { id, dir } of shipped) {
  mkdirSync(`${out}${id}`, { recursive: true });
  for (const f of SHIPPED) {
    if (existsSync(`${dir}${f}`)) cpSync(`${dir}${f}`, `${out}${id}/${f}`, { recursive: true });
  }
}
console.error(`sync-examples: ${shipped.map((e) => e.id).join(", ")}`);
