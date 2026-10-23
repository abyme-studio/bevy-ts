import type { ProcessFile } from "@abyme/processes";

// Every process that runs cargo, itself or through a script, runs inside one of the machine's check
// slots, named bevy-ts:<process> (docs/check-slots.md); `gated` below wraps it. A long-running
// server holds a slot for its build only, so its entry wraps that step itself.
const processes = {
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
  // Change mesh-picking: its crates built, then the headless click and the switch's tests.
  // CI's headless-linux job in an OrbStack container: target and registry in volumes of their own.
  "mesh-picking-linux": { cmd: "bash -c 'set -o pipefail; docker run --rm --pull always -v $PWD:/src -v bevy-ts-linux-target:/target -v bevy-ts-linux-cargo:/usr/local/cargo/registry -e CARGO_TARGET_DIR=/target -w /src rust:1-bookworm bash -c \"apt-get update -qq && apt-get install -y -qq libasound2-dev libudev-dev libwayland-dev libxkbcommon-dev pkg-config mesa-vulkan-drivers >/dev/null && cargo test -p bevy-ts-cli --test headless_pointer\" 2>&1 | tee target/mesh-picking-linux.log | grep -E \"^error|^test |test result|panicked\"'" },
  "mesh-picking-gate": { cmd: "bash -c 'set -o pipefail; mkdir -p target; (cargo build --workspace --all-targets && cargo test --workspace --exclude bevy-ts-viewport && cargo test -p bevy-ts-viewport -- --test-threads=1 && cargo clippy --workspace --all-targets -- -D warnings && scripts/gen-protocol.sh --check && (cd packages/bevy-ts && pnpm check)) 2>&1 | grep -v ld64 | tee target/mesh-picking-gate.log | grep -E \"^test result|FAILED|panicked|^error|stale\"'" },
  // The gate's steps after the workspace tests, for a rerun that stopped in bevy-ts-viewport.
  "mesh-picking-gate-rest": { cmd: "bash -c 'set -o pipefail; mkdir -p target; (cargo test -p bevy-ts-viewport -- --test-threads=1 && cargo test -p bevy-ts-cli --test mesh_picking --test headless_pointer && cargo clippy --workspace --all-targets -- -D warnings && scripts/gen-protocol.sh --check && (cd packages/bevy-ts && pnpm check)) 2>&1 | grep -v ld64 | tee target/mesh-picking-gate-rest.log | grep -E \"^test result|FAILED|panicked|^error|stale|^warning\"'" },
  "mesh-picking-test": { cmd: "bash -c 'set -o pipefail; cargo test --no-fail-fast -p bevy-ts-viewport -p bevy-ts-cli --lib --test headless_pointer --test mesh_picking --test play --test build 2>&1 | grep -v ld64 | tee target/mesh-picking-test.log | grep -E \"^error|^warning: unused|^test |test result|panicked\"'" },
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
  // The newest release.yml run on this branch (a dispatched dry run), watched to its end.
  "release-run": { cmd: "bash -c 'id=$(gh run list --workflow release.yml --branch \"$(git branch --show-current)\" -L 1 --json databaseId --jq \".[0].databaseId\") && gh run watch \"$id\" --exit-status --interval 30 | tail -25'" },
  // Native task 4.4: what one exclusive slot costs the parallel executor.
  "bench-slot": { cmd: "cargo run --release -p bevy-ts-cli --example slot_barrier | tee bench/change-detection/results/slot-barrier.md" },
  "bench-bob": { cmd: "cargo run --release -p bevy-ts-cli --example bob_bench | tee bench/change-detection/results/bob-frame.md" },
  skeleton: { cmd: "$HOME/dev/abyme/scripts/gate-slot run bevy-ts:skeleton -- cargo build -p bevy-ts-cli && ./target/debug/bevy-ts skeleton --headless" },
  cube: { cmd: "$HOME/dev/abyme/scripts/gate-slot run bevy-ts:cube -- cargo build -p bevy-ts-cli && ./target/debug/bevy-ts dev examples/cube/src/game.ts --headless" },
  // A project engine (native crate + Rapier) built and run under the dev supervisor, headless.
  // A ports-only project engine: Rapier adopted in bevy-ts.json, no Rust in the project.
  rapier: {
    cmd: "$HOME/dev/abyme/scripts/gate-slot run bevy-ts:rapier -- cargo build -p bevy-ts-cli && CARGO_TARGET_DIR=$PWD/target ./target/debug/bevy-ts dev examples/rapier/src/game.tsx --headless --remote-port 15791 2>&1 | tee target/rapier.log",
  },
  thrusters: {
    cmd: "$HOME/dev/abyme/scripts/gate-slot run bevy-ts:thrusters -- cargo build -p bevy-ts-cli && CARGO_TARGET_DIR=$PWD/target ./target/debug/bevy-ts dev examples/thrusters/src/game.ts --headless --remote-port 15790 2>&1 | tee target/thrusters.log",
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
  "play-after-edit": { cmd: "cargo test -p bevy-ts-cli --test play_after_edit -- --ignored --nocapture 2>&1 | grep -v ld64 | tee target/play-after-edit.log | grep -E 'black|plain|test |panicked|assert|^thread|^error' | tail -30" },
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
  // Change script-packages: the cli and v8 crates' build, then the tests it touches.
  "script-packages-build": { cmd: "bash -c 'set -o pipefail; mkdir -p target; cargo build -p bevy-ts-cli -p bevy-ts-v8 --tests 2>&1 | grep -v ld64 | tee target/script-packages-build.log | grep -E \"^(error|warning)\" -A12 | head -200'" },
  "script-packages-tests": { cmd: "bash -c 'set -o pipefail; mkdir -p target; (cargo test -p bevy-ts-cli --lib; cargo test -p bevy-ts-cli --no-fail-fast --test packages --test build --test play --test reload --test systems --test materials; cargo test -p bevy-ts-v8) 2>&1 | grep -v ld64 | tee target/script-packages-tests.log | grep -E \"^test result|FAILED|panicked|^error|left|right\"'" },
  "script-packages-gate": { cmd: "bash -c 'set -o pipefail; mkdir -p target; (cargo build --workspace --all-targets && cargo test --workspace --exclude bevy-ts-viewport && cargo test -p bevy-ts-viewport -- --test-threads=1 && cargo clippy --workspace --all-targets -- -D warnings && scripts/gen-protocol.sh --check && (cd packages/bevy-ts && pnpm check)) 2>&1 | grep -v ld64 | tee target/script-packages-gate.log | grep -E \"^test result|FAILED|panicked|^error|stale\"; echo gate exit $?'" },
  // Fix stamp-linear: the last round before it lands.
  "stamp-linear-gate": { cmd: "bash -c 'set -o pipefail; mkdir -p target; (cargo build --workspace --all-targets && cargo test --workspace --exclude bevy-ts-viewport && cargo test -p bevy-ts-viewport -- --test-threads=1 && cargo clippy --workspace --all-targets -- -D warnings) 2>&1 | grep -v ld64 | tee target/stamp-linear-gate.log | grep -E \"^test result|FAILED|panicked|^error\"'" },
  "arena-assets": { cmd: "BEVY_TS_WRITE_ARENA_ASSETS=1 cargo test -p bevy-ts-viewport --test assets the_arenas_assets -- --nocapture 2>&1 | grep -v ld64 | tail -5" },
  // The same gate for levels task groups.
  "levels-bench": { cmd: "cargo test --release -p bevy-ts-cli --test levels_bench -- --ignored --nocapture 2>&1 | tee target/levels-bench.log | grep -E '^\\||test result|panicked'" },
  // The levels bench A/B: target/levels-bench-before (a saved binary) against this tree's, alternated 3 times.
  "levels-bench-ab": { cmd: "bash -c 'set -o pipefail; cargo test --release -p bevy-ts-cli --test levels_bench --no-run 2>&1 | grep -E \"^error|Executable\"; cp $(ls -t target/release/deps/levels_bench-* | grep -v [.]d$ | head -1) target/levels-bench-after; cd crates/bevy-ts-cli; for i in 1 2 3; do for arm in before after; do uptime; ../../target/levels-bench-$arm --ignored --nocapture 2>&1 | grep -E \"^\\| [0-9]|panicked\" | sed \"s/^/$arm /\"; done; done 2>&1 | tee -a ../../target/levels-bench-ab.log; uptime'" },
  // The stamp's written lookup, a find per instance against one walk, 3 times (in-process A/B).
  "written-lookup-bench": { cmd: "bash -c 'set -o pipefail; for i in 1 2 3; do uptime; cargo test --release -p bevy-ts-levels --lib written_lookup_at_scale -- --ignored --nocapture 2>&1 | grep -E \"^\\| [0-9]|panicked|^error\"; done 2>&1 | tee target/written-lookup-bench.log'" },
  "levels-walks": { cmd: "cargo test --release -p bevy-ts-cli --test levels_walk --test arena_walk -- --nocapture --test-threads 1 2>&1 | grep -vE 'ld64|INFO|WARN|^\\s+\\||^\\s+=' | tee target/levels-walks.log | tail -60" },
  // Change missing-scene-row: the crates it touches, and the last round before it lands.
  "missing-scene-row-tests": { cmd: "bash -c 'set -o pipefail; mkdir -p target; (cargo test -p bevy-ts-levels && cargo test -p bevy-ts-cli --test levels_protocol --test levels_scene --test protocol) 2>&1 | grep -v ld64 | tee target/missing-scene-row-tests.log | grep -E \"^test result|FAILED|panicked|^error|left|right\"'" },
  "missing-scene-row-gate": { cmd: "bash -c 'set -o pipefail; mkdir -p target; (cargo build --workspace --all-targets && cargo test --workspace --exclude bevy-ts-viewport && cargo test -p bevy-ts-viewport -- --test-threads=1 && cargo clippy --workspace --all-targets -- -D warnings && scripts/gen-protocol.sh --check && (cd packages/bevy-ts && pnpm check)) 2>&1 | grep -v ld64 | tee target/missing-scene-row-gate.log | grep -E \"^test result|FAILED|panicked|^error|stale\"'" },
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
  // Change scene-material: the crates it touches, then the last round before it lands.
  "scene-material-tests": { cmd: "bash -c 'set -o pipefail; mkdir -p target; (cargo test -p bevy-ts-material && cargo test -p bevy-ts-tsx && cargo test -p bevy-ts-cli --test scene_material --test materials --test material_errors --test tsx -- --test-threads 1) 2>&1 | grep -v ld64 | tee target/scene-material-tests.log | grep -E \"^test |^test result|FAILED|panicked|^error|left|right|never\"'" },
  "scene-material-cli": { cmd: "bash -c 'set -o pipefail; cargo test -p bevy-ts-cli --test scene_material -- --nocapture 2>&1 | grep -v ld64 | tee target/scene-material-cli.log | grep -E \"^test |^test result|panicked|^error|left|right|never|Script\"'" },
  // The gate's lint, plus a rerun of the timing tests that flaked under load.
  "scene-material-recheck": { cmd: "bash -c 'mkdir -p target; (cargo clippy --workspace --all-targets -- -D warnings; for i in 1 2; do uptime; cargo test -p bevy-ts-cli --no-fail-fast --test typescript --test play; done) 2>&1 | grep -v ld64 | tee target/scene-material-recheck.log | grep -E \"load av|^test result|FAILED|panicked|^error|-->\"'" },
  // The rebased tip: clippy over every target, then the tests the change touches.
  "scene-material-rebased": { cmd: "bash -c 'set -o pipefail; mkdir -p target; (cargo clippy --workspace --all-targets -- -D warnings && cargo test -p bevy-ts-material && cargo test -p bevy-ts-tsx && cargo test -p bevy-ts-cli --test scene_material --test materials --test material_errors --test tsx -- --test-threads 1) 2>&1 | grep -v ld64 | tee target/scene-material-rebased.log | grep -E \"^test result|FAILED|panicked|^error|-->\"'" },
  "scene-material-gate": { cmd: "bash -c 'set -o pipefail; mkdir -p target; (cargo build --workspace --all-targets && cargo test --workspace --no-fail-fast --exclude bevy-ts-viewport; cargo test -p bevy-ts-viewport -- --test-threads=1; cargo clippy --workspace --all-targets -- -D warnings; scripts/gen-protocol.sh --check; (cd packages/bevy-ts && pnpm check)) 2>&1 | grep -v ld64 | tee target/scene-material-gate.log | grep -E \"^test result|FAILED|panicked|^error|stale|^test .* FAILED|^    [a-z_]+$\"'" },
  // Change scene-animation: the animation crate and the crates it touches. Every cargo run here
  // holds one of the machine's two heavy slots (scripts/gate-slot in abyme), per lead@bevy-ts 2026-10-23.
  "animation-test": { cmd: "$HOME/dev/abyme/scripts/gate-slot run bevy-ts:script-gltf -- bash -c 'set -o pipefail; mkdir -p target; export RUSTC_WRAPPER=; (cargo test -p bevy-ts-animation -- --test-threads 1 && cargo test -p bevy-ts-tsx && cargo build -p bevy-ts-cli) 2>&1 | grep -v ld64 | tee target/animation-test.log | grep -E \"^test |test result|panicked|^error|^warning|left|right\" | tail -60'" },
  "animation-cli": { cmd: "$HOME/dev/abyme/scripts/gate-slot run bevy-ts:script-gltf -- bash -c 'set -o pipefail; mkdir -p target; export RUSTC_WRAPPER=; cargo test -p bevy-ts-cli --test animation --test viewport_preview --test tsx --test tsx_reactive --test levels_scene -- --test-threads 1 2>&1 | grep -v ld64 | tee target/animation-cli.log | grep -E \"^test |test result|panicked|^error|left|right\" | tail -60'" },
  // docs/animation.md's table: arms A, B and C at N = 50, 100 and 200, 3 runs each.
  "animation-bench": { cmd: "$HOME/dev/abyme/scripts/gate-slot run bevy-ts:script-gltf -- bash -c 'set -o pipefail; mkdir -p target; export RUSTC_WRAPPER=; scripts/animation-bench.sh 3 2>&1 | tee target/animation-bench.log'" },
  // The same at N = 50 and 100, 8 runs each, for arms within noise of each other.
  "animation-bench-8": { cmd: "$HOME/dev/abyme/scripts/gate-slot run bevy-ts:script-gltf -- bash -c 'set -o pipefail; mkdir -p target; export RUSTC_WRAPPER=; scripts/animation-bench.sh 8 50 100 2>&1 | grep -v ld64 | tee target/animation-bench-8.log'" },
  // Diagnostic arm D beside A and B at N = 100: where B's extra frame time goes.
  "animation-bench-d": { cmd: "$HOME/dev/abyme/scripts/gate-slot run bevy-ts:script-gltf -- bash -c 'set -o pipefail; mkdir -p target; export RUSTC_WRAPPER=; ARMS=\"A D B\" scripts/animation-bench.sh 4 100 2>&1 | grep -v ld64 | tee target/animation-bench-d.log'" },
  // Change scene-animation: the last round before it lands, as placeholder-errors', plus the npm suite and the types check.
  // Regenerates bevy-ts/bevy after a rebase (one debug build), in a heavy slot.
  "scene-animation-types": { cmd: "$HOME/dev/abyme/scripts/gate-slot run bevy-ts:script-gltf -- bash -c 'set -o pipefail; mkdir -p target; export RUSTC_WRAPPER=; scripts/gen-bevy-types.sh 2>&1 | tee target/scene-animation-types.log'" },
  "scene-animation-gate": { cmd: "$HOME/dev/abyme/scripts/gate-slot run bevy-ts:script-gltf -- bash -c 'set -o pipefail; mkdir -p target; export RUSTC_WRAPPER=; (cargo build --workspace --all-targets && cargo test --workspace --exclude bevy-ts-viewport && cargo test -p bevy-ts-viewport -- --test-threads=1 && cargo clippy --workspace --all-targets -- -D warnings && scripts/gen-protocol.sh --check && scripts/gen-bevy-types.sh --check && openspec validate scene-animation --strict && (cd packages/bevy-ts && pnpm install --frozen-lockfile && pnpm check && BEVY_TS_BINARY=$PWD/../../target/debug/bevy-ts pnpm test)) 2>&1 | grep -v ld64 | tee target/scene-animation-gate.log | grep -E \"^test result|FAILED|panicked|^error|stale|failed|✗|×\"'" },
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
  // Watches this branch's newest release workflow run (a dry run) to its end.
  "release-dry-run-watch": { cmd: "bash -c 'gh run watch \"$(gh run list --workflow release.yml -b \"$(git branch --show-current)\" -L1 --json databaseId -q .[0].databaseId)\" --exit-status --interval 60 >/dev/null; s=$?; gh run view \"$(gh run list --workflow release.yml -b \"$(git branch --show-current)\" -L1 --json databaseId -q .[0].databaseId)\" | tail -30; exit $s'" },
  // Change play-stop: Stop on a real game (V8, TSX, a level).
  "play-stop-cli": { cmd: "bash -c 'set -o pipefail; cargo test -p bevy-ts-cli --test play_stop -- --nocapture --test-threads 1 2>&1 | grep -v ld64 | tee target/play-stop-cli.log | grep -E \"^test |^test result|panicked|^error|left|right|-->\"'" },
  "play-stop-rapier": { cmd: "bash -c 'set -o pipefail; cargo test -p bevy-ts-cli --test play_stop rapier -- --ignored --nocapture 2>&1 | grep -v ld64 | tee target/play-stop-rapier.log | grep -E \"^test |^test result|panicked|^error|left|right|-->|assert\"'" },
  // play-stop task 6.1: what a Stop costs at 1k and 10k level instances, in release.
  "stop-bench": { cmd: "bash -c 'set -o pipefail; uptime; cargo test --release -p bevy-ts-cli --test stop_bench -- --ignored --nocapture 2>&1 | grep -v ld64 | tee -a target/stop-bench.log | grep -E \"^[0-9]+:|^\\||test result|panicked|^error|-->\"; uptime'" },
  // The levels bench then the Stop bench, back to back, so both see one machine load.
  "stop-bench-ab": { cmd: "bash -c 'set -o pipefail; for i in 1 2 3; do uptime; cargo test --release -p bevy-ts-cli --test levels_bench -- --ignored --nocapture 2>&1 | grep -E \"^\\| [0-9]\"; cargo test --release -p bevy-ts-cli --test stop_bench -- --ignored --nocapture 2>&1 | grep -E \"^[0-9]+:|^\\| [0-9]\"; done 2>&1 | tee -a target/stop-bench-ab.log; uptime'" },
  // Change play-stop: the last round before it lands.
  // play-grey-viewport: the workspace tests, the viewport's one at a time (build and clippy ran on their own).
  "play-grey-tests": { cmd: "bash -c 'set -o pipefail; mkdir -p target; (cargo test --workspace --exclude bevy-ts-viewport && cargo test -p bevy-ts-viewport -- --test-threads=1) 2>&1 | grep -v ld64 | tee target/play-grey-tests.log | grep -E \"^test result|FAILED|panicked|^error\"'" },
  "play-stop-gate": { cmd: "bash -c 'set -o pipefail; mkdir -p target; (cargo build --workspace --all-targets && cargo test --workspace --exclude bevy-ts-viewport && cargo test -p bevy-ts-viewport -- --test-threads=1 && cargo clippy --workspace --all-targets -- -D warnings && scripts/gen-protocol.sh --check && (cd packages/bevy-ts && pnpm check)) 2>&1 | grep -v ld64 | tee target/play-stop-gate.log | grep -E \"^test result|FAILED|panicked|^error|stale\"'" },
  // Change game-camera-edit: the viewport crate built with its tests, errors and warnings only.
  "game-camera-check": { cmd: "bash -c 'mkdir -p target; cargo build -p bevy-ts-viewport --all-targets 2>&1 | grep -v ld64 | tee target/game-camera-check.log | grep -E \"^(error|warning)\" -A8 | head -120; echo done'" },
  // Its tests in tests/edit.rs, one at a time, filtered by $T.
  "game-camera-tests": { cmd: "bash -c 'cargo test -p bevy-ts-viewport --test edit -- --nocapture --test-threads=1 $T 2>&1 | grep -v ld64 | tee target/game-camera-tests.log | grep -E \"^test |test result|panicked|left|right|assert|^thread|^error\" | tail -60'" },
  // Change game-camera-edit: the last round before it lands.
  "game-camera-gate": { cmd: "bash -c 'set -o pipefail; mkdir -p target; (cargo build --workspace --all-targets && cargo test --workspace --exclude bevy-ts-viewport && cargo test -p bevy-ts-viewport -- --test-threads=1 && cargo clippy --workspace --all-targets -- -D warnings && scripts/gen-protocol.sh --check && (cd packages/bevy-ts && pnpm check)) 2>&1 | grep -v ld64 | tee target/game-camera-gate.log | grep -E \"^test result|FAILED|panicked|^error|stale\"'" },
  // Fix stop-tree-remount: tsx_tree+watch follows a restart (a Stop's too); the crates it touches.
  "stop-tree-remount-tests": { cmd: "bash -c 'set -o pipefail; mkdir -p target; (cargo test -p bevy-ts-tsx && cargo test -p bevy-ts-cli --test play_stop --test tsx_protocol --test levels_protocol --test reload -- --test-threads 1) 2>&1 | grep -v ld64 | tee target/stop-tree-remount-tests.log | grep -E \"^test result|FAILED|panicked|^error|left|right\"'" },
  "stop-tree-remount-gate": { cmd: "bash -c 'set -o pipefail; mkdir -p target; (cargo build --workspace --all-targets && cargo test --workspace --exclude bevy-ts-viewport && cargo test -p bevy-ts-viewport -- --test-threads=1 && cargo clippy --workspace --all-targets -- -D warnings && scripts/gen-protocol.sh --check && (cd packages/bevy-ts && pnpm check)) 2>&1 | grep -v ld64 | tee target/stop-tree-remount-gate.log | grep -E \"^test result|FAILED|panicked|^error|stale\"'" },
  // Change bevy-ui-coverage: the tsx unit tests and every cli `ui_*` test (gallery, style, text...).
  // The last round before a bevy-ui-coverage stage lands, as placeholder-errors' gate (no sccache: its server drops under load).
  "ui-gate": { cmd: "bash -c 'set -o pipefail; mkdir -p target; export RUSTC_WRAPPER=; (cargo build --workspace --all-targets && cargo test --workspace --no-fail-fast --exclude bevy-ts-viewport && cargo test -p bevy-ts-viewport -- --test-threads=1 && cargo clippy --workspace --all-targets -- -D warnings && scripts/gen-protocol.sh --check && scripts/gen-bevy-types.sh --check && (cd packages/bevy-ts && pnpm check)) 2>&1 | grep -v ld64 | tee target/ui-gate.log | grep -E \"^test result|FAILED|panicked|^error|stale\"'" },
  // The gate's steps after the tests, for a lint fix that changes no behaviour.
  "ui-lint": { cmd: "bash -c 'set -o pipefail; export RUSTC_WRAPPER=; (cargo clippy --workspace --all-targets -- -D warnings && scripts/gen-protocol.sh --check && scripts/gen-bevy-types.sh --check && (cd packages/bevy-ts && pnpm check)) 2>&1 | grep -v ld64 | tee target/ui-lint.log | tail -30; echo exit $?'" },
  "ui-gen": { cmd: "bash -c 'export RUSTC_WRAPPER=; scripts/gen-bevy-types.sh && git diff --stat -- packages/bevy-ts/bevy.d.ts crates/bevy-ts-v8/js/bevy.js'" },
  // Walk the gallery: build, play it headless, shots at frames 60 and 75 (target/ui-walk.png, .2.png).
  "ui-walk": { cmd: "bash -c 'export RUSTC_WRAPPER=; cargo build -p bevy-ts-cli 2>&1 | grep -v ld64 | tail -3; rm -f target/ui-walk.png*; cd examples/ui-gallery && BEVY_TS_SCREENSHOT=$OLDPWD/target/ui-walk.png timeout 120 $OLDPWD/target/debug/bevy-ts dev src/gallery.tsx --headless > $OLDPWD/target/ui-walk.log 2>&1; ls -la $OLDPWD/target/ui-walk.png*; tail -8 $OLDPWD/target/ui-walk.log'" },
  "ui-tests": { cmd: "bash -c 'set -o pipefail; mkdir -p target; export RUSTC_WRAPPER=; (cargo test -p bevy-ts-tsx --lib && cargo test -p bevy-ts-cli --test \"ui_*\" --test tsx_reactive --test levels_scene --test tsx) 2>&1 | grep -v ld64 | tee target/ui-tests.log | grep -E \"^test |^test result|FAILED|panicked|^error|left|right\"'" },
  // Fix show-bound-text: bound children of <Show>/<For>; the crates it touches and the cli's tsx tests.
  "show-bound-text-tests": { cmd: "bash -c 'set -o pipefail; mkdir -p target; (cargo test -p bevy-ts-tsx -p bevy-ts-v8 && cargo test -p bevy-ts-cli --no-fail-fast --test tsx_reactive --test tsx --test tsx_sites --test tsx_expand --test tsx_overlay --test tsx_protocol --test levels_scene) 2>&1 | grep -v ld64 | tee target/show-bound-text-tests.log | grep -E \"^test result|FAILED|panicked|^error|left|right\"'" },
  "show-bound-text-gate": { cmd: "bash -c 'set -o pipefail; mkdir -p target; (cargo build --workspace --all-targets && cargo test --workspace --exclude bevy-ts-viewport && cargo test -p bevy-ts-viewport -- --test-threads=1 && cargo clippy --workspace --all-targets -- -D warnings && deno test crates/bevy-ts-v8/js/test && (cd packages/bevy-ts && pnpm check)) 2>&1 | grep -v ld64 | tee target/show-bound-text-gate.log | grep -E \"^test result|FAILED|panicked|^error|stale|failed\"'" },
  // Fix headless-ui-camera: UI in the headless image, on the GPU.
  "headless-ui-test": { cmd: "bash -c 'cargo test -p bevy-ts-cli --lib --test headless_ui --test material_errors -- --nocapture 2>&1 | grep -v ld64 | tee target/headless-ui-test.log | grep -E \"^test |test result|panicked|left|right|assert|^thread|^error\" -A3 | tail -60'" },
  // Fix headless-ui-camera: the last round before it lands.
  "headless-ui-gate": { cmd: "bash -c 'set -o pipefail; mkdir -p target; (cargo build --workspace --all-targets && cargo test --workspace --exclude bevy-ts-viewport && cargo test -p bevy-ts-viewport -- --test-threads=1 && cargo clippy --workspace --all-targets -- -D warnings) 2>&1 | grep -v ld64 | tee target/headless-ui-gate.log | grep -E \"^test result|FAILED|panicked|^error\"; echo gate exit $?'" },
  "show-bound-text-gate": { cmd: "bash -c 'set -o pipefail; mkdir -p target; (cargo build --workspace --all-targets && cargo test --workspace --exclude bevy-ts-viewport && cargo test -p bevy-ts-viewport -- --test-threads=1 && cargo clippy --workspace --all-targets -- -D warnings && deno test crates/bevy-ts-v8/js/test && (cd packages/bevy-ts && pnpm check)) 2>&1 | grep -v ld64 | tee target/show-bound-text-gate.log | grep -E \"^test result|FAILED|panicked|^error|stale|failed\"'" },
  // Change script-networking: the crates it touches, while it is built.
  "net-tests": { cmd: "bash -c 'set -o pipefail; mkdir -p target; (cargo test -p bevy-ts --lib --test net && cargo test -p bevy-ts-v8 --test net --test web && cargo test -p bevy-ts-cli --lib play::) 2>&1 | grep -v ld64 | tee target/net-tests.log | grep -E \"^test |^test result|FAILED|panicked|^error|left|right\"'" },
  "net-bench": { cmd: "cargo test --release -p bevy-ts-v8 --test net burst_bench -- --ignored --nocapture 2>&1 | grep -v ld64 | tee target/net-bench.log | tail -20" },
  // Change script-networking: the last round before it lands.
  "script-networking-gate": { cmd: "bash -c 'set -o pipefail; mkdir -p target; (cargo build --workspace --all-targets && cargo test --workspace --exclude bevy-ts-viewport && cargo test -p bevy-ts-viewport -- --test-threads=1 && cargo clippy --workspace --all-targets -- -D warnings && scripts/gen-protocol.sh --check && (cd packages/bevy-ts && pnpm check) && openspec validate script-networking --type spec --strict) 2>&1 | grep -v ld64 | tee target/script-networking-gate.log | grep -E \"^test result|FAILED|panicked|^error|stale|valid\"; echo gate exit ${PIPESTATUS[0]}'" },
  // The gate's steps after the tests, for a lint fix that changes no behaviour.
  "script-networking-lint": { cmd: "bash -c 'set -o pipefail; (cargo clippy --workspace --all-targets -- -D warnings && scripts/gen-protocol.sh --check && (cd packages/bevy-ts && pnpm check) && openspec validate script-networking --type spec --strict) 2>&1 | grep -v ld64 | tee target/script-networking-lint.log | tail -30'" },
  // Change script-time: its time and levels tests, and the last round before stage 1 lands.
  "script-time-time": { cmd: "bash -c 'set -o pipefail; mkdir -p target; cargo test -p bevy-ts-cli --no-fail-fast --test time --test levels_protocol 2>&1 | grep -v ld64 | tee target/script-time-time.log | grep -E \"^test |^test result|panicked|^error|left|right\"'" },
  "script-time-gate": { cmd: "bash -c 'set -o pipefail; mkdir -p target; (cargo build --workspace --all-targets && cargo test --workspace --exclude bevy-ts-viewport && cargo test -p bevy-ts-viewport -- --test-threads=1 && cargo clippy --workspace --all-targets -- -D warnings && scripts/gen-protocol.sh --check && (cd packages/bevy-ts && pnpm check)) 2>&1 | grep -v ld64 | tee target/script-time-gate.log | grep -E \"^test result|FAILED|panicked|^error|stale\"; echo gate exit $?'" },
  // Change world-to-screen: the crates it touches, and the last round before it lands.
  "world-to-screen-tests": { cmd: "$HOME/dev/abyme/scripts/gate-slot run bevy-ts:world-to-screen -- bash -c 'set -o pipefail; mkdir -p target; (cargo test -p bevy-ts-cli --lib types && cargo test -p bevy-ts-cli --test cameras --test exported) 2>&1 | grep -v ld64 | tee target/world-to-screen-tests.log | grep -E \"^test |^test result|FAILED|panicked|^error|left|right|-->\"'" },
  "world-to-screen-gate": { cmd: "$HOME/dev/abyme/scripts/gate-slot run bevy-ts:world-to-screen -- bash -c 'set -o pipefail; mkdir -p target; (cargo build --workspace --all-targets && cargo test --workspace --exclude bevy-ts-viewport && cargo test -p bevy-ts-viewport -- --test-threads=1 && cargo clippy --workspace --all-targets -- -D warnings && scripts/gen-protocol.sh --check && (cd packages/bevy-ts && pnpm check)) 2>&1 | grep -v ld64 | tee target/world-to-screen-gate.log | grep -E \"^test result|FAILED|panicked|^error|stale\"'" },
} satisfies ProcessFile;

const GATE_SLOT = "$HOME/dev/abyme/scripts/gate-slot";
const runsCargo = (cmd: string) => /\bcargo\b|scripts\/[\w-]+\.sh/.test(cmd);
const quoted = (cmd: string) => `'${cmd.replaceAll("'", "'\\''")}'`;

/** Each entry that runs cargo, and does not take its slot itself, as `gate-slot run` around it. */
function gated(file: ProcessFile): ProcessFile {
  return Object.fromEntries(
    Object.entries(file).map(([name, p]) =>
      runsCargo(p.cmd) && !p.cmd.includes(GATE_SLOT)
        ? [name, { ...p, cmd: `${GATE_SLOT} run bevy-ts:${name} -- bash -c ${quoted(p.cmd)}` }]
        : [name, p],
    ),
  );
}

export default gated(processes);
