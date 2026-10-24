## Context

Research (`~/.local/state/abyme/documents/bevy-ts/android-target.md`) recommends, if the phone
must run the game itself, option B: bevy-ts as a native library inside abyme's Android app,
drawing into a `SurfaceView` the app places behind a hole in the page, as `Streams.kt` does for
the Mac's H.264 stream today. The page keeps every touch and already speaks SURFACE.md input.

What bevy-ts has today, and what stands in the way:

- The engine is `bevy-ts-cli`. Its play path (`crates/bevy-ts-cli/src/play.rs`) builds the app
  from a game folder, but the folder comes from `beside_exe()` (`current_exe()`) or argv, and the
  app comes from `cli.rs`'s `base_app`, which also links the compiler, remote and viewport.
- Bevy's window comes from winit, or from `bevy-ts-viewport`'s virtual window, which is macOS-only
  (IOSurface, Mach). The `ViewportInput` mapping (`bevy-ts-viewport/src/input.rs`) lives there.
- The net policy reads the process environment (`NetPolicy::from_env`), though
  `NetPolicy::from_config` already takes a lookup function.
- No TLS: `wss:` is refused, and `headers` only go to loopback.
- `bevy-ts-v8` uses rusty_v8's prebuilts, which have no Android build.

A sibling item, `android-native/spike`, is proving three things on a device: V8 from source,
deno_core 0.412 on Android (deno_core#738), and Bevy 0.20 drawing into a window it didn't create.
This design assumes all three hold and names the fallback where they don't.

## Goals / Non-Goals

**Goals:**

- A platform-neutral embedding API (a C ABI), with Android its first host and iOS possible later.
- An Android library and Java class abyme's app can load and drive.
- A game that runs on the desktop runs unchanged on the phone, networking included.

**Non-Goals:**

- abyme's side: the game download, the Kotlin glue in `Streams.kt`, the pane, Gradle packaging,
  the self-update. That is abyme's follow-up change; this design only fixes the boundary.
- A web build (option A), the stream (option 0), a separate APK (option C).
- Hot reload, the studio, the remote protocol or the inspector on the phone.
- Project-engine games (native crates) on Android.

## Decisions

### 1. The boundary is a C ABI; JNI is a thin shim over it

`bevy-ts-embed` exports:

```c
typedef struct BevyTsEngine BevyTsEngine;
typedef void (*bevy_ts_event_fn)(void *user, const char *json, size_t len);

BevyTsEngine *bevy_ts_create(const char *config_json, void *window,
                             uint32_t width, uint32_t height, float scale,
                             bevy_ts_event_fn on_event, void *user, char **error);
void bevy_ts_surface(BevyTsEngine *e, void *window, uint32_t width, uint32_t height, float scale);
void bevy_ts_pause(BevyTsEngine *e);
void bevy_ts_resume(BevyTsEngine *e);
void bevy_ts_input(BevyTsEngine *e, const char *json, size_t len);
void bevy_ts_destroy(BevyTsEngine *e);
void bevy_ts_free_string(char *s);
```

- `config_json`: `{ "game": "<abs path>", "env": { "NAME": "value" }, "headless": false }`.
  JSON keeps the ABI stable as options grow.
- `window`: the platform's native window (Android `ANativeWindow *`), or null.
- `width`, `height`: physical pixels. `scale`: pixels per point (Android's `density`), so the
  window's logical size equals the page's CSS size, and SURFACE.md points map 1:1 to logical px.
- Events out are JSON lines (see the engine-embedding spec): `started`, `log`, `ime`, `exited`.

`bevy-ts-android` is a `cdylib` that links `bevy-ts-embed` and adds JNI exports for
`dev.bevyts.Engine` (`nativeCreate`, `nativeSurface`, `nativePause`, `nativeResume`,
`nativeInput`, `nativeDestroy`). bevy-ts ships that Java class, so the JNI symbol names are
bevy-ts's own and never abyme's package.

Kotlin's side, for abyme's change:

```kotlin
// in Streams.kt's SurfaceHolder.Callback, for a game pane
override fun surfaceCreated(holder: SurfaceHolder) {
  val f = holder.surfaceFrame
  engine = engine?.also { it.surface(holder.surface, f.width(), f.height(), density) }
    ?: Engine.create(config, holder.surface, f.width(), f.height(), density) { json -> page.post(json) }
}
override fun surfaceChanged(holder: SurfaceHolder, format: Int, w: Int, h: Int) { engine?.surface(holder.surface, w, h, density) }
override fun surfaceDestroyed(holder: SurfaceHolder) { engine?.surface(null, 0, 0, density) } // waits
// Activity onPause/onResume -> engine.pause()/resume(); pane closed -> engine.close()
// abymeShell input message from the page -> engine.input(json)
```

Alternatives: JNI exports named for abyme's package (couples bevy-ts to abyme); linking bevy-ts
into `libabyme_phone.so` (the phone crate is Tauri, built alone, and would carry Bevy and V8 even
when no game runs; a separate `.so` loads only when a game opens); a Rust API only (no iOS or
other host later without a rewrite).

### 1b. The engine is a per-version download, loaded by path; games come from any build

A game folder is platform-neutral (compiled JS, assets, `game.json`), so the phone plays the
`game/` of any prebuilt-engine build: abymed serves `dist/<host target>/game/`, and there is no
Android build target. `game.json`'s `bevyTs` must match the engine exactly, as on the desktop, so
abyme keeps one engine per version in `filesDir/engines/<version>/libbevy_ts_android.so`,
downloaded from abymed, and the APK carries none.

`Engine.java` never calls `System.loadLibrary`. The host calls `Engine.load(absolutePath)`, which
`System.load`s it and compares the library's `bevy_ts_abi()` with the class's `Engine.ABI`. The
check is the ABI, not the bevy-ts version, so one `Engine.java` compiled into the APK loads any
engine of its ABI, and the game's `game.json` picks which version to fetch. `Engine.version()` reads
the loaded library's `bevy_ts_version()`. The ABI number rises only on an incompatible change to the
C ABI, its config keys or its event shapes; a new APK is needed only then.

Where engines come from: the release workflow publishes `@bevy-ts/android-arm64` at every version,
as it does the desktop engines (`docs/release.md`), not as an optional dependency of `bevy-ts`.
abymed fetches `@bevy-ts/android-arm64@<version>` from npm into its state folder on first ask and
serves it to the phone. Alternative: abyme's `scripts/android` builds it from the pinned bevy-ts;
that pins one version and puts a 1-2 h V8 build in abyme's promote.

Risk: dlopen of a library in app storage on API 29+. Android's W^X rule forbids `exec` of files
there and, as far as known, still allows mapping a library; the smoke run (task 4.6) proves it. If
it fails, the fallback is the engine in the APK: abyme stores `.so` files uncompressed, so the APK
grows by the stripped size, about 67 MB for Bevy alone plus V8.

### 2. The engine owns a thread; the host never drives frames

`bevy_ts_create` validates `game.json` on the caller's thread (fast, so errors come back
synchronously), then starts the engine thread, which builds the Bevy app and runs a custom runner
(`App::set_runner`). Host calls become commands on a channel the runner drains at each frame
boundary. The runner loops while `resumed && (window || headless)`, and otherwise parks on the
channel.

Two calls wait: `bevy_ts_surface(null)`, because Android forbids touching a `Surface` after
`surfaceDestroyed` returns, and `bevy_ts_destroy`, which joins the thread. Both have deadlines
(2 s, 5 s) and log if missed.

One engine per process: V8's platform and Bevy's task pools are process-wide.

Alternative: the host calls `bevy_ts_frame()` from Android's `Choreographer`. That ties frame
pacing to the UI thread and blocks it for a whole frame. Rejected; vsync pacing comes from the
surface's present mode instead.

### 3. Bevy gets a window from a raw handle, with no winit

The app is built with `DefaultPlugins` minus `WinitPlugin`, with `WindowPlugin { primary_window:
None }`. On a window, the runner spawns (or updates) one `Window` entity with `PrimaryWindow`,
its resolution set to the physical size and `scale_factor_override` to `scale`, and a
`RawHandleWrapper` built from the `ANativeWindow`'s `AndroidNdkWindowHandle`. bevy_render's
surface creation does the rest, Vulkan by default. On a new window the runner replaces the
`RawHandleWrapper`; on null it removes it and waits until the render world has dropped the
surface before answering. Resize writes `WindowResized`, and the first window and a scale change write `WindowCreated` and
`WindowScaleFactorChanged`: camera targets update only on those messages.

The spike confirmed this from Bevy 0.20's sources, and `jinleili/bevy-in-app` does the same:
bevy_render's window code handles a handle removed on Android suspend and a new one inserted on
resume (`bevy_render` `view/window/mod.rs`). What winit would do and we must: our own runner, the
`WindowResized` messages, and pipelined rendering off (`PipelinedRenderingPlugin` left out), so the
render world drops the surface inside the frame that removes the handle. Still to see on a device;
if a new handle doesn't recreate the surface, the runner despawns the window entity and spawns a
new one.

Alternative: winit's Android backend through `android-activity`. It wants to own an activity and
has open resume bugs (bevy#26282). Rejected.

### 4. The game folder and env come from the host

`play.rs`'s `app(engine, extra, folder, display)` already takes a folder. It, `read_game`,
`check_engine`, `read_scripts` and the shipped plugin set from `base_app` move into
`bevy-ts-embed` as `embed::play_app(PlayConfig { folder, env, display })`. `bevy-ts-cli` keeps
`beside_exe()` and argv parsing and calls `play_app` with the process environment.
`read_scripts` becomes a `ModuleSource` over a folder (`GameFolder`), used once.

`env` reaches `NetPolicy::from_config` as its lookup, in place of `std::env::var`.

Assets: Bevy on Android defaults to its `AndroidAssetReader`, which panics without an
android-activity app, and a Kotlin host has none. The embedding registers the default source as a
`FileAssetReader` rooted at `<game>/assets`, unwatched, before `AssetPlugin` builds.

abyme's shell owns getting the folder: abymed serves the built `game/` and the shell downloads it
with the device's key into `filesDir/games/<name>/`. The JS is data the engine reads, so it is not
downloaded executable code.

### 5. Input mapping moves to `bevy-ts-embed`

`bevy-ts-viewport/src/input.rs`'s parse and apply (`PendingInput`, the touch and gamepad diffing,
the winit-equivalent messages) moves to `bevy-ts-embed::input`, generic over "the host window"
entity. The viewport keeps its Mach and HTTP carriers and calls it. One owner, one mapping.

### 6. `wss:` with rustls, and headers over TLS

The async-io transport gains a TLS connector: `futures-rustls` with the `ring` provider and
`webpki-roots`, on every target. `async-tungstenite` stays on its `handshake` feature only: its own
TLS features are stale or tokio-only, so the transport wraps the TCP stream in TLS itself. No platform verifier: tailscale serve uses
Let's Encrypt certificates, which webpki roots trust, and a platform verifier on Android needs JNI
setup. Tests add a root through a Rust-only API, never from a script.

The header rule becomes: headers go to loopback, or over `wss:`, and only ever to a host in the
allowlist (`net/policy.rs`'s check order is unchanged). A key still never crosses the network in
clear text. `Refusal::Wss` goes; `Refusal::HeadersNotLoopback`'s message changes.

Headers go to any allowlisted `wss:` host, not only to an origin the host names. The allowlist is
already the game author's list of where it may connect, and a second list would split one rule in
two. The alternative, headers only to an origin the embedding host passes, protects a key from a
game whose allowlist is wider than its author meant; the operator decides (proposal, Impact).

On the phone: the host passes `ABYME_URL=https://mac.<tailnet>.ts.net:<port>` and
`ABYME_KEY=<device key>` in `env`; the game's `net.allow` holds `${ABYME_URL}`; `connect()` sends
`authorization: Bearer <key>` over `wss:`. abymed accepts a device key that way, as it does a
CLI's `ABYME_KEY` (abyme `docs/abymed.md`, Devices); the cookie is only for browsers.

### 7. JS engine: V8 from source, QuickJS-ng as the fallback

`scripts/android` runs `cargo ndk -t arm64-v8a -P 29 build -p bevy-ts-android --release` with
`V8_FROM_SOURCE=1`, `GN_ARGS=use_relative_vtables_abi=false` and a persistent V8 build cache
(`~/.cache/bevy-ts/v8-android`). API 29 is the lowest the recipe proves. The locked `v8` is
150.4.0.

deno_core#738's cause, from the code (`docs/research/android.md`): V8's GN turns on relative
vtables for arm64 Android with the custom libc++, so a vtable holds 32-bit offsets. rusty_v8's
`OneByteConst` lays out an `ExternalOneByteStringResource` vtable by hand with absolute pointers,
and deno_core makes every static string with it while it sets up `Deno.core`. V8 reads the
pointers as offsets and jumps to a misaligned address, which is #738's `BUS_ADRALN`. Turning the
flag off fixes it; the other fix is a rusty_v8 patch laying the vtable out as offsets on Android.
The run that confirms it waits for a check slot.

Building V8 for Android from a Mac needs a recipe rusty_v8 no longer keeps (its Android CI stopped
in 2024): a git checkout with submodules (the crate leaves out `build/android/`), allowing a Mac
host in `BUILDCONFIG.gn`, writing in `android_ndk_version`, the NDK symlinked at
`third_party/android_toolchain/ndk`, a stub `aarch64-linux-android24-clang++` so `build.rs` doesn't
fetch a Linux NDK, and the NDK's clang runtimes copied into Chromium's clang. `scripts/android`
applies these as a patch set on a pinned checkout; the release job builds on a Linux runner, where
most of them fall away.

If the spike shows deno_core or V8 can't run there, `crates/bevy-ts-quickjs` implements
`ScriptBackend` (`crates/bevy-ts/src/backend.rs`) on `rquickjs` 0.14, and `bevy-ts-android`
selects it by feature. The globals in `bevy-ts-v8/js/web.js` are plain JS and port as they are; the
ops become rquickjs functions. No JIT and no inspector: 1.46 MB stripped, 0.68 MB gzipped.

## Risks / Trade-offs

- [deno_core aborts at init on Android (#738)] → the spike runs deno_core's hello world on the
  phone first; QuickJS-ng is the fallback.
- [Bevy 0.20 won't recreate its surface on a new handle] → confirmed from source, not yet on a
  device; despawn and respawn the window as the fallback.
- [V8 builds take 1-2 h] → a persistent cache; abyme pins a bevy-ts build and doesn't rebuild V8
  per promote.
- [API 29 above abyme's `minSdk` 26] → abyme gates the game pane on API 29 (`Build.VERSION`), or
  raises `minSdk`; abyme's decision.
- [A large engine download per bevy-ts version: Bevy alone is 66.9 MB stripped, 22.3 MB gzipped;
  V8 not yet measured] → served gzipped; strip, thin LTO, `opt-level = "s"` for non-render crates.
- [`bevy_asset` pulls in `android-activity` on Android, which won't compile without an activity
  backend] → turn on `android-native-activity`, never used.
- [Skinned meshes break on Adreno (bevy#24926), open in 0.20.0, a GPU-path bug that hits agent
  town] → on Adreno only (`get_adreno_model`), set
  `WgpuSettings.constrained_limits.max_storage_buffers_per_shader_stage = 0`, which moves skinning
  and morphs to uniform buffers and turns off GPU preprocessing and storage-buffer clustering; it needs a real Adreno phone to confirm. PowerVR aborts (#25788): `gles` as the fallback
  backend.
- [Frame pacing with `multi_threaded`, MSAA cost and background CPU (from #26282's notes)] → the
  smoke run measures them; MSAA defaults off on Android if it costs.
- [Fonts: no fontconfig on Android] → fallback fonts read `/system/fonts` through fontdb; checked
  in the device walk (`docs/fonts.md`).
- [A held socket during a long pause overflows its queue] → the existing limits close it with
  1009, and the game's own reconnect logic runs at resume.

## Migration Plan

Lands in this order, each on its own green gate: move play and input into `bevy-ts-embed` (no
behaviour change; the CLI and viewport tests stay green); TLS and the header rule; the runner,
raw-handle window and C ABI, tested headless and on macOS with a real window; the Android library
and `scripts/android`; then abyme's change. Rollback is a revert: the CLI and viewport keep working
on the moved code.

## Open Questions

- The spike's verdict on V8 versus QuickJS-ng. This design and the specs are updated when it
  reports.
- How abyme gets the library: built by its `scripts/android` from the pinned bevy-ts, or a
  prebuilt bevy-ts publishes. Default: abyme builds it, with the V8 cache.
