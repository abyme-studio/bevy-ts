#!/usr/bin/env node
// Sets one version on bevy-ts and its platform packages, and lists in bevy-ts's
// optionalDependencies only the platforms given (those built, and on npm when publishing).
//   node scripts/set-npm-version.mjs 0.1.0-alpha.0 darwin-arm64,linux-x64
import { readFileSync, writeFileSync } from "node:fs";

const ALL = ["darwin-arm64", "darwin-x64", "linux-arm64", "linux-x64", "win32-x64"];
const [version, list] = process.argv.slice(2);
if (!version) {
  console.error("usage: set-npm-version.mjs <version> [platform,platform,...]");
  process.exit(1);
}
const platforms = list ? list.split(",").filter(Boolean) : ALL;
for (const p of platforms) {
  if (!ALL.includes(p)) {
    console.error(`unknown platform ${p}; known: ${ALL.join(", ")}`);
    process.exit(1);
  }
}

const edit = (path, f) => {
  const pkg = JSON.parse(readFileSync(path, "utf8"));
  f(pkg);
  writeFileSync(path, JSON.stringify(pkg, null, 2) + "\n");
};

for (const p of ALL) edit(`packages/platforms/${p}/package.json`, (pkg) => { pkg.version = version; });
edit("packages/bevy-ts/package.json", (pkg) => {
  pkg.version = version;
  pkg.optionalDependencies = Object.fromEntries(platforms.map((p) => [`@bevy-ts/${p}`, version]));
});
console.log(`bevy-ts ${version}, optionalDependencies: ${platforms.join(", ")}`);
