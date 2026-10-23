// What abyme runs in this repo when something happens (the triggers skill).
import type { TriggerFile } from "@abyme/triggers";

export default {
  // The full run on main's tip (`full` in .abyme/land.ts): native-gate, the first red landing found
  // and reverted, a fix agent for it. At 04:30, off abyme's 04:00 run, while the operator sleeps.
  daily: { on: { cron: "30 4 * * *" }, run: { land: "verify" } },
} satisfies TriggerFile;
