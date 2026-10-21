import { fileURLToPath } from "node:url";
import bevyTs from "../../plugins/vite.js";

const pkg = (p) => fileURLToPath(new URL(`../../${p}`, import.meta.url));

export default {
  plugins: [bevyTs()],
  resolve: {
    alias: [
      { find: /^bevy-ts\/testing$/, replacement: pkg("testing.js") },
      { find: /^bevy-ts\/jsx-runtime$/, replacement: pkg("runtime/jsx-runtime.js") },
      { find: /^bevy-ts\/bevy$/, replacement: pkg("runtime/bevy.js") },
      { find: /^bevy-ts$/, replacement: pkg("runtime/core.js") },
    ],
  },
  // examples.test.ts spawns tsc and npm pack, slow on a loaded machine.
  test: { include: ["test/vitest/*.test.ts"], testTimeout: 60_000 },
};
