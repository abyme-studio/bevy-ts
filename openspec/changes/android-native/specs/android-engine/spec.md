## ADDED Requirements

### Requirement: An Android engine library with a Java class
bevy-ts SHALL build `libbevy_ts_android.so` for `aarch64-linux-android` (Android API 29 and later) from `crates/bevy-ts-android`, and SHALL ship the Java class `dev.bevyts.Engine` (`crates/bevy-ts-android/java/dev/bevyts/Engine.java`) whose native methods it exports over JNI. The class SHALL NOT load the library by name: the host SHALL call `static void load(String absolutePath)` once, which calls `System.load` on that path and checks the library's version against the class's own, throwing `UnsatisfiedLinkError` on a mismatch. The class SHALL offer `static String version()` (the bevy-ts version it was built with), `static Engine create(String configJson, Surface surface, int width, int height, float scale, Listener listener)`, `void surface(Surface surface, int width, int height, float scale)`, `void pause()`, `void resume()`, `void input(String json)` and `void close()`, each calling the matching C ABI function (engine-embedding), and `interface Listener { void onEvent(String json); }`, called on the engine's thread. `create` SHALL throw `IllegalStateException` with the C ABI's error. A null `Surface` SHALL mean no window. `scripts/android` SHALL build the library and write it, `Engine.java` and `bevy_ts_embed.h` to `dist/aarch64-linux-android/`.

#### Scenario: Building for the phone
- **WHEN** `scripts/android` runs on the Mac with the NDK installed
- **THEN** `dist/aarch64-linux-android/` holds `libbevy_ts_android.so`, `dev/bevyts/Engine.java` and `bevy_ts_embed.h`, and `llvm-readelf` shows the `.so` exports `Java_dev_bevyts_Engine_nativeCreate`

#### Scenario: Loading a downloaded engine
- **WHEN** a host calls `Engine.load("/data/user/0/app.abyme.android/files/engines/0.4.0/libbevy_ts_android.so")` with a library of that version
- **THEN** it returns, and `create` works; with a library of another version, `load` throws naming both versions

#### Scenario: A host in Kotlin
- **WHEN** after `Engine.load`, a Kotlin host calls `Engine.create(config, holder.surface, w, h, density, listener)` from `surfaceCreated`
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

### Requirement: Any prebuilt build's game folder plays on the phone
A game folder SHALL be platform-neutral: the `game/` that `bevy-ts build` writes for any target, with `engine: "prebuilt"`, SHALL play on the Android engine of the same bevy-ts version, with no Android build target. The engine SHALL refuse a `game.json` whose `bevyTs` is not its own version exactly, naming both, as the desktop engine does, so a host SHALL keep an engine of each version its games need.

#### Scenario: A game built on the Mac
- **WHEN** abymed serves `dist/aarch64-apple-darwin/game/` built by bevy-ts 0.4.0 and the phone's 0.4.0 engine is created on a downloaded copy
- **THEN** the game plays

#### Scenario: A game from another version
- **WHEN** the same folder is created on a 0.4.1 engine
- **THEN** `create` fails naming 0.4.0 and 0.4.1
