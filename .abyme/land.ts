// How bevy-ts lands a finished branch: abyme's land plugin (`abyme call land add`). A batch closes at
// 5 waiting, a quiet 10 minutes, or 45 minutes after its first branch joined; then `affected-gate`
// (scripts/affected-gate.sh) checks the batch stacked on main, scoped by LAND_BASE. The full run,
// `native-gate`, checks main's tip whole when the 04:30 trigger calls `land verify`
// (.abyme/triggers.ts), reverting the first red landing. The land script (rjm:land) lands it too
// until callers switch.
import type { LandFile } from "@abyme/land";

export default {
  check: "affected-gate",
  flush: { count: 5, quiet: "10m", max: "45m" },
  full: { check: "native-gate" },
} satisfies LandFile;
