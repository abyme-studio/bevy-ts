import type { ProcessFile } from "@abyme/processes";

// Every process that runs cargo, itself or through a script, runs inside one of the machine's check
// slots (docs/check-slots.md): `gated` below marks it heavy, and abyme holds a slot while it runs. A
// long-running server holds a slot for its build only, so its entry wraps that step in gate-slot itself.
// A run on a slot lives 30 min at most (`gated`); an entry that needs longer sets `limit` and says why.
//
// A branch is tested by `scoped` and nothing wider (docs/check-slots.md, "Scoped tests"). Add no
// per-team gate here: pass `env` to `scoped` instead. A bench or a tool that is not a gate may have
// an entry; it goes when its team ends, unless a doc names it.
// A bench that builds the workspace cold, per arm, over several arms: 2 h on a quiet machine.
const LONG_BENCH = 2 * 3600;

const processes = {
  // The tests of the crates the branch changed, then their clippy (scripts/scoped-test.sh). Its env,
  // for one run (`abyme call processes start name=scoped env:='{"TESTS":"relationships::"}'`):
  // CRATES over the crates the diff names, TESTS to run only those filters of their tests/it, LINT=0.
  // `scripts/scoped-test.sh --plan` prints what it would run, and takes no slot.
  scoped: { cmd: "set -o pipefail; mkdir -p target && NO_COLOR=1 scripts/scoped-test.sh 2>&1 | grep --line-buffered -v ld64 | tee target/scoped.log | grep -E -A5 \"^scoped-test|^error|^warning: [a-z]|-->|test result|FAILED|panicked|error TS\" | tail -150" },
  "bench-bindings-rust": {
    cmd: "cd bench/bindings/rust && cargo bench 2>&1 | tee ../results/rust-criterion.txt",
  },
  "bench-bindings-js": {
    cmd: "deno run --v8-flags=--expose-gc bench/bindings/fire-paths.js | tee bench/bindings/results/v8-jit.md",
  },
  "ports-proof": {
    cmd: "cd bench/ports && cargo test -- --nocapture 2>&1 | tee results/cargo-test.txt",
  },
  // docs/research/script-threading.md: isolate startup, memory, thread hop, column split.
  "bench-threading": {
    cmd: "cd bench/threading/rust && mkdir -p ../results && cargo run --release 2>&1 | grep -v '^ *\\(Compiling\\|Downloaded\\|Downloading\\)' | tee ../results/run.txt",
  },
  "dx-rust-check": {
    cmd: "cd bench/dx/rust && cargo check 2>&1 | tee ../results/rust-check.txt",
  },
  build: { cmd: "cargo build --workspace --all-targets" },
  // Change session-bytes, task 4.3: bench/session-bytes on this tree's release engine, built in a check slot.
  "session-bytes-bench": { cmd: "bash -c 'set -o pipefail; mkdir -p target; ~/dev/abyme/scripts/gate-slot run bevy-ts:session-bytes -- cargo build --release -p bevy-ts-cli 2>&1 | grep -v ld64 | tail -5 && cp target/release/bevy-ts target/session-bytes-bench && node bench/session-bytes/bench.mjs ${SESSION_BYTES_LABEL:-after} target/session-bytes-bench ${SESSION_BYTES_RUNS:+--runs $SESSION_BYTES_RUNS} 2>&1 | tee -a target/session-bytes-bench.log && node bench/session-bytes/summary.mjs ${SESSION_BYTES_LABEL:-after} | tee -a target/session-bytes-bench.log'" },
  // Change script-time stage 2: protocol regenerated, then the clock writes' crates and tests.
  // script-time red-main fix: regenerate bevy-ts/bevy and test/engine, check the protocol types, then pnpm check.
  "script-time-gen": { cmd: "bash -c 'set -o pipefail; mkdir -p target; (scripts/gen-bevy-types.sh && scripts/gen-protocol.sh --check && git status --short && (cd packages/bevy-ts && pnpm check)) 2>&1 | grep -v ld64 | tee target/script-time-gen.log | grep -E \"^error|error TS|stale|^ ?M |^\\?\\?\"'" },
  "script-time-stage2": { cmd: "bash -c 'set -o pipefail; mkdir -p target; (scripts/gen-protocol.sh && cargo test -p bevy-ts --lib host && cargo test -p bevy-ts-native --doc && cargo test -p bevy-ts-cli --lib -- time:: types:: && cargo test -p bevy-ts-cli --test it -- time_writes:: time:: exported:: native_writes:: protocol::) 2>&1 | grep -v ld64 | tee target/script-time-stage2.log | grep -E \"^error|^warning: [^\\`]|-->|^test |test result|panicked\"'" },
  // Change script-time stage 3: the time conditions' crates, their unit and integration tests.
  "script-time-test": { cmd: "bash -c 'set -o pipefail; mkdir -p target; (cargo test -p bevy-ts --lib time_conditions && cargo test -p bevy-ts-cli --test it -- time::) 2>&1 | grep --line-buffered -v ld64 | tee target/script-time-test.log | grep -E \"^error|^warning|-->|^test |test result|panicked\"'" },
  // The npm package's vitest suite (examples, `new`), against the debug engine for the TS transform.
  "npm-test": { cmd: "bash -c 'set -o pipefail; cargo build -p bevy-ts-cli && cd packages/bevy-ts && pnpm install --frozen-lockfile && BEVY_TS_BINARY=$PWD/../../target/debug/bevy-ts pnpm test 2>&1 | tee ../../target/npm-test.log | tail -40'" },
  "diagnostics-bench": { cmd: "set -o pipefail; cargo build --release -p bevy-ts-cli && cp target/release/bevy-ts target/diagnostics-bench && node bench/diagnostics/bench.mjs after target/diagnostics-bench 2>&1 | tee -a target/diagnostics-bench.log" },
  // Change stable-bin-order-key, task 5.1: both arms, in a slot run.sh takes itself; $RUNS per arm.
  "stable-bin-order-bench": { cmd: "bash -c 'set -o pipefail; mkdir -p target; bench/stable-bin-order/run.sh ${RUNS:-3} 2>&1 | grep -v ld64 | tee -a target/stable-bin-order-bench.txt | grep -E \"^arm=|error|panicked\"'" },
  "arena-walk": { cmd: "cargo static test --release -p bevy-ts-cli --test it arena_walk:: -- --nocapture 2>&1 | grep --line-buffered -v ld64 | tee target/arena-walk.log | grep -E '^[0-9] |test |panicked'" },
  "check-track-location": { cmd: "CARGO_TARGET_DIR=target/track-location cargo check -p bevy-ts --features track_location --all-targets" },
  release: { cmd: "cargo build --release -p bevy-ts-cli" },
  // The newest release.yml run on this branch (a dispatched dry run), watched to its end.
  "release-run": { cmd: "bash -c 'id=$(gh run list --workflow release.yml --branch \"$(git branch --show-current)\" -L 1 --json databaseId --jq \".[0].databaseId\") && gh run watch \"$id\" --exit-status --interval 30 | tail -25'" },
  // Native task 4.4: what one exclusive slot costs the parallel executor.
  "bench-slot": { cmd: "cargo static run --release -p bevy-ts-cli --example slot_barrier | tee bench/change-detection/results/slot-barrier.md" },
  "bench-bob": { cmd: "cargo static run --release -p bevy-ts-cli --example bob_bench | tee bench/change-detection/results/bob-frame.md" },
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
  // The arena built and played headless, a screenshot at the 60th ready frame (its game camera, not the editor's).
  "arena-shot": {
    cmd: "bash -c 'cargo build -p bevy-ts-cli && rm -f target/arena-shot.png && CARGO_TARGET_DIR=$PWD/target ./target/debug/bevy-ts build examples/arena --out target/arena-dist > target/arena-shot.log 2>&1 && BEVY_TS_SCREENSHOT=$PWD/target/arena-shot.png timeout 60 ./target/debug/bevy-ts play target/arena-dist/game --headless >> target/arena-shot.log 2>&1; ls -la target/arena-dist target/arena-shot.png; tail -5 target/arena-shot.log'",
  },
  // docs/research/viewport-pacing.md: the arena in edit mode at 120 Hz ticks, with the per-frame trace.
  // The same, alternating the earlier pacing (BEVY_TS_VIEWPORT_PACING=0) and the current (1), so both see one machine load.
  "viewport-pacing-ab": { cmd: "bash -c 'cargo static test --release -p bevy-ts-cli --test it viewport_pacing:: --no-run 2>/dev/null; for q in 0 1 0 1 0 1; do echo pacing=$q; BEVY_TS_VIEWPORT_PACING=$q cargo static test --release -p bevy-ts-cli --test it viewport_pacing:: -- --ignored --nocapture 2>&1 | grep -E fps; uptime; done 2>&1 | tee target/viewport-pacing-ab.log'" },
  // docs/research/viewport-tools.md: the same, alternating without the orientation gizmo (BEVY_TS_VIEWPORT_ORIENT=0) and with it.
  "viewport-tools-ab": { cmd: "bash -c 'cargo build --release -p bevy-ts-cli && cargo static test --release -p bevy-ts-cli --test it viewport_pacing:: --no-run 2>/dev/null; for o in 0 1 0 1 0 1; do echo orient=$o; BEVY_TS_VIEWPORT_ORIENT=$o BEVY_TS_BINARY=$PWD/target/release/bevy-ts cargo static test --release -p bevy-ts-cli --test it viewport_pacing:: -- --ignored --nocapture 2>&1 | grep -E \"^edit (at rest|pointer)|panicked|never answered\"; uptime; done 2>&1 | tee target/viewport-tools-ab.log'" },
  "viewport-pacing": { cmd: "PACING_ENGINE_LOG=$PWD/target/viewport-pacing-engine.log BEVY_TS_VIEWPORT_TRACE=$PWD/target/viewport-pacing.trace PACING_SAMPLE=$PWD/target/viewport-pacing.sample cargo static test --release -p bevy-ts-cli --test it viewport_pacing:: -- --ignored --nocapture 2>&1 | grep --line-buffered -v ld64 | tee target/viewport-pacing.log | grep -E '^engine|fps|^stats|test |panicked|assert|^thread' | tail -20" },
  // Change asset-preview: several viewports and preview scenes.
  // Change asset-preview, task 4.2; `preview-bench-threejs` runs while a three.js viewer shows the model.
  "preview-bench": { cmd: "PREVIEW_MODEL=$HOME/dev/spikes/mesh-conform/inputs/alicia.glb scripts/preview-bench.sh 2>&1 | tee -a target/preview-bench.log" },
  "three-viewer": { cmd: "cd bench/asset-preview && npm install --silent && cp $HOME/dev/spikes/mesh-conform/inputs/alicia.glb model.glb && python3 -m http.server 8791" },
  "preview-bench-threejs": { cmd: "PREVIEW_MODEL=$HOME/dev/spikes/mesh-conform/inputs/alicia.glb scripts/preview-bench.sh threejs 2>&1 | tee -a target/preview-bench.log" },
  "thumbnail-bench": { cmd: "uptime && cargo static test --release -p bevy-ts-cli --test it viewport_thumbnail:: -- --ignored --nocapture 2>&1 | grep --line-buffered -v ld64 | tee target/thumbnail-bench.log | grep -E '^\\||test |panicked' | tail -20 && uptime" },
  "gen-protocol": { cmd: "scripts/gen-protocol.sh 2>&1 | tail -5" },
  "levels-bench": { cmd: "cargo static test --release -p bevy-ts-cli --test it levels_bench:: -- --ignored --nocapture 2>&1 | tee target/levels-bench.log | grep -E '^\\||test result|panicked'" },
  // The levels bench A/B: target/levels-bench-before (a saved binary) against this tree's, alternated 3 times, with the write and CPU split lines.
  "levels-bench-ab": { cmd: "bash -c 'set -o pipefail; exe=$(cargo static test --release -p bevy-ts-cli --test it --no-run 2>&1 | tee /dev/stderr | grep -o \"target/release/deps/it-[0-9a-f]*\" | head -1); cp $exe target/levels-bench-after; cd crates/bevy-ts-cli; for i in 1 2 3; do for arm in before after; do uptime; f=; [ $arm = after ] && f=levels_bench::; ../../target/levels-bench-$arm $f --ignored --nocapture 2>&1 | grep -E \"^[0-9]+:|^\\| [0-9]|panicked\" | sed \"s/^/$arm /\"; done; done 2>&1 | tee -a ../../target/levels-bench-ab.log; uptime'" },
  // scripts/affected-gate.sh by hand, to reproduce a red batch: what changed since LAND_BASE, with its dependents.
  "affected-gate": { cmd: "set -o pipefail; mkdir -p target && scripts/affected-gate.sh 2>&1 | tee target/affected-gate.log" },
  // docs/animation.md's table: arms A, B and C at N = 50, 100 and 200, 3 runs each.
  "animation-bench": { cmd: "$HOME/dev/abyme/scripts/gate-slot run bevy-ts:animation-bench -- bash -c 'set -o pipefail; mkdir -p target; export RUSTC_WRAPPER=; scripts/animation-bench.sh 3 2>&1 | tee target/animation-bench.log'" },
  // The same at N = 50 and 100, 8 runs each, for arms within noise of each other.
  "animation-bench-8": { cmd: "$HOME/dev/abyme/scripts/gate-slot run bevy-ts:animation-bench-8 -- bash -c 'set -o pipefail; mkdir -p target; export RUSTC_WRAPPER=; scripts/animation-bench.sh 8 50 100 2>&1 | grep --line-buffered -v ld64 | tee target/animation-bench-8.log'" },
  "glow-walk": { cmd: "cargo build -p bevy-ts-cli && node examples/cube/glow-walk.mjs target/debug/bevy-ts 2>&1 | tee target/glow-walk.log" },
  // docs/build-cache.md: cold workspace build without sccache, then with it warm, 3 per arm.
  "sccache-bench": { cmd: "bash -c 'set -o pipefail; mkdir -p bench/results && scripts/sccache-bench.sh 3 2>&1 | tee -a bench/results/sccache.txt'", limit: LONG_BENCH },
  "sccache-bench-ab": { cmd: "bash -c 'set -o pipefail; echo ab8; scripts/sccache-bench.sh 8 ab 2>&1 | tee -a bench/results/sccache.txt'", limit: LONG_BENCH },
  // play-stop task 6.1: what a Stop costs at 1k and 10k level instances, in release.
  "stop-bench": { cmd: "bash -c 'set -o pipefail; uptime; cargo static test --release -p bevy-ts-cli --test it stop_bench:: -- --ignored --nocapture 2>&1 | grep --line-buffered -v ld64 | tee -a target/stop-bench.log | grep -E \"^[0-9]+:|^\\||test result|panicked|^error|-->\"; uptime'" },
  // The levels bench then the Stop bench, back to back, so both see one machine load.
  "stop-bench-ab": { cmd: "bash -c 'set -o pipefail; for i in 1 2 3; do uptime; cargo static test --release -p bevy-ts-cli --test it levels_bench:: -- --ignored --nocapture 2>&1 | grep -E \"^\\| [0-9]\"; cargo static test --release -p bevy-ts-cli --test it stop_bench:: -- --ignored --nocapture 2>&1 | grep -E \"^[0-9]+:|^\\| [0-9]\"; done 2>&1 | tee -a target/stop-bench-ab.log; uptime'" },
  // Walk the gallery: build, play it headless, a shot at the 60th ready frame (target/ui-walk.png).
  "ui-walk": { cmd: "bash -c 'export RUSTC_WRAPPER=; cargo build -p bevy-ts-cli 2>&1 | grep --line-buffered -v ld64 | tail -3; rm -f target/ui-walk.png; cd examples/ui-gallery && BEVY_TS_SCREENSHOT=$OLDPWD/target/ui-walk.png timeout 120 $OLDPWD/target/debug/bevy-ts dev src/gallery.tsx --headless > $OLDPWD/target/ui-walk.log 2>&1; ls -la $OLDPWD/target/ui-walk.png; tail -8 $OLDPWD/target/ui-walk.log'" },
  // Change script-time: its time and levels tests, and the last round before stage 1 lands.
  "script-time-time": { cmd: "bash -c 'set -o pipefail; mkdir -p target; cargo test -p bevy-ts-cli --no-fail-fast --test it -- time:: levels_protocol::  2>&1 | grep --line-buffered -v ld64 | tee target/script-time-time.log | grep -E \"^test |^test result|panicked|^error|left|right\"'" },
  "script-time-gate": { cmd: "bash -c 'set -o pipefail; mkdir -p target; (cargo build --workspace --all-targets && cargo test --workspace && cargo clippy --workspace --all-targets -- -D warnings && scripts/gen-protocol.sh --check && (cd packages/bevy-ts && pnpm check)) 2>&1 | grep --line-buffered -v ld64 | tee target/script-time-gate.log | grep -E \"^test result|FAILED|panicked|^error|stale\"; echo gate exit $?'" },
  // `bevy-ts/bevy` regenerated, and what it changed.
  "gen-bevy-types": { cmd: "~/dev/abyme/scripts/gate-slot run bevy-ts:gen-bevy-types -- bash -c 'scripts/gen-bevy-types.sh 2>&1 | tail -20; git diff --stat -- packages/bevy-ts/bevy.d.ts crates/bevy-ts-v8/js/bevy.js'" },
  // Fix lean-test-binaries: target/ size, test binaries, build and test wall time and peak load
  // (docs/build-cache.md). The after arm is the gate on HEAD: clippy here, then the bench's cold
  // build and workspace tests on an export of HEAD.
  "target-bench-before": { cmd: "bash -c 'set -o pipefail; scripts/target-bench.sh before 1933a72 3 2>&1 | tee -a bench/results/target-bench.txt'", limit: LONG_BENCH },
  "target-bench-after": { cmd: "bash -c 'set -o pipefail; mkdir -p target; (cargo clippy --workspace --all-targets -- -D warnings 2>&1 | grep --line-buffered -v ld64 | tee target/lean-clippy.log | grep -E ^error -A8 | head -80; scripts/target-bench.sh after HEAD 3) 2>&1 | tee -a bench/results/target-bench.txt'", limit: LONG_BENCH },
  // Change text-font-coverage: the glyph test, with its first-layout timings.
  "text-fonts": { cmd: "bash -c 'set -o pipefail; mkdir -p target; ~/dev/abyme/scripts/gate-slot run bevy-ts:text-font-coverage -- cargo test -p bevy-ts-cli --test it text_fonts:: -- --nocapture --test-threads 1 2>&1 | grep --line-buffered -v ld64 | tee -a target/text-fonts.log | grep -E \"layout with|^test |test result|panicked|notdef|^error\" -A3'" },
  // docs/fonts.md: the Linux engine with and without fontconfig and the Noto fonts, in docker.
  "linux-fonts": { cmd: "scripts/linux-fonts.sh" },
  // Stage 2: the same bench with Bevy as a dylib for tests, then what links it (scripts/dylib-check.sh).
  "target-bench-dylib": { cmd: "bash -c 'set -o pipefail; (scripts/target-bench.sh dylib lean-test-binaries-stage2 3; scripts/dylib-check.sh ${TMPDIR:-/tmp}/bevy-ts-target-bench/dylib) 2>&1 | tee -a bench/results/target-bench.txt'", limit: LONG_BENCH },
  // Stage 2's review: levels_bench in release, the static stage-1 test binary against the dylib one.
  "dylib-ab": { cmd: "bash -c 'set -o pipefail; scripts/dylib-ab.sh 893f47d 1435774 8 2>&1 | tee -a bench/results/dylib-ab.txt'", limit: LONG_BENCH },
  // The workspace doctests in this checkout (a doctest that runs cannot find the Bevy dylib).
  "doctests": { cmd: "bash -c 'set -o pipefail; mkdir -p target; cargo test --workspace --doc 2>&1 | grep --line-buffered -v ld64 | tee target/doctests.log | grep -E \"^test result|FAILED|dyld|^error\"'" },
  // The shipped engine's build (release.yml's step) in a check slot, then project B's first launch with it.
  "ecl-release": { cmd: "bash -c 'set -o pipefail; mkdir -p target/ecl; ~/dev/abyme/scripts/gate-slot run bevy-ts:engine-cache-link -- bash -c \"cargo build --release -p bevy-ts-cli 2>&1 | grep -v ld64 | tail -3 && cp target/release/bevy-ts target/ecl/bevy-ts-release && ECL_ARM=release scripts/engine-cache-link-bench.sh\" 2>&1 | grep -v ld64 | tee -a target/ecl/bench.log'" },
  "ecl-bench-before": { cmd: "bash -c 'set -o pipefail; ECL_ARM=before scripts/engine-cache-link-bench.sh 2>&1 | grep --line-buffered -v ld64 | tee -a target/ecl/bench.log'" },
  "ecl-bench-after": { cmd: "bash -c 'set -o pipefail; cargo build -p bevy-ts-cli 2>&1 | grep --line-buffered -v ld64 | tail -3 && cp target/debug/bevy-ts target/ecl/bevy-ts-after && ECL_ARM=after scripts/engine-cache-link-bench.sh 2>&1 | grep --line-buffered -v ld64 | tee -a target/ecl/bench.log'" },
  // Its fox bench at 400: playing, no graph, paused, off screen, off screen culled; 3 runs per arm, interleaved, niced.
  "animation-control-bench": { cmd: "bash -c 'set -o pipefail; mkdir -p target; export RUSTC_WRAPPER=; ARMS=\"B G P H K\" nice -n 10 scripts/animation-bench.sh 3 400 2>&1 | grep -v ld64 | tee target/animation-control-bench.log'" },
  "animation-control-villagers": { cmd: "bash -c 'set -o pipefail; mkdir -p target; export RUSTC_WRAPPER=; cargo build --release -p bevy-ts-cli 2>&1 | tail -3 && node bench/animation-control/villagers.mjs target/release/bevy-ts 2>&1 | tee target/animation-control-villagers.log'" },
  // animation-control's tween arms (T, S), RUNS runs each (3).
  "animation-control-tween-bench": { cmd: "bash -c 'set -o pipefail; mkdir -p target; export RUSTC_WRAPPER=; ARMS=\"T S\" nice -n 10 scripts/animation-bench.sh ${RUNS:-3} 400 2>&1 | grep -v ld64 | tee target/animation-control-tween-bench.log'" },
  // Change script-2d: the 2D tests (sprites, Camera2d, picking), rendering headless.
  "two-d": { cmd: "bash -c 'set -o pipefail; mkdir -p target; cargo test -p bevy-ts-cli --test it two_d:: -- --nocapture 2>&1 | grep -v ld64 | tee target/two-d.log | grep -E \"^test |^test result|panicked|^error|left|right|-->\" | tail -40'" },
  // The tests script-2d's crates touch: tsx, and the cli's TSX and 2D tests.
  // The two tests the stage 1 gate failed: the gallery's intrinsics, and the PBR limits run alone three times.
  "two-d-recheck": { cmd: "bash -c 'set -o pipefail; mkdir -p target; (cargo test -p bevy-ts-cli --test it -- ui_gallery:: two_d && for i in 1 2 3; do cargo test -p bevy-ts-cli --test it -- standard_material_limits:: || exit 1; done) 2>&1 | grep -v ld64 | tee target/two-d-recheck.log | grep -E \"^test result|FAILED|panicked|^error\"'" },
  // What land's affected-gate failed for script-2d alone: the npm examples test, and viewport_thumbnail three times.
  "two-d-land-recheck": { cmd: "bash -c 'set -o pipefail; mkdir -p target; (cargo build -p bevy-ts-cli && (cd packages/bevy-ts && pnpm install --frozen-lockfile && BEVY_TS_BINARY=$PWD/../../target/debug/bevy-ts pnpm test) && for i in 1 2 3; do cargo test -p bevy-ts-cli --test it -- viewport_thumbnail:: || exit 1; done) 2>&1 | grep -v ld64 | tee target/two-d-land-recheck.log | grep -E \"^test result|FAILED|panicked|^error|Tests |failed\"'" },
  "two-d-types": { cmd: "bash -c 'set -o pipefail; export CARGO_TARGET_DIR=$HOME/.cache/bevy-ts-target/script-2d; (cargo build -q -p bevy-ts-cli && $CARGO_TARGET_DIR/debug/bevy-ts types --bevy crates/bevy-ts-v8/js/bevy.js packages/bevy-ts/bevy.d.ts && cd packages/bevy-ts && node scripts/sync-runtime.mjs && pnpm check) 2>&1 | grep -v ld64 | tee $TMPDIR/bevy-ts-two-d-types.log | { grep -E \"^error|error TS|warning:\" -A4 || true; }'" },
  "two-d-gate": { cmd: "bash -c 'set -o pipefail; export CARGO_TARGET_DIR=$HOME/.cache/bevy-ts-target/script-2d; (cargo build --workspace --all-targets && cargo test --workspace --no-fail-fast && cargo clippy --workspace --all-targets -- -D warnings && scripts/gen-protocol.sh --check && env -u CARGO_TARGET_DIR scripts/gen-bevy-types.sh --check && (cd packages/bevy-ts && pnpm check)) 2>&1 | grep --line-buffered -v ld64 | tee $TMPDIR/bevy-ts-two-d-gate.log | grep -E \"^error|^warning|-->|FAILED|panicked|test result\"'" },
  "two-d-gate-rest": { cmd: "bash -c 'set -o pipefail; export CARGO_TARGET_DIR=$HOME/.cache/bevy-ts-target/script-2d; (cargo test -p bevy-ts-cli --test it -- --exact screenshot_ready::the_method_waits_for_a_texture_on_a_slow_disk && cargo clippy --workspace --all-targets -- -D warnings && scripts/gen-protocol.sh --check && env -u CARGO_TARGET_DIR scripts/gen-bevy-types.sh --check && (cd packages/bevy-ts && pnpm check) && echo GATE-REST-GREEN) 2>&1 | grep --line-buffered -v ld64 | tee $TMPDIR/bevy-ts-two-d-gate-rest.log | grep -E \"^error|^warning|-->|FAILED|panicked|test result|stale|error TS|GATE-REST-GREEN\"'" },
  "two-d-clippy": { cmd: "bash -c 'set -o pipefail; export CARGO_TARGET_DIR=$HOME/.cache/bevy-ts-target/script-2d; (cargo clippy --workspace --all-targets -- -D warnings && echo CLIPPY-CLEAN) 2>&1 | grep -v ld64 | tee $TMPDIR/bevy-ts-two-d-clippy.log | grep -E \"^error|^warning|CLIPPY-CLEAN\" -A4'" },
  "two-d-only": { cmd: "bash -c 'set -o pipefail; export CARGO_TARGET_DIR=$HOME/.cache/bevy-ts-target/script-2d; mkdir -p target; cargo test -p bevy-ts-cli --test it -- two_d 2>&1 | grep -v ld64 | tee $TMPDIR/bevy-ts-two-d-only.log | grep -E \"^test result|FAILED|panicked|^error|left|right\"'" },
  "two-d-sheet": { cmd: "bash -c 'set -o pipefail; export CARGO_TARGET_DIR=$HOME/.cache/bevy-ts-target/script-2d; cargo test -p bevy-ts-cli --test it -- two_d::a_ts_system_on_a_timer_steps 2>&1 | grep -v ld64 | tee $TMPDIR/bevy-ts-two-d-sheet.log | grep -E \"^test result|FAILED|panicked|^error|left|right\" -A3'" },
  // The four tests stage 4's gate failed, each alone, three times: load or real?
  "two-d-gate-fails": { cmd: "bash -c 'set -o pipefail; export CARGO_TARGET_DIR=$HOME/.cache/bevy-ts-target/script-2d; (for i in 1 2 3; do cargo test -p bevy-ts --test it -- http:: && cargo test -p bevy-ts-cli --test it -- --exact reflect_assets::a_failed_reload_adds_no_asset viewport_thumbnail::thumbnails_asked_together_do_not_see_each_other || exit 1; done; echo FAILS-GREEN) 2>&1 | grep -v ld64 | tee $TMPDIR/bevy-ts-two-d-gate-fails.log | grep -E \"^test result|FAILED|panicked|left|right|FAILS-GREEN\" -A2'" },
  // The gate's generated-file and type checks, with gen-bevy-types.sh's steps on this target's binary (no second build in ./target).
  "two-d-gen": { cmd: "bash -c 'set -o pipefail; export CARGO_TARGET_DIR=$HOME/.cache/bevy-ts-target/script-2d; B=$CARGO_TARGET_DIR/debug/bevy-ts; (scripts/gen-protocol.sh --check && cargo build -q -p bevy-ts-cli && $B types --bevy crates/bevy-ts-v8/js/bevy.js packages/bevy-ts/bevy.d.ts 2>/dev/null && $B types --module bevy-ts/input packages/bevy-ts/input.d.ts 2>/dev/null && $B types --out packages/bevy-ts/test/engine/native.d.ts 2>/dev/null && (cd packages/bevy-ts && node scripts/sync-runtime.mjs >/dev/null) && git diff --exit-code --stat -- crates/bevy-ts-v8/js/bevy.js packages/bevy-ts && (cd packages/bevy-ts && pnpm check) && echo GEN-GREEN) 2>&1 | grep -v ld64 | tee $TMPDIR/bevy-ts-two-d-gen.log | grep -E \"^error|stale|error TS|\\||GEN-GREEN\" -A2'" },
  "two-d-tilemap": { cmd: "bash -c 'set -o pipefail; export CARGO_TARGET_DIR=$HOME/.cache/bevy-ts-target/script-2d; (cargo test -p bevy-ts --lib -- reflect_asset atlas_layout && cargo test -p bevy-ts-tsx && cargo test -p bevy-ts-cli --test it -- two_d::a_tilemap two_d::a_ts_system) 2>&1 | grep -v ld64 | tee $TMPDIR/bevy-ts-two-d-tilemap.log | grep -E \"^test result|FAILED|panicked|^error|left|right\" -A3'" },
  "two-d-text": { cmd: "bash -c 'set -o pipefail; export CARGO_TARGET_DIR=$HOME/.cache/bevy-ts-target/script-2d; (cargo test -p bevy-ts-tsx && cargo test -p bevy-ts-cli --test it -- two_d ui_gallery) 2>&1 | grep -v ld64 | tee $TMPDIR/bevy-ts-two-d-text.log | grep -E \"^test result|FAILED|panicked|^error|left|right\"'" },
  "two-d-atlas": { cmd: "bash -c 'set -o pipefail; export CARGO_TARGET_DIR=$HOME/.cache/bevy-ts-target/script-2d; (cargo test -p bevy-ts --lib -- atlas_layout reflect_asset && cargo test -p bevy-ts-tsx && cargo test -p bevy-ts-cli --test it -- two_d ui_media ui_gallery) 2>&1 | grep -v ld64 | tee $TMPDIR/bevy-ts-two-d-atlas.log | grep -E \"^test result|FAILED|panicked|^error|left|right\"'" },
  "two-d-tests": { cmd: "bash -c 'set -o pipefail; mkdir -p target; (cargo test -p bevy-ts-tsx && cargo test -p bevy-ts-cli --test it -- tsx two_d) 2>&1 | grep -v ld64 | tee target/two-d-tests.log | grep -E \"^test result|FAILED|panicked|^error|left|right\"'" },
  // Change script-gizmos, tasks 3.1-3.3: the TS arms on this tree's release engine, then the Rust
  // baseline. GIZMOS_RUNS=8 tops every arm up to 8 runs.
  "bench-gizmos": { cmd: "set -o pipefail; mkdir -p target bench/gizmos/results; (cargo build --release -p bevy-ts-cli && cp target/release/bevy-ts target/gizmos-bench && node bench/gizmos/bench.mjs after target/gizmos-bench --runs ${GIZMOS_RUNS:-3} && node bench/gizmos/summary.mjs after | tee bench/gizmos/results/after.md && (cd bench/gizmos/rust && cargo bench 2>&1 | tee ../results/rust-criterion.txt | grep -E \"^error|^frame|time:\")) 2>&1 | grep -v ld64 | tee target/bench-gizmos.log | tail -40" },
  "mesh-update-bench": { cmd: "bash -c 'set -o pipefail; mkdir -p target; cargo test --release -p bevy-ts-cli --test it mesh_update_bench -- --ignored --nocapture 2>&1 | grep -v ld64 | tee target/mesh-update-bench.log | tail -20'" },
  // Change script-image-bytes: a 1024² RGBA8 image updated from TS against the same copy in Rust.
  "image-bytes-bench": { cmd: "bash -c 'set -o pipefail; bench/script-image-bytes/run.sh ${RUNS:-3} 2>&1 | grep -v ld64 | tail -20'" },
  // Change pixel-art-2d: `bevy-ts/bevy` regenerated, the touched crates' unit tests, the pixel-2D,
  // sampler, sources, 2D, build and play tests, clippy and tsc. PA_ONLY narrows the CLI's tests.
  "pixel-art-2d-test": {
    cmd: "bash -c 'set -o pipefail; mkdir -p target; (scripts/gen-bevy-types.sh && cargo test -p bevy-ts-levels --lib && cargo test -p bevy-ts-cli --lib project && cargo test -p bevy-ts-material --no-fail-fast --test it -- ${PM_ONLY:-pixel2d bitmap_text} && cargo test -p bevy-ts-tsx --lib && cargo test -p bevy-ts-cli --no-fail-fast --test it -- ${PA_ONLY:-image_sampler:: asset_sources:: two_d:: two_d_doc:: build:: play:: argv:: mesh_picking:: ui_focus:: ui_widgets::} && cargo clippy -p bevy-ts-material -p bevy-ts-tsx -p bevy-ts-levels -p bevy-ts-cli --all-targets -- -D warnings && (cd packages/bevy-ts && pnpm check)) 2>&1 | grep --line-buffered -v ld64 | tee target/pixel-art-2d-test.log | grep -E -A5 \"^test result|FAILED|panicked|^error|^warning|-->|error TS|stale\" | tail -150'",
  },
  // Change pixel-art-2d, design 8: bench/pixel-2d's arms, 3 runs each (8 with RUNS=8), in a check slot.
  "pixel-2d-bench": { cmd: "bash -c 'set -o pipefail; bench/pixel-2d/run.sh ${RUNS:-3} 2>&1 | grep -v ld64 | tee -a bench/pixel-2d/results/runs.txt | grep -E \"^arm=|load|error\"'" },
  // Change script-fullscreen-pass: the material crate's lib tests and pass tests, the CLI's pass,
  // 2D doc and material tests, bevy-ts-tsx's tests, tsc and clippy, in a check slot. SFP_ONLY
  // narrows the CLI's tests.
  "script-fullscreen-pass-test": {
    cmd: "bash -c 'set -o pipefail; mkdir -p target; ~/dev/abyme/scripts/gate-slot run bevy-ts:script-fullscreen-pass -- bash -c \"cargo test -p bevy-ts-material --lib && cargo test -p bevy-ts-material --no-fail-fast --test it -- pass:: pass_render:: && cargo test -p bevy-ts-tsx && cargo test -p bevy-ts-cli --no-fail-fast --test it -- ${SFP_ONLY:-passes:: two_d_doc:: materials::} && (cd packages/bevy-ts && pnpm check) && cargo clippy -p bevy-ts-material -p bevy-ts-tsx -p bevy-ts-cli --all-targets -- -D warnings\" 2>&1 | grep -v ld64 | tee target/script-fullscreen-pass-test.log | grep -E -A7 \"^error|^warning|-->|test result|FAILED|panicked|error TS|stale\" | tail -150'",
  },
  // Change script-fullscreen-pass, design 8: bench/fullscreen-pass's arms, 3 runs each (8 with RUNS=8), in a check slot.
  "fullscreen-pass-bench": { cmd: "bash -c 'set -o pipefail; bench/fullscreen-pass/run.sh ${RUNS:-3} 2>&1 | grep -v ld64 | tee -a bench/fullscreen-pass/results/runs.txt | grep -E \"^arm=|load|error|panicked\"'" },
  // The same at 8 runs per arm, for arms within noise of each other.
  "fullscreen-pass-bench-8": { cmd: "bash -c 'set -o pipefail; bench/fullscreen-pass/run.sh 8 2>&1 | grep -v ld64 | tee -a bench/fullscreen-pass/results/runs.txt | grep -E \"^arm=|load|error|panicked\"'" },
  // Change script-material2d, design 8: bench/material2d's arms, 3 runs each (8 with RUNS=8), in a check slot.
  // Its cold release build at high load runs past 30 min.
  "material2d-bench": { cmd: "bash -c 'set -o pipefail; bench/material2d/run.sh ${RUNS:-3} 2>&1 | grep -v ld64 | tee -a bench/material2d/results/runs.txt | grep -E \"^arm=|load|error|panicked\"'", limit: LONG_BENCH },
  // The same at 8 runs per arm, for arms within noise of each other.
  "material2d-bench-8": { cmd: "bash -c 'set -o pipefail; bench/material2d/run.sh 8 2>&1 | grep -v ld64 | tee -a bench/material2d/results/runs.txt | grep -E \"^arm=|load|error|panicked\"'", limit: LONG_BENCH },
  // Change custom-relationships: bevy-ts checked with and without reflect, in a check slot.
  "custom-relationships-check": { cmd: "bash -c 'set -o pipefail; mkdir -p target; ~/dev/abyme/scripts/gate-slot run bevy-ts:custom-relationships -- bash -c \"cargo check -p bevy-ts -p bevy-ts-remote -p bevy-ts-levels -p bevy-ts-cli --all-targets && cargo check -p bevy-ts --features reflect --all-targets\" 2>&1 | grep -v ld64 | tee target/custom-relationships-check.log | grep -E -A7 \"^error|^warning|Finished\" | tail -150'" },
  "custom-relationships-test": { cmd: "bash -c 'set -o pipefail; mkdir -p target; ~/dev/abyme/scripts/gate-slot run bevy-ts:custom-relationships -- bash -c \"scripts/gen-protocol.sh && cargo test -p bevy-ts -p bevy-ts-remote -p bevy-ts-levels --lib --tests && cargo test -p bevy-ts --features reflect --lib && cargo test -p bevy-ts-cli --lib && cargo test -p bevy-ts-cli --test it -- relationships:: ecs_queries:: heap_fields:: protocol:: saves:: reload:: tsx::\" 2>&1 | grep -v ld64 | tee target/custom-relationships-test.log | grep -E -A12 \"^error|panicked|FAILED|^test result\" | tail -150'" },
  // custom-relationships 5.4: packed target reads against an inline array, 10,000 rows x 1, 4, 16, release,
  // ${RUNS:-3} runs per arm. Arms alternate each frame, timed in-process; it prints the load before and after.
  "custom-relationships-bench": {
    cmd: "bash -c 'set -o pipefail; mkdir -p target; uptime; ~/dev/abyme/scripts/gate-slot run bevy-ts:custom-relationships -- bash -c \"uptime; BENCH_RUNS=${RUNS:-3} cargo static test --release -p bevy-ts-cli --test it -- --ignored --nocapture --exact relationships::bench_packed_target_reads; uptime\" 2>&1 | grep -v ld64 | tee target/custom-relationships-bench.log | grep -E \"^\\||load average|^error|panicked|test result\"'",
  },
  // The same, 8 runs per arm, for arms within noise of the gate.
  "custom-relationships-bench-8": {
    cmd: "bash -c 'set -o pipefail; mkdir -p target; uptime; ~/dev/abyme/scripts/gate-slot run bevy-ts:custom-relationships -- bash -c \"uptime; BENCH_RUNS=8 cargo static test --release -p bevy-ts-cli --test it -- --ignored --nocapture --exact relationships::bench_packed_target_reads; uptime\" 2>&1 | grep -v ld64 | tee target/custom-relationships-bench-8.log | grep -E \"^\\||load average|^error|panicked|test result\"'",
  },
  "custom-relationships-rerun": {
    cmd: "bash -c 'set -o pipefail; mkdir -p target; ~/dev/abyme/scripts/gate-slot run bevy-ts:custom-relationships -- bash -c \"scripts/gen-protocol.sh --check && cargo test -p bevy-ts-cli --no-fail-fast --test it -- ui_gallery:: relationships:: && cargo test -p bevy-ts-material --no-fail-fast --test it -- alpha_render::\" 2>&1 | grep -v ld64 | tee target/custom-relationships-rerun.log | grep -E \"^error|-->|test result|FAILED|panicked|stale\"'",
  },
  "custom-relationships-pack": {
    cmd: "bash -c 'set -o pipefail; mkdir -p target; ~/dev/abyme/scripts/gate-slot run bevy-ts:custom-relationships -- bash -c \"cargo test -p bevy-ts --lib packed_lists && cargo test -p bevy-ts-cli --no-fail-fast --test it -- relationships:: && cargo clippy -p bevy-ts -p bevy-ts-cli --all-targets -- -D warnings\" 2>&1 | grep -v ld64 | tee target/custom-relationships-pack.log | grep -E \"^error|^warning|-->|test result|FAILED|panicked\"'",
  },
  "doctests": { cmd: "bash -c 'set -o pipefail; mkdir -p target; cargo test --workspace --doc 2>&1 | grep -v ld64 | tee target/doctests.log | grep -E \"^test result|FAILED|dyld|^error\"'" },
  // Change android-native: the Android library (scripts/android takes its own check slot; a V8
  // build from source when its cache is cold, about 47 min at -j4).
  android: { cmd: "bash -c 'set -o pipefail; mkdir -p target; scripts/android 2>&1 | grep --line-buffered -v ld64 | tee target/android.log | tail -40'" },
  // Task 4.6: the library on a headless emulator, from bench/android-smoke (no window on screen).
  "android-smoke": { cmd: "bash -c 'set -o pipefail; mkdir -p target; bench/android-smoke/run.sh 2>&1 | tee target/android-smoke.log | tail -40'" },
  // The same for agent town's two worlds, against its stand-in abymed (bench/android-smoke/abyme-game.mjs).
  "android-agent-town": { cmd: "bash -c 'set -o pipefail; mkdir -p target; node bench/android-smoke/abyme-game.mjs town 2>&1 | tee target/android-agent-town.log | tail -40'" },
  "android-learn": { cmd: "bash -c 'set -o pipefail; mkdir -p target; node bench/android-smoke/abyme-game.mjs learn 2>&1 | tee target/android-learn.log | tail -40'" },
  // The crates android-native changed, after its rebase onto main: clippy, embed's tests, the CLI's
  // unit tests and the integration tests over what moved into embed.
  "android-native-test": { cmd: "bash -c 'set -o pipefail; mkdir -p target; ~/dev/abyme/scripts/gate-slot run bevy-ts:android-native -- bash -c \"cargo clippy -p bevy-ts-embed -p bevy-ts-cli -p bevy-ts-viewport --all-targets -- -D warnings && cargo test -p bevy-ts-embed && cargo test -p bevy-ts-viewport && cargo test -p bevy-ts-cli --lib && cargo test -p bevy-ts-cli --no-fail-fast --test it -- ${ONLY:-image_sampler:: asset_sources:: two_d:: build:: play:: argv:: time:: time_writes:: ready_hold:: session:: script_assets:: diagnostics:: types_modules:: headless_pointer::}\" 2>&1 | grep --line-buffered -v ld64 | tee target/android-native-test.log | grep -E -A5 \"^test result|FAILED|panicked|^error|^warning|-->\" | tail -150'", limit: 90 * 60 }, // clippy and three crates' tests from a cold incremental cache pass 30 min
} satisfies ProcessFile;

