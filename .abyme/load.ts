// What abyme's load needs to know of bevy-ts: past 120 GB of worktree builds, load clears the least
// recently built worktree's cargo `target/` folders, never one an agent or process is in (storage
// audit, 2026-10-30).
import type { LoadFile } from "@abyme/load";

export default {
  quota: { disk: 120 },
  clear: ["target"],
} satisfies LoadFile;
