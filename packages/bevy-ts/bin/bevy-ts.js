#!/usr/bin/env node
// Runs the precompiled engine binary from the platform package npm installed alongside this one
// (as esbuild and Biome ship), so `npx bevy-ts dev src/game.ts` needs no Rust toolchain.
// BEVY_TS_BINARY overrides the binary, for engine development. `new` and `examples` run here, in
// Node, with no engine.
import { spawn } from "node:child_process";
import { create, examples } from "./examples.js";
import { binary } from "./resolve.js";

// exitCode, not exit(): stdout to a pipe is written asynchronously and must drain first.
const [command, ...rest] = process.argv.slice(2);
if (command === "new") process.exitCode = create(rest);
else if (command === "examples") process.exitCode = examples(rest);
else runEngine();

function binaryOrExit() {
  try {
    return binary();
  } catch (e) {
    console.error(e.message);
    process.exit(1);
  }
}

function runEngine() {
  // Forward signals so stopping the launcher stops the engine too.
  const child = spawn(binaryOrExit(), process.argv.slice(2), { stdio: "inherit" });
  for (const sig of ["SIGINT", "SIGTERM", "SIGHUP"]) process.on(sig, () => child.kill(sig));
  child.on("error", (e) => {
    console.error(`bevy-ts: ${e.message}`);
    process.exit(1);
  });
  child.on("exit", (code, signal) => process.exit(code ?? (signal ? 1 : 0)));
}