// Long-running servers: each wraps only its build in gate-slot, so no limit ends it.
const servers = new Set(["skeleton", "cube", "rapier", "thrusters", "three-viewer"]);
const runsCargo = (cmd: string) => /\bcargo\b|scripts\/[\w-]+\.sh/.test(cmd);
const takesSlot = (cmd: string) => /scripts\/gate-slot/.test(cmd);
const quoted = (cmd: string) => `'${cmd.replaceAll("'", "'\\''")}'`;
/** The longest a run holds a slot, in seconds, unless its entry sets its own `limit`. */
const SLOT_LIMIT = 30 * 60;

/** Each entry that runs cargo, and does not take its slot itself, marked heavy: abyme holds one of
 * the check slots while it runs. In bash, as before: the operator's shell (fish) has no pipefail.
 * Every run on a slot, but a server's, lives `SLOT_LIMIT` at most. */
function gated(file: ProcessFile): ProcessFile {
  return Object.fromEntries(
    Object.entries(file).map(([name, p]) => {
      const limit = servers.has(name) || !runsCargo(p.cmd) ? {} : { limit: p.limit ?? SLOT_LIMIT };
      if (runsCargo(p.cmd) && !takesSlot(p.cmd)) return [name, { ...p, cmd: `bash -c ${quoted(p.cmd)}`, heavy: true, ...limit }];
      return [name, { ...p, ...limit }];
    }),
  );
}

export default gated(processes);
