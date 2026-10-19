// `bevy-ts/esbuild`: an esbuild plugin that compiles TS through bevy-ts.
import { transform } from "./transform.js";

export default function bevyTs() {
  return {
    name: "bevy-ts",
    setup(build) {
      build.onLoad({ filter: /\.(c|m)?tsx?$/ }, (args) => {
        if (args.path.includes("/node_modules/")) return undefined;
        return { contents: transform(args.path), loader: "js" };
      });
    },
  };
}
