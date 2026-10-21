// `bevy-ts/vite`: a Vite (and vitest) plugin that compiles TS through bevy-ts, so systems carry
// their declarations outside the engine.
import { TS_FILE, transform } from "./transform.js";

export default function bevyTs() {
  return {
    name: "bevy-ts",
    enforce: "pre",
    // A `.wgsl` import is a shader-file value, as in the engine; the WGSL is not loaded.
    load(id) {
      // `./x.wgsl?raw` and `?url` keep Vite's own meaning.
      if (!id.endsWith(".wgsl")) return null;
      return `export default Object.freeze({ kind: "shaderFile", path: ${JSON.stringify(id)} });\n`;
    },
    transform(code, id) {
      const file = id.split("?")[0];
      if (!TS_FILE.test(file) || file.includes("/node_modules/")) return null;
      return { code: transform(file), map: null };
    },
  };
}
