// Runs `bevy-ts transform <file>`: oxc type stripping plus the system declarations the engine
// reads, exactly as under `bevy-ts dev`.
import { execFileSync } from "node:child_process";
import { binary } from "../bin/resolve.js";

export const TS_FILE = /\.(c|m)?tsx?$/;

export function transform(file) {
  return execFileSync(binary(), ["transform", file], { encoding: "utf8", maxBuffer: 64 << 20 });
}
