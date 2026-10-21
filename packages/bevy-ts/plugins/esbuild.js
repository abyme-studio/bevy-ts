// `bevy-ts/esbuild`: an esbuild plugin that compiles TS through bevy-ts.
import { transform } from "./transform.js";

export default function bevyTs() {
  return {
    name: "bevy-ts",
    setup(build) {
      // A `.wgsl` import is a shader-file value, as in the engine; the WGSL is not loaded.
      build.onLoad({ filter: /\.wgsl$/ }, (args) => ({
        contents: `export default Object.freeze({ kind: "shaderFile", path: ${JSON.stringify(args.path)} });\n`,
        loader: "js",
      }));
      build.onLoad({ filter: /\.(c|m)?tsx?$/ }, (args) => {
        if (args.path.includes("/node_modules/")) return undefined;
        return { contents: transform(args.path), loader: "js" };
      });
    },
  };
}
