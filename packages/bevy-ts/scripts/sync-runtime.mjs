// Copies the engine's JS runtime (crates/bevy-ts-v8/js) into `runtime/`, so `bevy-ts` imports
// the same code outside the engine (Node, vitest through `bevy-ts/testing`).
// `--check` fails when the copies differ.
import { readFileSync, writeFileSync } from "node:fs";
import { fileURLToPath } from "node:url";

const here = (p) => fileURLToPath(new URL(p, import.meta.url));
const files = ["core.js", "bevy.js", "jsx-runtime.js", "jsx-dev-runtime.js"];
let stale = [];
for (const f of files) {
  const src = readFileSync(here(`../../../crates/bevy-ts-v8/js/${f}`), "utf8");
  const out = `// @generated from crates/bevy-ts-v8/js/${f} by scripts/sync-runtime.mjs. Do not edit.\n${src}`;
  const dest = here(`../runtime/${f}`);
  let cur = "";
  try {
    cur = readFileSync(dest, "utf8");
  } catch {}
  if (cur !== out) {
    if (process.argv.includes("--check")) stale.push(f);
    else writeFileSync(dest, out);
  }
}
if (stale.length) {
  console.error(`runtime/ is stale (${stale.join(", ")}): run node scripts/sync-runtime.mjs`);
  process.exit(1);
}
