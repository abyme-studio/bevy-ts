// How bevy-ts lands a finished branch: abyme's land plugin (`abyme call land add`). A batch closes at
// 3 waiting, a quiet 10 minutes, or 45 minutes after its first branch joined, and lands unchecked
// (operator, 2026-10-26: landing takes minutes, a red main is cheap). The full run, `native-gate`,
// checks main's tip whole every 10 landings and when the 04:30 trigger calls `land verify`
// (.abyme/triggers.ts), reverting the first red landing.
import type { LandFile } from "@abyme/land";

export default {
  flush: { count: 3, quiet: "10m", max: "45m" },
  full: { check: "native-gate", every: 10 },
} satisfies LandFile;
