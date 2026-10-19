import type { ProcessFile } from "@abyme/processes";

export default {
  "bench-bindings-rust": {
    cmd: "cd bench/bindings/rust && cargo bench 2>&1 | tee ../results/rust-criterion.txt",
  },
  "bench-bindings-js": {
    cmd: "deno run --v8-flags=--expose-gc bench/bindings/fire-paths.js | tee bench/bindings/results/v8-jit.md",
  },
  build: { cmd: "cargo build --workspace --all-targets" },
  test: { cmd: "cargo test --workspace" },
  clippy: { cmd: "cargo clippy --workspace --all-targets -- -D warnings" },
  skeleton: { cmd: "cargo run -p bevy-ts-cli -- skeleton --headless" },
  cube: { cmd: "cargo run -p bevy-ts-cli -- dev examples/cube/src/game.ts --headless" },
} satisfies ProcessFile;
