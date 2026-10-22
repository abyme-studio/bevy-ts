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
  // Every cli test binary, past the first failure, for a change that moves what several assert.
  "cli-test-all": { cmd: "bash -c 'cargo test -p bevy-ts-cli --no-fail-fast 2>&1 | grep -v ld64 | tee target/cli-test-all.log | grep -E \"^     Running|FAILED|panicked\"'" },
  "cli-test": { cmd: "cargo test -p bevy-ts-cli 2>&1 | tee target/cli-test.log | grep -E \"^test result|FAILED|panicked|^error\"" },
  // The npm package's vitest suite (examples, `new`), against the debug engine for the TS transform.
  "npm-test": { cmd: "bash -c 'set -o pipefail; cargo build -p bevy-ts-cli && cd packages/bevy-ts && pnpm install --frozen-lockfile && BEVY_TS_BINARY=$PWD/../../target/debug/bevy-ts pnpm test 2>&1 | tee ../../target/npm-test.log | tail -40'" },
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
  // Change build-command, task 5.4: a project engine built by `bevy-ts build`, its events kept.
  "build-thrusters": {
    cmd: "bash -c 'cargo build -p bevy-ts-cli && CARGO_TARGET_DIR=$PWD/target ./target/debug/bevy-ts build examples/thrusters --json --out target/thrusters-dist > target/build-thrusters.jsonl; echo exit $?; grep -c progress target/build-thrusters.jsonl; grep -v log target/build-thrusters.jsonl | tail -8'",
  },
  // The arena built and played headless, a screenshot at frame 60 (its game camera, not the editor's).
  "arena-shot": {
    cmd: "bash -c 'cargo build -p bevy-ts-cli && rm -f target/arena-shot.png && CARGO_TARGET_DIR=$PWD/target ./target/debug/bevy-ts build examples/arena --out target/arena-dist > target/arena-shot.log 2>&1 && BEVY_TS_SCREENSHOT=$PWD/target/arena-shot.png timeout 60 ./target/debug/bevy-ts play target/arena-dist/game --headless >> target/arena-shot.log 2>&1; ls -la target/arena-dist target/arena-shot.png; tail -5 target/arena-shot.log'",
  },
  "viewport-test": { cmd: "cargo test -p bevy-ts-viewport -- --nocapture --test-threads=1 2>&1 | tee target/viewport-test.log | tail -60" },
  "viewport-one": { cmd: "cargo test -p bevy-ts-viewport --test viewport -- --nocapture --test-threads=1 $VIEWPORT_TEST 2>&1 | grep -v ld64 | tee target/viewport-one.log | tail -30" },
  "viewport-edit": { cmd: "cargo test -p bevy-ts-viewport --test edit -- --nocapture --test-threads=1 2>&1 | grep -v ld64 | tee target/viewport-edit.log | grep -E 'test |panicked|left|right|assert|^thread' | tail -40" },
  "viewport-roundtrip": { cmd: "cargo test -p bevy-ts-cli --test viewport_edit -- --nocapture 2>&1 | grep -v ld64 | tee target/viewport-roundtrip.log | grep -E 'test |panicked|left|right|assert|^thread|error' | tail -30" },
  "viewport-assets": { cmd: "cargo test -p bevy-ts-viewport --test assets --lib -- --nocapture --test-threads=1 2>&1 | grep -v ld64 | tee target/viewport-assets.log | grep -E 'test |panicked|left|right|assert|^thread' | tail -30" },
  "viewport-walk": { cmd: "cargo test -p bevy-ts-cli --test viewport_walk -- --ignored --nocapture 2>&1 | grep -v ld64 | tee target/viewport-walk.log | grep -E '^[0-9] |^stats|test |panicked|assert|^thread' | tail -30" },
  // The arena lit in play, edit, then play again (black meshes after edit mode).
  "play-after-edit": { cmd: "cargo test -p bevy-ts-cli --test play_after_edit -- --ignored --nocapture 2>&1 | grep -v ld64 | tee target/play-after-edit.log | grep -E 'black|test |panicked|assert|^thread|^error' | tail -30" },
  "viewport-walk-release": { cmd: "cargo test --release -p bevy-ts-cli --test viewport_walk -- --ignored --nocapture 2>&1 | grep -v ld64 | tee target/viewport-walk-release.log | grep -E '^[0-9] |^stats|test |panicked|assert|^thread' | tail -30" },
  "viewport-spike-short": { cmd: "cargo test -p bevy-ts-viewport --test spike -- --nocapture 2>&1 | tee target/viewport-spike-short.log | tail -40" },
  "viewport-latency": { cmd: "SPIKE_PRESSES=200 cargo test --release -p bevy-ts-viewport --test spike key_to_pixels -- --ignored --nocapture 2>&1 | grep -v ld64 | tee target/viewport-latency.log | tail -8" },
  "viewport-spike": { cmd: "SPIKE_SECS=60 cargo test --release -p bevy-ts-viewport --test spike -- --nocapture 2>&1 | tee target/viewport-spike.log | tail -30" },
  "viewport-gate": { cmd: "scripts/viewport-gate.sh" },
  // docs/research/viewport-pacing.md: the arena in edit mode at 120 Hz ticks, with the per-frame trace.
  // The same, alternating the earlier pacing (BEVY_TS_VIEWPORT_PACING=0) and the current (1), so both see one machine load.
  "viewport-pacing-ab": { cmd: "bash -c 'cargo test --release -p bevy-ts-cli --test viewport_pacing --no-run 2>/dev/null; for q in 0 1 0 1 0 1; do echo pacing=$q; BEVY_TS_VIEWPORT_PACING=$q cargo test --release -p bevy-ts-cli --test viewport_pacing -- --ignored --nocapture 2>&1 | grep -E fps; uptime; done 2>&1 | tee target/viewport-pacing-ab.log'" },
  // docs/research/viewport-tools.md: the same, alternating without the orientation gizmo (BEVY_TS_VIEWPORT_ORIENT=0) and with it.
  "viewport-tools-ab": { cmd: "bash -c 'cargo build --release -p bevy-ts-cli && cargo test --release -p bevy-ts-cli --test viewport_pacing --no-run 2>/dev/null; for o in 0 1 0 1 0 1; do echo orient=$o; BEVY_TS_VIEWPORT_ORIENT=$o BEVY_TS_BINARY=$PWD/target/release/bevy-ts cargo test --release -p bevy-ts-cli --test viewport_pacing -- --ignored --nocapture 2>&1 | grep -E \"^edit (at rest|pointer)|panicked|never answered\"; uptime; done 2>&1 | tee target/viewport-tools-ab.log'" },
  "viewport-pacing": { cmd: "PACING_ENGINE_LOG=$PWD/target/viewport-pacing-engine.log BEVY_TS_VIEWPORT_TRACE=$PWD/target/viewport-pacing.trace PACING_SAMPLE=$PWD/target/viewport-pacing.sample cargo test --release -p bevy-ts-cli --test viewport_pacing -- --ignored --nocapture 2>&1 | grep -v ld64 | tee target/viewport-pacing.log | grep -E '^engine|fps|^stats|test |panicked|assert|^thread' | tail -20" },
  // Change asset-preview: several viewports and preview scenes.
  // Change asset-preview, task 4.2; `preview-bench-threejs` runs while a three.js viewer shows the model.
  "preview-bench": { cmd: "PREVIEW_MODEL=$HOME/dev/spikes/mesh-conform/inputs/alicia.glb scripts/preview-bench.sh 2>&1 | tee -a target/preview-bench.log" },
  "preview-bench-idle": { cmd: "PREVIEW_MODEL=$HOME/dev/spikes/mesh-conform/inputs/alicia.glb scripts/preview-bench.sh idle idle-edit 2>&1 | tee -a target/preview-bench.log" },
  "three-viewer": { cmd: "cd bench/asset-preview && npm install --silent && cp $HOME/dev/spikes/mesh-conform/inputs/alicia.glb model.glb && python3 -m http.server 8791" },
  "preview-bench-threejs": { cmd: "PREVIEW_MODEL=$HOME/dev/spikes/mesh-conform/inputs/alicia.glb scripts/preview-bench.sh threejs 2>&1 | tee -a target/preview-bench.log" },
  "viewport-preview-cli": { cmd: "cargo test -p bevy-ts-cli --test viewport_preview -- --nocapture 2>&1 | grep -v ld64 | tee target/viewport-preview-cli.log | grep -E 'test |panicked|left|right|assert|^thread|error' | tail -30" },
  "viewport-preview": { cmd: "cargo test -p bevy-ts-viewport --test preview -- --nocapture --test-threads=1 2>&1 | grep -v ld64 | tee target/viewport-preview.log | grep -E 'test |panicked|left|right|assert|^thread' | tail -40" },
  // Change asset-catalog: level thumbnails that the game never sees, and what they cost (task 3.4).
  "viewport-thumbnail": { cmd: "cargo test -p bevy-ts-cli --test viewport_thumbnail -- --nocapture 2>&1 | grep -v ld64 | tee target/viewport-thumbnail.log | grep -E 'test |panicked|left|right|assert|^thread|error' | tail -30" },
  "thumbnail-bench": { cmd: "uptime && cargo test --release -p bevy-ts-cli --test viewport_thumbnail -- --ignored --nocapture 2>&1 | grep -v ld64 | tee target/thumbnail-bench.log | grep -E '^\\||test |panicked' | tail -20 && uptime" },
  "gen-protocol": { cmd: "scripts/gen-protocol.sh 2>&1 | tail -5" },
  // Every test the change touches, in one run: viewport, levels, tsx unit, and the cli tests on thumbnails and levels.
  "asset-catalog-tests": { cmd: "bash -c 'set -o pipefail; (cargo test -p bevy-ts-viewport --lib --test assets --test edit -- --test-threads=1 && cargo test -p bevy-ts-levels && cargo test -p bevy-ts-tsx --lib && cargo test -p bevy-ts-cli --test viewport_thumbnail --test viewport_level --test viewport_edit --test viewport_preview --test levels_protocol) 2>&1 | grep -v ld64 | tee target/asset-catalog-tests.log | grep -E \"^test result|FAILED|panicked|^error\"'" },
  // The last round before asset-catalog lands: everything, once.
  "asset-catalog-gate": { cmd: "bash -c 'set -o pipefail; mkdir -p target; (cargo build --workspace --all-targets && cargo test --workspace && cargo clippy --workspace --all-targets -- -D warnings && scripts/gen-protocol.sh --check && (cd packages/bevy-ts && pnpm check)) 2>&1 | grep -v ld64 | tee target/asset-catalog-gate.log | grep -E \"^test result|FAILED|panicked|^error|stale\"; echo gate exit $?'" },
  // Change level-asset-root: the writer and the level a dropped glTF composes into.
  "level-asset-root-tests": { cmd: "bash -c 'set -o pipefail; (cargo test -p bevy-ts-levels && cargo test -p bevy-ts-cli --test viewport_level --test viewport_thumbnail --test levels_protocol) 2>&1 | grep -v ld64 | tee target/level-asset-root-tests.log | grep -E \"^test |^test result|FAILED|panicked|^error|left|right\"'" },
  "level-asset-root-gate": { cmd: "bash -c 'set -o pipefail; mkdir -p target; (cargo build --workspace --all-targets && cargo test --workspace && cargo clippy --workspace --all-targets -- -D warnings && scripts/gen-protocol.sh --check) 2>&1 | grep -v ld64 | tee target/level-asset-root-gate.log | grep -E \"^test result|FAILED|panicked|^error|stale\"'" },
  // Change level-open: the last round before it lands.
  // Viewport's render tests run one at a time, as viewport-test does: in parallel a frame can miss its 5 s budget.
  "level-open-gate": { cmd: "bash -c 'set -o pipefail; mkdir -p target; (cargo build --workspace --all-targets && cargo test --workspace --exclude bevy-ts-viewport && cargo test -p bevy-ts-viewport -- --test-threads=1 && cargo clippy --workspace --all-targets -- -D warnings && scripts/gen-protocol.sh --check && (cd packages/bevy-ts && pnpm check)) 2>&1 | grep -v ld64 | tee target/level-open-gate.log | grep -E \"^test result|FAILED|panicked|^error|stale\"'" },
  // Change touch-time: the last round before it lands.
  "touch-time-gate": { cmd: "bash -c 'set -o pipefail; mkdir -p target; (cargo build --workspace --all-targets && cargo test --workspace --exclude bevy-ts-viewport && cargo test -p bevy-ts-viewport -- --test-threads=1 && cargo clippy --workspace --all-targets -- -D warnings && scripts/gen-protocol.sh --check && (cd packages/bevy-ts && pnpm check)) 2>&1 | grep -v ld64 | tee target/touch-time-gate.log | grep -E \"^test result|FAILED|panicked|^error|stale\"'" },
  // Change editor-state: the last round before it lands, as level-open's.
  "editor-state-gate": { cmd: "bash -c 'set -o pipefail; mkdir -p target; (cargo build --workspace --all-targets && cargo test --workspace --exclude bevy-ts-viewport && cargo test -p bevy-ts-viewport -- --test-threads=1 && cargo clippy --workspace --all-targets -- -D warnings && scripts/gen-protocol.sh --check && (cd packages/bevy-ts && pnpm check)) 2>&1 | grep -v ld64 | tee target/editor-state-gate.log | grep -E \"^test result|FAILED|panicked|^error|stale\"'" },
  // Change level-placeholders: the last round before it lands.
  "level-placeholders-gate": { cmd: "bash -c 'set -o pipefail; mkdir -p target; (cargo build --workspace --all-targets && cargo test --workspace --exclude bevy-ts-viewport && cargo test -p bevy-ts-viewport -- --test-threads=1 && cargo clippy --workspace --all-targets -- -D warnings && scripts/gen-protocol.sh --check && (cd packages/bevy-ts && pnpm check)) 2>&1 | grep -v ld64 | tee target/level-placeholders-gate.log | grep -E \"^test result|FAILED|panicked|^error|stale\"'" },
  // Change placeholder-errors: the last round before it lands, as level-placeholders'.
  "placeholder-errors-gate": { cmd: "bash -c 'set -o pipefail; mkdir -p target; (cargo build --workspace --all-targets && cargo test --workspace --exclude bevy-ts-viewport && cargo test -p bevy-ts-viewport -- --test-threads=1 && cargo clippy --workspace --all-targets -- -D warnings && scripts/gen-protocol.sh --check && (cd packages/bevy-ts && pnpm check)) 2>&1 | grep -v ld64 | tee target/placeholder-errors-gate.log | grep -E \"^test result|FAILED|panicked|^error|stale\"'" },
  "arena-assets": { cmd: "BEVY_TS_WRITE_ARENA_ASSETS=1 cargo test -p bevy-ts-viewport --test assets the_arenas_assets -- --nocapture 2>&1 | grep -v ld64 | tail -5" },
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
  // Change script-materials.
  "material-test": { cmd: "cargo test -p bevy-ts-material -- --nocapture 2>&1 | grep -v ld64 | tee target/material-test.log | grep -E '^test |test result|panicked|^error|^warning|left|right' | tail -40" },
  "materials-cli": { cmd: "cargo test -p bevy-ts-cli --test materials --test material_errors --test tsx -- --nocapture --test-threads 1 2>&1 | grep -v ld64 | tee target/materials-cli.log | grep -E '^test |test result|panicked|^error|left|right|assert' | tail -40" },
  "glow-walk": { cmd: "cargo build -p bevy-ts-cli && node examples/cube/glow-walk.mjs target/debug/bevy-ts 2>&1 | tee target/glow-walk.log" },
  // docs/build-cache.md: cold workspace build without sccache, then with it warm, 3 per arm.
  "sccache-bench": { cmd: "bash -c 'set -o pipefail; mkdir -p bench/results && scripts/sccache-bench.sh 3 2>&1 | tee -a bench/results/sccache.txt'" },
  "sccache-bench-ab": { cmd: "bash -c 'set -o pipefail; echo ab8; scripts/sccache-bench.sh 8 ab 2>&1 | tee -a bench/results/sccache.txt'" },
  // Change dev-edit: the crates it touches, then the CLI tests on the --edit boot and the examples it moved into scenes.
  "dev-edit-gate": { cmd: "bash -c 'set -o pipefail; mkdir -p target; (cargo build --workspace --all-targets && cargo test --workspace && cargo clippy --workspace --all-targets -- -D warnings && scripts/gen-protocol.sh --check && (cd packages/bevy-ts && pnpm check)) 2>&1 | grep -v ld64 | tee target/dev-edit-gate.log | grep -E \"^test result|FAILED|panicked|^error|stale\"'" },
  "dev-edit-levels-x5": { cmd: "bash -c 'for i in 1 2 3 4 5; do cargo test -p bevy-ts-cli --test levels_protocol 2>&1 | grep -E \"^test result|panicked\"; done 2>&1 | tee target/dev-edit-levels-x5.log'" },
  // The gate's steps after the tests, for a lint fix that changes no behaviour.
  "dev-edit-lint": { cmd: "bash -c 'set -o pipefail; (cargo clippy --workspace --all-targets -- -D warnings && scripts/gen-protocol.sh --check && (cd packages/bevy-ts && pnpm check)) 2>&1 | grep -v ld64 | tee target/dev-edit-lint.log | tail -30'" },
  "dev-edit-cli": { cmd: "bash -c 'set -o pipefail; cargo test -p bevy-ts-cli --no-fail-fast --test dev_edit --test play --test protocol --test viewport_edit 2>&1 | grep -v ld64 | tee target/dev-edit-cli.log | grep -E \"^test |^test result|panicked|^error|left|right|never\"'" },
  "dev-edit-tests": { cmd: "bash -c 'set -o pipefail; mkdir -p target; (cargo test -p bevy-ts -p bevy-ts-remote && cargo test -p bevy-ts-viewport --test edit -- --test-threads=1 && cargo test -p bevy-ts-cli --test dev_edit --test protocol --test play --test viewport_edit) 2>&1 | grep -v ld64 | tee target/dev-edit-tests.log | grep -E \"^test result|FAILED|panicked|^error|left|right\"'" },
} satisfies ProcessFile;
