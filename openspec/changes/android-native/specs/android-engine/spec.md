## ADDED Requirements

### Requirement: An Android engine library with a Java class
bevy-ts SHALL build `libbevy_ts_android.so` for `aarch64-linux-android` (Android API 29 and later) from `crates/bevy-ts-android`, and SHALL ship the Java class `dev.bevyts.Engine` (`crates/bevy-ts-android/java/dev/bevyts/Engine.java`) whose native methods it exports over JNI. The class SHALL load the library itself and SHALL offer `static Engine create(String configJson, Surface surface, int width, int height, float scale, Listener listener)`, `void surface(Surface surface, int width, int height, float scale)`, `void pause()`, `void resume()`, `void input(String json)` and `void close()`, each calling the matching C ABI function (engine-embedding), and `interface Listener { void onEvent(String json); }`, called on the engine's thread. `create` SHALL throw `IllegalStateException` with the C ABI's error. A null `Surface` SHALL mean no window. `scripts/android` SHALL build the library and write it, `Engine.java` and `bevy_ts_embed.h` to `dist/aarch64-linux-android/`.

#### Scenario: Building for the phone
- **WHEN** `scripts/android` runs on the Mac with the NDK installed
- **THEN** `dist/aarch64-linux-android/` holds `libbevy_ts_android.so`, `dev/bevyts/Engine.java` and `bevy_ts_embed.h`, and `llvm-readelf` shows the `.so` exports `Java_dev_bevyts_Engine_nativeCreate`

#### Scenario: A host in Kotlin
- **WHEN** a Kotlin host calls `Engine.create(config, holder.surface, w, h, density, listener)` from `surfaceCreated`
- **THEN** the game draws into that `SurfaceView` and the listener gets `started`

### Requirement: The Surface becomes the engine's window
The JNI layer SHALL turn a `Surface` into an `ANativeWindow` with `ANativeWindow_fromSurface` and pass it to the C ABI, and SHALL release its own reference after the call. Bevy SHALL get a window entity carrying that window's raw Android handle, with no winit, and wgpu SHALL draw to it with Vulkan. The engine SHALL NOT depend on winit, `android-activity` or `GameActivity`.

#### Scenario: No activity of its own
- **WHEN** the engine runs inside abyme's app
- **THEN** the app's own activity is the only one, and the engine library links no winit symbols

### Requirement: Scripts run as on the desktop
The Android engine SHALL run a game's compiled scripts with the same globals, networking, timers and source-mapped errors as the desktop engine. Its JS engine SHALL be V8, built from source for Android. If V8 or deno_core cannot run on Android, it SHALL be QuickJS-ng behind the same `ScriptBackend` seam, and the `started` event's `js` SHALL say which.

#### Scenario: A game that connects to abymed
- **WHEN** a built game whose script calls `@abyme/client`'s `connect()` plays on the phone with `ABYME_URL` set to the Mac's `https:` tailnet address and `ABYME_KEY` to the device key
- **THEN** it connects over `wss:` and receives abymed's hello

### Requirement: It plays shipped prebuilt-engine games only
The Android engine SHALL play only games whose `game.json` has `engine: "prebuilt"`, refusing others with the message the desktop engine gives for a project engine's game. It SHALL contain no TS compiler, file watcher, remote protocol server, inspector, studio viewport tools or hot reload, and SHALL open no listening port.

#### Scenario: A project engine's game
- **WHEN** the host creates an engine on a game built with native crates (`engine` a hash)
- **THEN** `create` fails, saying the game was built for its project engine

#### Scenario: Nothing listens on the phone
- **WHEN** a game is playing on the phone
- **THEN** the app's process has no listening socket the engine opened
