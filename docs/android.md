# bevy-ts on Android

A built game plays inside an Android app: the engine is a library the app loads, drawing into a
`Surface` the app owns. abyme's phone app is the first host. There is no Android `bevy-ts` binary
and no APK of bevy-ts's own.

## The pieces

- `crates/bevy-ts-embed`: the engine as a library on every platform. It plays a built game folder
  (`game/` from `bevy-ts build`, [build.md](build.md)) with the same plugins as `bevy-ts play`, on a
  window the host gives it, and exports the C ABI in `include/bevy_ts_embed.h`.
- `crates/bevy-ts-android`: `libbevy_ts_android.so`, JNI over that C ABI for one Java class.
- `crates/bevy-ts-android/java/dev/bevyts/Engine.java`: the class an app compiles in.
- `scripts/android`: builds the library and writes `dist/aarch64-linux-android/` (the `.so`,
  `dev/bevyts/Engine.java`, the header).
- `@bevy-ts/android-arm64` on npm: those three files at each bevy-ts version ([release.md](release.md)).

Only arm64 (`arm64-v8a`), API 29 (Android 10) and later.

## The Java class

```java
Engine.load(new File(getFilesDir(), "libbevy_ts_android.so").getAbsolutePath());
Engine engine = Engine.create(configJson, holder.getSurface(), width, height, density, json -> ...);
engine.surface(newSurface, w, h, density);   // a new surface, or the same one at a new size
engine.surface(null, 0, 0, density);         // in surfaceDestroyed: waits until the engine let go
engine.pause(); engine.resume();             // onPause, onResume
engine.input(json);                          // one SURFACE.md input event, x and y in points
engine.close();                              // stops, sends exited, joins
```

- `load(path)` loads the library by absolute path, never by name: the library is a per-version
  download into the app's own storage, not part of the APK. It throws `UnsatisfiedLinkError` when the
  library is of another ABI (`Engine.ABI`, the header's `BEVY_TS_ABI`), naming both. Any bevy-ts
  version of the same ABI loads; `Engine.version()` says which.
- `create(configJson, ...)` takes `{ "game": "<absolute folder holding game.json>", "env": {...},
  "headersOrigin": "https://host:port", "originHeaders": { "authorization": "Bearer <key>" },
  "headless": false }`. It checks `game.json` as `bevy-ts
  play` does and throws `IllegalStateException` with the same message for a game built by another
  version or for a project engine. One engine per process: a second `create` throws "an engine is
  already running".
- `env` is the game's whole environment: `process.env` and `${NAME}` in `net.allow` read it, never
  the app's process. `headersOrigin` is the one non-loopback origin a script may send WebSocket
  headers to, over `wss:` only ([networking.md](networking.md)).
- `originHeaders` are the host's own (a device key): the engine adds them to every `wss:` handshake
  and `https:` fetch to `headersOrigin`, replacing a script's of the same name, and sends them
  nowhere else, not even on a redirect away. No script reads them, so pass the key here, never in
  `env`.
- Width and height are physical pixels, `scale` pixels per point (`DisplayMetrics.density`).
- Events come one JSON object per call, on the engine's thread: `started` (`inputs`, `js`), `log`
  (`level`, `message`, `file`, `line`, source-mapped to the TS), `ime` (`enabled`, `disabled`) and
  `exited` (`code`, `message`), last. Never call `surface(null, ...)` or `close()` from the listener.

## The lifecycle

The engine runs frames only while it is resumed and has a surface. `pause` stops it after the
current frame; the world, its scripts, sockets and timers are kept, and the first frame after
`resume` sees a delta capped at Bevy's maximum, so game time does not jump. `surface(null)` drops the
GPU surface and the engine's reference to the `ANativeWindow` (it holds its own, from
`ANativeWindow_acquire`) and returns once both are gone, within 2 s. The next surface draws the same
world. `close` closes every socket with 1001 and returns within 5 s; a new engine may then be
created.

## Building

```sh
scripts/android
```

It needs the NDK r28c (`ANDROID_NDK_HOME`, else under `ANDROID_HOME`) and `cargo-ndk`, and builds
`-p bevy-ts-android` with `cargo ndk -t arm64-v8a -P 29` in the `android` profile (size-optimised,
fat LTO, stripped, unwinding kept for the engine's panic guard).

- V8 has no Android prebuilt. The first build of a `v8` version builds it from source (about 47
  min at `-j4` on a Mac, `BEVY_TS_V8_JOBS`), with the Mac-host patch set from
  `docs/research/android.md`, and caches `librusty_v8.a` under `~/.cache/bevy-ts/v8-android/`.
  Later builds link the cache.
- deno_core 0.412 does not compile for Android as published (bionic's errno). The script patches a
  copy (`scripts/android-patches/`) in for its build only and puts `Cargo.lock` back after. The patch
  is carried here, not sent upstream (operator, 2026-10-28): a deno_core upgrade re-checks it.
- CI builds the library on every push and checks its JNI exports (`ci.yml`, `android`).

`bench/android-smoke/run.sh` plays a game on a headless arm64 emulator through a minimal Java host,
loading the library from `filesDir`, and records sizes and the first-frame time in
`bench/android-smoke/results/`. On arena (API 34 emulator, SwiftShader):

| | |
|---|---|
| `.so`, stripped | 87 MB |
| `.so`, gzip -9 (the download) | 33 MB |
| `Engine.load` | 0.36 s |
| `create` to first frame | 5.4 s |
| `surface(null)` returns | 26 ms |

A real GPU's first frame is faster; the emulator renders on the CPU.

## What the phone plays

A game the prebuilt engine plays: scripts, levels, materials and world text, animation, UI,
picking, gizmos, diagnostics, `WebSocket` and `fetch` (both in clear and over TLS, trusting
Mozilla's roots). Not on Android: a project engine (a game with Rust, [native.md](native.md)),
`bevy-ts dev` and its viewport, gamepads, and audio (`bevy-ts/audio` is the CLI's alone, so a game
that imports it does not load). Saves have no folder yet: the host names none. Text fields get the soft keyboard through the
host: the engine sends `ime`, and the host sends the keyboard's `ime` and `key` input back.

On an Adreno GPU the engine holds Bevy's device to no storage buffers per shader stage, which
moves skinning to uniform buffers (bevy#24926: skinned meshes break on Adreno otherwise).
