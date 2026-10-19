#!/usr/bin/env node
// Runs the precompiled engine binary from the platform package npm installed alongside this one
// (as esbuild and Biome ship), so `npx bevy-ts dev src/game.ts` needs no Rust toolchain.
// BEVY_TS_BINARY overrides the binary, for engine development.
import { spawn } from "node:child_process";
import { createRequire } from "node:module";

const PLATFORMS = {
  "darwin-arm64": "@bevy-ts/darwin-arm64",
  "darwin-x64": "@bevy-ts/darwin-x64",
  "linux-x64": "@bevy-ts/linux-x64",
  "linux-arm64": "@bevy-ts/linux-arm64",
  "win32-x64": "@bevy-ts/win32-x64",
};

function binary() {
  if (process.env.BEVY_TS_BINARY) return process.env.BEVY_TS_BINARY;
  const key = `${process.platform}-${process.arch}`;
  const pkg = PLATFORMS[key];
  if (!pkg) {
    console.error(`bevy-ts: no prebuilt engine for ${key}; supported: ${Object.keys(PLATFORMS).join(", ")}`);
    process.exit(1);
  }
  const exe = process.platform === "win32" ? "bevy-ts.exe" : "bevy-ts";
  try {
    return createRequire(import.meta.url).resolve(`${pkg}/bin/${exe}`);
  } catch {
    console.error(`bevy-ts: the engine package ${pkg} is not installed. Reinstall bevy-ts without --no-optional (npm i bevy-ts).`);
    process.exit(1);
  }
}

// Forward signals so stopping the launcher stops the engine too.
const child = spawn(binary(), process.argv.slice(2), { stdio: "inherit" });
for (const sig of ["SIGINT", "SIGTERM", "SIGHUP"]) process.on(sig, () => child.kill(sig));
child.on("error", (e) => {
  console.error(`bevy-ts: ${e.message}`);
  process.exit(1);
});
child.on("exit", (code, signal) => process.exit(code ?? (signal ? 1 : 0)));
