## 0. Gate on the spike

- [ ] 0.1 Read `android-native/spike`'s report: V8 from source on the phone, deno_core 0.412's hello world (deno_core#738), Bevy 0.20 drawing into a window from a raw handle. Update design.md decisions 3 and 7, the android-engine spec's JS requirement and these tasks if any fails
- [ ] 0.2 Get the operator's go on the ask-first items in proposal.md's Impact before group 2

## 1. Move play and input into `bevy-ts-embed` (no behaviour change)

- [ ] 1.1 Create `crates/bevy-ts-embed` (rlib) in the workspace; `docs/build-cache.md`'s test-binary rules apply (one `tests/it` binary)
- [ ] 1.2 Move `read_game`, `check_engine`, `read_scripts` (as `GameFolder`, a `ModuleSource`), `GameFile` and the play app from `crates/bevy-ts-cli/src/play.rs` to `bevy-ts-embed::play` as `play_app(PlayConfig { folder, env, display })`; keep `beside_exe()` and argv in the CLI
- [ ] 1.3 Move the shipped plugin set out of `cli.rs`'s `base_app` (tsx, material, animation, script input, levels, cameras) so the CLI's play and the embedding build the same app; the dev-only parts stay in the CLI
- [ ] 1.4 `NetPolicy` takes the env lookup from `PlayConfig.env`; the CLI passes the process environment
- [ ] 1.5 Move `ViewportInput` parse and apply from `crates/bevy-ts-viewport/src/input.rs` to `bevy-ts-embed::input`, keyed on a host-window marker; the viewport calls it
- [ ] 1.6 The existing shipped-game, viewport-input and networking tests pass unchanged; move tests whose owner moved (rule 1: one layer)

## 2. `wss:` and the header rule

- [ ] 2.1 Add `futures-rustls` (ring) with `webpki-roots` to the async-io transport, `async-tungstenite` kept on `handshake`, in `crates/bevy-ts/src/net/`; `wss:` and `https:` open over TLS with host-name verification
- [ ] 2.2 Remove `Refusal::Wss`; headers allowed to loopback or over `wss:`, with the new message
- [ ] 2.3 A Rust-only API to add a trusted root, for tests
- [ ] 2.4 Tests in `crates/bevy-ts/tests/it/`: a local TLS server on `common::free_port()` with a test root (open, headers carried), a self-signed server without it (1006, TLS error logged), `ws:` plus headers to a non-loopback host still refused
- [ ] 2.5 `docs/networking.md`: `wss:`, the header rule, the refusal table

## 3. The embedding runner, window and C ABI

- [ ] 3.1 The engine thread and custom runner: command channel, loop while resumed with a window or headless, park otherwise
- [ ] 3.2 Raw-handle window: `Window` + `PrimaryWindow` + `RawHandleWrapper` from the host's handle, no `WinitPlugin`, no `PipelinedRenderingPlugin`; replace on a new window, resize with `WindowResized`, drop and wait on null
- [ ] 3.3 Default asset source as a `FileAssetReader` at `<game>/assets`, unwatched, on every platform (Bevy's Android reader panics with no android-activity app)
- [ ] 3.4 Events out: `started` (inputs, js), `log` (source-mapped), `ime`, `exited`
- [ ] 3.5 The C ABI and `include/bevy_ts_embed.h`; one engine per process; refuse a project-engine game at create; error strings and `bevy_ts_free_string`
- [ ] 3.6 Tests in `crates/bevy-ts-embed/tests/it/`, headless through the C ABI on a built fixture game (`common::project`): create and `started`; version mismatch error and no thread left; second create refused; pause stops a frame counter and resume caps the delta; a socket on the fake transport survives a pause; input `touch` and `key` reach the game; malformed input logged; env map drives `process.env` and `net.allow`; destroy sends `exited` and a second create plays from the start
- [ ] 3.7 A macOS window test through the C ABI with a real window from the test harness (`common::render_app()`'s serialisation), proving surface replace and null-wait on a rendering app (rule 5)

## 4. The Android library

- [ ] 4.1 `crates/bevy-ts-android` (cdylib): JNI exports for `dev.bevyts.Engine`, `ANativeWindow_fromSurface` and release, listener calls on the engine thread (attached once)
- [ ] 4.2 `crates/bevy-ts-android/java/dev/bevyts/Engine.java`: `load(absolutePath)` by `System.load` with a version check, `version()`, no `loadLibrary`
- [ ] 4.3 `bevy-ts-v8` builds from source for `aarch64-linux-android` (`V8_FROM_SOURCE=1`, cached at `~/.cache/bevy-ts/v8-android`); or, if 0.1 says so, `crates/bevy-ts-quickjs` as a `ScriptBackend` and a feature selecting it
- [ ] 4.4 `scripts/android`: `cargo ndk -t arm64-v8a -P 29`, writing `dist/aarch64-linux-android/` (`.so`, `Engine.java`, header)
- [ ] 4.5 A CI job that builds the Android library (build only) and checks its exports with `llvm-readelf`
- [ ] 4.7 `packages/platforms/android-arm64` and a release job publishing `@bevy-ts/android-arm64` at every version (Linux runner, V8 from source with a cached build), not an optional dependency of `bevy-ts`; the package's first version by hand on the operator's go (`docs/release.md`); document it there
- [ ] 4.9 Adreno skinning: force skinned meshes onto uniform buffers when the adapter is Adreno (bevy#24926); confirm on a real Adreno phone with agent town's characters
- [ ] 4.8 `bevy_ts_abi()` and `bevy_ts_version()` exports, `Engine.ABI`, and a test that `Engine.load` refuses another ABI
- [ ] 4.6 A smoke run on a headless emulator (`emulator -no-window`), from `bench/android-smoke/` outside the gate: a minimal Kotlin host creates the engine on a `SurfaceView`, plays `examples/arena`, reports `started`, a tap and `exited`. loading the `.so` by path from the app's `filesDir`. Record the stripped and gzipped `.so` sizes and the first-frame time

## 5. Docs and handover

- [ ] 5.1 `docs/android.md`: the embedding API, the Java class, the lifecycle, building, what the phone plays and leaves out
- [ ] 5.2 `docs/build.md` and `docs/viewport.md`: play as a library, input's new owner
- [ ] 5.3 Hand abyme's follow-up to lead@abyme: the game download into `filesDir/games/`, `Streams.kt` glue for a game pane, page input over `abymeShell`, Gradle packaging of the `.so`, the `minSdk` 29 gate, `ABYME_URL` and `ABYME_KEY` in `env`
