## Why

The operator wants bevy-ts games on their Android phone, run by the phone itself, inside abyme's
app. Research (`~/.local/state/abyme/documents/bevy-ts/android-target.md`, option B) picked a
native engine drawing into the app's `SurfaceView`. bevy-ts can't do it today: its only engine is
a desktop CLI that owns its window, finds its game beside `current_exe()`, reads its settings from
the process environment, has no TLS, and links V8 from a prebuilt that has no Android build.

## What Changes

- **An embedding API.** A host creates an engine on a native surface it owns, and drives it:
  `create`, `surface` (new or resized), `pause`, `resume`, `input` (one SURFACE.md
  `ViewportInput` event as JSON), `destroy`. Events come back on one callback as JSON lines:
  `started`, `log`, `ime`, `exited`. The contract is a C ABI (`bevy_ts_embed.h`). The engine runs
  on a thread of its own, and every call returns at once.
- **The host gives the game.** `create` takes the game folder's path (the one holding
  `game.json`), so the engine never looks beside its executable. abyme's shell downloads `game/`
  from abymed into app storage; bevy-ts only reads the folder it is given.
- **The host gives the environment.** `create` takes the `env` map that `process.env` and
  `${NAME}` in `net.allow` read, in place of the process environment.
- **No winit.** The engine takes a raw window handle from the host and gives Bevy a window entity
  on it. wgpu draws into it with Vulkan. A new surface replaces the old one, and a destroyed one
  stops drawing while the world keeps its state.
- **An Android engine library.** `libbevy_ts_android.so` for `aarch64-linux-android`, with JNI
  exports for a `dev.bevyts.Engine` Java class that bevy-ts ships. It takes the `Surface` through
  `ANativeWindow_fromSurface`. It plays prebuilt-engine games only, with no compiler, remote,
  inspector, viewport or hot reload.
- **V8 from source for Android.** `bevy-ts-v8` builds with `V8_FROM_SOURCE=1` through cargo-ndk.
  The spike ran deno_core on an arm64 Android emulator: it works with a 3-line deno_core patch and
  compiler-rt's builtins at link. No second JS engine.
- **`wss:`.** The `WebSocket` client gains TLS (rustls with webpki roots). `wss:` URLs to allowed
  hosts open on every target.
- **The header rule.** A `headers` option is allowed to a loopback host, as now, or over `wss:`
  to an allowed host. A key still never crosses the network in clear text. **BREAKING** only for
  the refusal message, which changes.
- **Input moves out of the viewport crate.** The `ViewportInput` to Bevy-message mapping becomes
  platform-neutral, so the macOS viewport and the Android engine apply the same events one way.
- **Shipped play becomes a library.** The play app (game folder, scripts, net policy, plugin set)
  moves out of `bevy-ts-cli` so the CLI and the embedding build the same app.
- **Out of scope:** abyme's side (the download, the Kotlin glue, the pane, the page's input
  forwarding, APK packaging), which is abyme's change; a web build (option A); iOS (the C ABI is
  ready for it); project-engine games on Android; gamepads and keyboards on the phone beyond what
  the events already carry; audio.

## Capabilities

### New Capabilities

- `engine-embedding`: the platform-neutral embedding API: create on a surface, surface changes,
  pause and resume, input, destroy, events out, the game folder and env the host gives.
- `android-engine`: the Android library: JNI and the Java class, `ANativeWindow`, the build for
  `aarch64-linux-android`, the JS engine on Android, what it plays and what it leaves out.

### Modified Capabilities

- `script-networking`: `wss:` is supported; headers are allowed over `wss:`; the environment can
  come from the host.
- `shipped-game`: a game folder can be given by an embedding host, not only found beside the
  executable or named to `bevy-ts play`.
- `viewport-input`: the same events arrive from an embedding host and are applied the same way.

## Impact

- New crates: `crates/bevy-ts-embed` (the play app as a library, the host-driven runner, the C
  ABI, the raw-handle window, the input mapping moved from the viewport) and
  `crates/bevy-ts-android` (cdylib, JNI, `java/dev/bevyts/Engine.java`).
- `crates/bevy-ts-cli`: `play.rs` and the shipped part of `cli.rs`'s `base_app` move to
  `bevy-ts-embed`; the CLI calls them. `beside_exe()` stays in the CLI.
- `crates/bevy-ts-viewport`: `input.rs` moves to `bevy-ts-embed`; the viewport calls it.
- `crates/bevy-ts`: `net/` (TLS, the header rule, `wss:` refusal gone).
- `crates/bevy-ts-v8`: an Android build path. A `[patch.crates-io]` for deno_core's Android errno.
- Docs: a new `docs/android.md`; `docs/networking.md`, `docs/build.md`, `docs/viewport.md`.
- abyme: a follow-up change in `crates/phone` (Kotlin, Gradle, the download), named here, not done
  here.
- Needs the operator's go:
  - **V8 from source in the build.** 47 min a clean build at `-j4` on the Mac, cached after; a
    patched V8 checkout to build from a Mac; NDK r28 and API 29, above abyme's `minSdk` 26; a
    patched deno_core until upstream takes the fix. The alternative is QuickJS-ng (no JIT, 1 MB).
  - **A TLS crate.** `futures-rustls` with `ring` and `webpki-roots`, in `bevy-ts` on every
    target. New dependencies.
  - **The header rule.** Headers allowed over `wss:` to any allowlisted host, not only loopback.
    The alternative is headers only to an origin the embedding host names.
  - **The engine as a per-version download, loaded by path.** The `.so` is not in abyme's APK: the
    host downloads one per bevy-ts version into `filesDir/engines/<version>/` and loads it with
    `Engine.load(path)` (`System.load`). The APK stays at 8.6 MB, but it is native code
    downloaded at run time: fine outside Play, refused by Play's policy, and dlopen from app
    storage on API 29+ must be proven (W^X forbids exec there, not mapping a library, as far as
    known). Each download, measured by the spike: about 27 MB gzipped, 73 MB on disk (Bevy
    30.7 MB and V8 42.2 MB stripped, in the size profile). `Engine.load` checks an ABI number, not the bevy-ts
    version, so one APK loads any engine of its ABI.
  - **A new npm package and release job.** `@bevy-ts/android-arm64` at every release, built on a
    Linux runner with V8 from source (1-2 h uncached), its first version published by hand. abymed
    fetches it by the game's version.
  - **A no-winit window path.** Bevy gets a window it didn't create, from a raw handle; a new
    architectural pattern, and two new crates.
  - **Performance:** a phone GPU and CPU run the game; skinned meshes are known broken on some
    Adreno GPUs (bevy#24926, open in 0.20.0); the fix forces skinning onto uniform buffers on
    Adreno only, and needs a real Adreno phone to confirm.
