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
  "cli-test": { cmd: "cargo test -p bevy-ts-cli 2>&1 | tee target/cli-test.log | grep -E \"^test result|FAILED|panicked|^error\"" },
  "tsx-test": { cmd: "cargo test -p bevy-ts-tsx -- --nocapture 2>&1 | tee target/tsx-test.log | tail -80" },
  "tsx-sites-bench": { cmd: "cargo test --release -p bevy-ts-cli --test tsx_sites -- --ignored --nocapture 2>&1 | tee target/tsx-sites.log | tail -20" },
  "tsx-scale-bench": { cmd: "cargo test --release -p bevy-ts-cli --test tsx_scale -- --ignored --nocapture --test-threads 1 2>&1 | tee target/tsx-scale.log | tail -20" },
  "arena-walk": { cmd: "cargo test --release -p bevy-ts-cli --test arena_walk -- --nocapture 2>&1 | grep -v ld64 | tee target/arena-walk.log | grep -E '^[0-9] |test |panicked'" },
  test: { cmd: "cargo test --workspace 2>&1 | tee target/test.log | grep -E \"^test result|FAILED|panicked\"" },
  "check-track-location": { cmd: "CARGO_TARGET_DIR=target/track-location cargo check -p bevy-ts --features track_location --all-targets" },
  clippy: { cmd: "cargo clippy --workspace --all-targets -- -D warnings 2>&1 | tee target/clippy.log | tail -40" },
  release: { cmd: "cargo build --release -p bevy-ts-cli" },
  // Native task 4.4: what one exclusive slot costs the parallel executor.
  "bench-slot": { cmd: "cargo run --release -p bevy-ts-cli --example slot_barrier | tee bench/change-detection/results/slot-barrier.md" },
  "bench-bob": { cmd: "cargo run --release -p bevy-ts-cli --example bob_bench | tee bench/change-detection/results/bob-frame.md" },
  skeleton: { cmd: "cargo run -p bevy-ts-cli -- skeleton --headless" },
  cube: { cmd: "cargo run -p bevy-ts-cli -- dev examples/cube/src/game.ts --headless" },
  // A project engine (native crate + Rapier) built and run under the dev supervisor, headless.
  // A ports-only project engine: Rapier adopted in bevy-ts.json, no Rust in the project.
  rapier: {
    cmd: "cargo build -p bevy-ts-cli && CARGO_TARGET_DIR=$PWD/target ./target/debug/bevy-ts dev examples/rapier/src/game.tsx --headless --remote-port 15791 2>&1 | tee target/rapier.log",
  },
  thrusters: {
    cmd: "cargo build -p bevy-ts-cli && CARGO_TARGET_DIR=$PWD/target ./target/debug/bevy-ts dev examples/thrusters/src/game.ts --headless --remote-port 15790 2>&1 | tee target/thrusters.log",
  },
  "viewport-test": { cmd: "cargo test -p bevy-ts-viewport -- --nocapture --test-threads=1 2>&1 | tee target/viewport-test.log | tail -60" },
  "viewport-one": { cmd: "cargo test -p bevy-ts-viewport --test viewport -- --nocapture --test-threads=1 $VIEWPORT_TEST 2>&1 | grep -v ld64 | tee target/viewport-one.log | tail -30" },
  "viewport-edit": { cmd: "cargo test -p bevy-ts-viewport --test edit -- --nocapture --test-threads=1 2>&1 | grep -v ld64 | tee target/viewport-edit.log | grep -E 'test |panicked|left|right|assert|^thread' | tail -40" },
  "viewport-roundtrip": { cmd: "cargo test -p bevy-ts-cli --test viewport_edit -- --nocapture 2>&1 | grep -v ld64 | tee target/viewport-roundtrip.log | grep -E 'test |panicked|left|right|assert|^thread|error' | tail -30" },
  "viewport-assets": { cmd: "cargo test -p bevy-ts-viewport --test assets --lib -- --nocapture --test-threads=1 2>&1 | grep -v ld64 | tee target/viewport-assets.log | grep -E 'test |panicked|left|right|assert|^thread' | tail -30" },
  "viewport-walk": { cmd: "cargo test -p bevy-ts-cli --test viewport_walk -- --ignored --nocapture 2>&1 | grep -v ld64 | tee target/viewport-walk.log | grep -E '^[0-9] |^stats|test |panicked|assert|^thread' | tail -30" },
  "viewport-walk-release": { cmd: "cargo test --release -p bevy-ts-cli --test viewport_walk -- --ignored --nocapture 2>&1 | grep -v ld64 | tee target/viewport-walk-release.log | grep -E '^[0-9] |^stats|test |panicked|assert|^thread' | tail -30" },
  "viewport-spike-short": { cmd: "cargo test -p bevy-ts-viewport --test spike -- --nocapture 2>&1 | tee target/viewport-spike-short.log | tail -40" },
  "viewport-latency": { cmd: "SPIKE_PRESSES=200 cargo test --release -p bevy-ts-viewport --test spike key_to_pixels -- --ignored --nocapture 2>&1 | grep -v ld64 | tee target/viewport-latency.log | tail -8" },
  "viewport-spike": { cmd: "SPIKE_SECS=60 cargo test --release -p bevy-ts-viewport --test spike -- --nocapture 2>&1 | tee target/viewport-spike.log | tail -30" },
  "viewport-gate": { cmd: "scripts/viewport-gate.sh" },
  // docs/research/viewport-pacing.md: the arena in edit mode at 120 Hz ticks, with the per-frame trace.
  // The same, alternating the engine's threads at default QoS (0) and user-interactive (1), so both see one machine load.
  "viewport-pacing-ab": { cmd: "bash -c 'cargo test --release -p bevy-ts-cli --test viewport_pacing --no-run 2>/dev/null; for q in 0 1 0 1 0 1; do echo qos=$q; BEVY_TS_VIEWPORT_QOS=$q cargo test --release -p bevy-ts-cli --test viewport_pacing -- --ignored --nocapture 2>&1 | grep -E fps; uptime; done 2>&1 | tee target/viewport-pacing-ab.log'" },
  "viewport-pacing": { cmd: "BEVY_TS_VIEWPORT_TRACE=$PWD/target/viewport-pacing.trace PACING_SAMPLE=$PWD/target/viewport-pacing.sample cargo test --release -p bevy-ts-cli --test viewport_pacing -- --ignored --nocapture 2>&1 | grep -v ld64 | tee target/viewport-pacing.log | grep -E '^engine|fps|^stats|test |panicked|assert|^thread' | tail -20" },
  // Change asset-preview: several viewports and preview scenes.
  // Change asset-preview, task 4.2; `preview-bench-threejs` runs while a three.js viewer shows the model.
  "preview-bench": { cmd: "PREVIEW_MODEL=$HOME/dev/spikes/mesh-conform/inputs/alicia.glb scripts/preview-bench.sh 2>&1 | tee -a target/preview-bench.log" },
  "preview-bench-idle": { cmd: "PREVIEW_MODEL=$HOME/dev/spikes/mesh-conform/inputs/alicia.glb scripts/preview-bench.sh idle idle-edit 2>&1 | tee -a target/preview-bench.log" },
  "three-viewer": { cmd: "cd bench/asset-preview && npm install --silent && cp $HOME/dev/spikes/mesh-conform/inputs/alicia.glb model.glb && python3 -m http.server 8791" },
  "preview-bench-threejs": { cmd: "PREVIEW_MODEL=$HOME/dev/spikes/mesh-conform/inputs/alicia.glb scripts/preview-bench.sh threejs 2>&1 | tee -a target/preview-bench.log" },
  "viewport-preview-cli": { cmd: "cargo test -p bevy-ts-cli --test viewport_preview -- --nocapture 2>&1 | grep -v ld64 | tee target/viewport-preview-cli.log | grep -E 'test |panicked|left|right|assert|^thread|error' | tail -30" },
  "viewport-preview": { cmd: "cargo test -p bevy-ts-viewport --test preview -- --nocapture --test-threads=1 2>&1 | grep -v ld64 | tee target/viewport-preview.log | grep -E 'test |panicked|left|right|assert|^thread' | tail -40" },
  // The same gate for levels task groups.
  "levels-bench": { cmd: "cargo test --release -p bevy-ts-cli --test levels_bench -- --ignored --nocapture 2>&1 | tee target/levels-bench.log | grep -E '^\\||test result|panicked'" },
  "levels-walks": { cmd: "cargo test --release -p bevy-ts-cli --test levels_walk --test arena_walk -- --nocapture --test-threads 1 2>&1 | grep -vE 'ld64|INFO|WARN|^\\s+\\||^\\s+=' | tee target/levels-walks.log | tail -60" },
  "levels-protocol": { cmd: "cargo test -p bevy-ts-cli --test levels_protocol 2>&1 | grep -v ld64 | tee target/levels-protocol.log | grep -E 'test |panicked|left|right|assert|^error|^thread' | tail -30" },
  "levels-unit": { cmd: "cargo test -p bevy-ts-levels 2>&1 | grep -v ld64 | tee target/levels-unit.log | grep -E '^test result|FAILED|panicked|^error|left|right' | tail -30" },
  "levels-gate": { cmd: "scripts/levels-gate.sh 2>&1 | tee target/levels-gate.log" },
  // The gate a native task group lands on.
  "native-gate": { cmd: "mkdir -p target && scripts/native-gate.sh 2>&1 | tee target/native-gate.log" },
  // The gate a systems-api task group lands on.
  "systems-gate": { cmd: "bash -c 'mkdir -p target && set -o pipefail && scripts/systems-gate.sh 2>&1 | tee target/systems-gate.log'" },
} satisfies ProcessFile;
