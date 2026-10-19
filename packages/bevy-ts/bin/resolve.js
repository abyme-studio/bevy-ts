// Finds the engine binary: BEVY_TS_BINARY, else the platform package npm installed beside bevy-ts.
import { createRequire } from "node:module";

const PLATFORMS = {
  "darwin-arm64": "@bevy-ts/darwin-arm64",
  "darwin-x64": "@bevy-ts/darwin-x64",
  "linux-x64": "@bevy-ts/linux-x64",
  "linux-arm64": "@bevy-ts/linux-arm64",
  "win32-x64": "@bevy-ts/win32-x64",
};

export function binary() {
  if (process.env.BEVY_TS_BINARY) return process.env.BEVY_TS_BINARY;
  const key = `${process.platform}-${process.arch}`;
  const pkg = PLATFORMS[key];
  if (!pkg) throw new Error(`bevy-ts: no prebuilt engine for ${key}; supported: ${Object.keys(PLATFORMS).join(", ")}`);
  const exe = process.platform === "win32" ? "bevy-ts.exe" : "bevy-ts";
  try {
    return createRequire(import.meta.url).resolve(`${pkg}/bin/${exe}`);
  } catch {
    throw new Error(`bevy-ts: the engine package ${pkg} is not installed. Reinstall bevy-ts without --no-optional (npm i bevy-ts).`);
  }
}
