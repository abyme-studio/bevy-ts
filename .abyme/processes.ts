import type { ProcessFile } from "@abyme/processes";

export default {
  "bench-bindings-rust": {
    cmd: "cd bench/bindings/rust && cargo bench 2>&1 | tee ../results/rust-criterion.txt",
  },
  "bench-bindings-js": {
    cmd: "deno run --v8-flags=--expose-gc bench/bindings/fire-paths.js | tee bench/bindings/results/v8-jit.md",
  },
  "ports-proof": {
    cmd: "cd bench/ports && cargo test -- --nocapture 2>&1 | tee results/cargo-test.txt",
  },
  "dx-rust-check": {
    cmd: "cd bench/dx/rust && cargo check 2>&1 | tee ../results/rust-check.txt",
  },
  build: { cmd: "cargo build --workspace --all-targets" },
  "tsx-test": { cmd: "cargo test -p bevy-ts-tsx -- --nocapture 2>&1 | tee target/tsx-test.log | tail -80" },
  "tsx-sites-bench": { cmd: "cargo test --release -p bevy-ts-cli --test tsx_sites -- --ignored --nocapture 2>&1 | tee target/tsx-sites.log | tail -20" },
  "tsx-scale-bench": { cmd: "cargo test --release -p bevy-ts-cli --test tsx_scale -- --ignored --nocapture --test-threads 1 2>&1 | tee target/tsx-scale.log | tail -20" },
  test: { cmd: "cargo test --workspace 2>&1 | tee target/test.log | grep -E \"^test result|FAILED|panicked\"" },
  "check-track-location": { cmd: "CARGO_TARGET_DIR=target/track-location cargo check -p bevy-ts --features track_location --all-targets" },
  clippy: { cmd: "cargo clippy --workspace --all-targets -- -D warnings 2>&1 | tee target/clippy.log | tail -40" },
  release: { cmd: "cargo build --release -p bevy-ts-cli" },
  "bench-bob": { cmd: "cargo run --release -p bevy-ts-cli --example bob_bench | tee bench/change-detection/results/bob-frame.md" },
  skeleton: { cmd: "cargo run -p bevy-ts-cli -- skeleton --headless" },
  cube: { cmd: "cargo run -p bevy-ts-cli -- dev examples/cube/src/game.ts --headless" },
  // A project engine (native crate + Rapier) built and run under the dev supervisor, headless.
  // A ports-only project engine: Rapier adopted in bevy-ts.json, no Rust in the project.
  rapier: {
    cmd: "cargo build -p bevy-ts-cli && CARGO_TARGET_DIR=$PWD/target ./target/debug/bevy-ts dev examples/rapier/src/game.ts --headless --remote-port 15791 2>&1 | tee target/rapier.log",
  },
  thrusters: {
    cmd: "cargo build -p bevy-ts-cli && CARGO_TARGET_DIR=$PWD/target ./target/debug/bevy-ts dev examples/thrusters/src/game.ts --headless --remote-port 15790 2>&1 | tee target/thrusters.log",
  },
  "viewport-test": { cmd: "cargo test -p bevy-ts-viewport -- --nocapture --test-threads=1 2>&1 | tee target/viewport-test.log | tail -60" },
  "viewport-one": { cmd: "cargo test -p bevy-ts-viewport --test viewport -- --nocapture --test-threads=1 $VIEWPORT_TEST 2>&1 | grep -v ld64 | tee target/viewport-one.log | tail -30" },
  "viewport-spike-short": { cmd: "cargo test -p bevy-ts-viewport --test spike -- --nocapture 2>&1 | tee target/viewport-spike-short.log | tail -40" },
  "viewport-latency": { cmd: "SPIKE_PRESSES=200 cargo test --release -p bevy-ts-viewport --test spike key_to_pixels -- --ignored --nocapture 2>&1 | grep -v ld64 | tee target/viewport-latency.log | tail -8" },
  "viewport-spike": { cmd: "SPIKE_SECS=60 cargo test --release -p bevy-ts-viewport --test spike -- --nocapture 2>&1 | tee target/viewport-spike.log | tail -30" },
  "viewport-gate": { cmd: "scripts/viewport-gate.sh" },
} satisfies ProcessFile;
