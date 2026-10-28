# bevy-ts on Android: the spike

Option B of the Android target (`~/.local/state/abyme/documents/bevy-ts/android-target.md`): bevy-ts'
engine as a `.so` in abyme's APK, drawing into its `SurfaceView`. This spike checks V8 and deno_core on
aarch64 Android, the sizes, Bevy without winit, and TLS.

Researched 2026-10-24 and 2026-10-25 against bevy-ts `1cebc81e`: Bevy 0.20.0, wgpu 30, deno_core 0.412.0, and
`v8` 150.4.0 (the version in `Cargo.lock`; older notes say 152.2). NDK r28c (28.2.13676358), API 29,
built on the Mac (Apple silicon), run on a headless arm64 emulator (API 34, `-no-window`).

## V8 and deno_core

**V8 works.** deno_core 0.412's `hello_world` runs on the arm64 emulator (Android 14): `JsRuntime::new`
returns, the script runs, and `1+2+3` comes back as 6. The `TypeError` it prints after that is part of the
example, which passes a number on purpose to show the error path. No crash, nothing in the crash log.

### deno_core#738 is fixed upstream

- The cause was relative vtables. V8's GN turns them on for arm64 Android
  (`build/config/compiler/compiler.gni:254`, when `use_custom_libcxx` is on, which is rusty_v8's default).
  rusty_v8's `OneByteConst` (`src/string.rs:270`) lays out a C++ vtable by hand, with absolute pointers.
  deno_core makes its static strings with it in `JsRuntime::new`, so V8 jumped to a misaligned address.
  That gave #738's SIGBUS `BUS_ADRALN`.
- rusty_v8 turned them off in its own `.gn` (`use_relative_vtables_abi = false`, `.gn:79`):
  PR #1581, "Disable relative vtable on Android", 2024-08-26. Every `v8` crate since then has it.
- So the stock build and one with `GN_ARGS=use_relative_vtables_abi=false` are the same: the two
  `librusty_v8.a` files and the two `hello` binaries are byte for byte equal, and both run.
- QuickJS-ng is not needed. It is measured below only for comparison.

### deno_core 0.412 on Android

- It does not compile for Android as published: `uv_compat/tty.rs` has `errno_location` for Linux and
  macOS only, and hits `compile_error!` on Android. The fix is 3 lines, bionic's `__errno`:
  ```rust
  #[cfg(target_os = "android")]
  fn errno_location() -> *mut c_int { unsafe extern "C" { fn __errno() -> *mut c_int; } unsafe { __errno() } }
  ```
  Carry it as a `[patch.crates-io]` (carried, not sent upstream: operator, 2026-10-28).
- Linking needs compiler-rt's builtins: V8's `cpu-arm64.cc` calls `__clear_cache`. Pass
  `-C link-arg=$NDK/toolchains/llvm/prebuilt/darwin-x86_64/lib/clang/19/lib/linux/libclang_rt.builtins-aarch64-android.a`.

### Building V8 for Android from a Mac

rusty_v8's Android path has rotted since its Android CI stopped (PR 1558, 2024). From a Mac host it
needs all of this:

1. A git checkout with submodules (`git clone --depth 1 --branch v150.4.0 --recurse-submodules`,
   1.2 GB). The published `v8` crate leaves out `build/android/`, and `gn gen` fails without it.
2. `build/config/BUILDCONFIG.gn` refuses a host other than Linux for Android. Its own comment says a Mac
   host is "best-effort". Allow `host_os == "mac"`.
3. Nothing defines `android_ndk_version` any more (`build/config/android/BUILD.gn:37`). Write the version in.
4. Symlink the NDK at `third_party/android_toolchain/ndk`.
5. Create a stub file at `third_party/android_ndk/.../linux-x86_64/bin/aarch64-linux-android24-clang++`,
   or `build.rs` downloads a Linux NDK for `cross`'s linker.
6. Chromium's Mac clang ships no Android runtimes, so ninja stops on
   `libclang_rt.builtins-aarch64-android.a`. Copy the NDK's `lib/clang/19/lib/linux/` into
   that clang's `lib/clang/23/lib/linux/`, and pass its folder as `clang_base_path` in `GN_ARGS`.

With these, `gn gen` makes 1117 targets. The build took 47 min at `-j4` on the Mac. Cap ninja with a
wrapper set in `NINJA` that drops `build.rs`'s `-j` and passes `-j4`: without it ninja runs a job per core
and the load went past 90.

## Sizes

Stripped, for aarch64-linux-android. "release" is `lto = "thin"`, `codegen-units = 1`, `strip = true`;
"size" adds `opt-level = "z"`, `lto = "fat"`, `panic = "abort"`.

| Library | release | release gzip -9 | size | size gzip -9 |
|---|---|---|---|---|
| Bevy, the shipped-game set without winit, audio or remote (`3d_bevy_render`, `scene`, `picking`, `bevy_ui_render`) | 66.9 MB | 22.3 MB | 30.7 MB | 12.1 MB |
| V8 + deno_core (`hello`) | 42.2 MB | 15.2 MB | not built | |
| QuickJS-ng (`rquickjs` 0.14) | 1.46 MB | 0.68 MB | 1.01 MB | 0.52 MB |

- In the Bevy probe, `.text` is 44.6 MB, `.eh_frame` 6.2 MB and `.rela.dyn` 6.2 MB.
- In `hello`, `.text` is 26.3 MB, `.rodata` 11.6 MB (most of it ICU data, built in) and `.eh_frame` 3.6 MB.
  Nearly all of it is V8's C++, which the Rust profile does not touch, so a size build would save little.
  The size build was skipped: it rebuilds V8 (47 min) for that.
- abyme's APK stores `.so` files uncompressed: `libabyme_phone.so` is "Stored" at 6.3 MB of the 8.5 MB APK.
  So the APK grows by the stripped size, not the gzipped one.
- Bevy (size) plus V8 in the APK: 8.6 + 30.7 + 42.2 = about 82 MB. Release Bevy: about 118 MB.
- As a per-version download (gzip): 12.1 + 15.2 = about 27 MB, on top of the 8.6 MB APK.
- With QuickJS-ng in place of V8: 8.6 + 30.7 + 1.0 = about 40 MB in the APK, or a 12.6 MB download.
- `jniLibs.useLegacyPackaging = true` would compress them in the APK, at the cost of Android extracting
  them on install.

## Bevy 0.20 without winit

From the 0.20.0 sources. About 90% sure; nothing rendered yet.

- **It works.** bevy_render needs `Window`, `PrimaryWindow` and `RawHandleWrapper` on the window entity
  (`bevy_render/src/view/window/mod.rs:123`). It makes the surface from the raw handle, and nothing in
  that path calls winit.
- **The handle.** Build `RawHandleWrapper::new(&WindowWrapper::new(w))` (`bevy_window/src/raw_handle.rs:66`).
  `w` returns `AndroidNdkWindowHandle` and `AndroidDisplayHandle`. Acquire the `ANativeWindow` when
  making `w`, and release it in `Drop`.
- **Destroy and create.** Remove `RawHandleWrapper` in `surfaceDestroyed` and insert a new one in
  `surfaceCreated`. bevy_render keeps the render entity for exactly this; its comment names Android
  suspend (`mod.rs:194`). The removal is only seen at the next extract, so run an update before
  `surfaceDestroyed` returns. Turn `PipelinedRenderingPlugin` off, so one update is enough.
- **Resize.** Set `Window.resolution` and write `WindowResized` (and `WindowCreated` and
  `WindowScaleFactorChanged`) ourselves. Camera targets only update on those messages.
- **The runner.** Use our own `set_runner` loop, like `bevy-ts-viewport/src/runner.rs`.
- **Assets.** On Android, Bevy's default asset reader panics without an android-activity `AndroidApp`
  (`bevy_asset/src/io/android.rs:23`). Register a `FileAssetReader` on the game's folder.
- **The build.** `bevy_asset` always pulls in `bevy_android` and `android-activity` on Android, and that
  won't compile unless an activity backend is chosen. Turn on `android-native-activity` even though we
  never use it.
- **Prior art.** `jinleili/bevy-in-app` does the same with Bevy 0.18: a `SurfaceView` over JNI, no winit.

## The two Bevy issues

- **#26282** (Android issue list, open, 2026-10-23). Its winit and GameActivity items don't touch us:
  resume not recreating the surface, the crash past 8 touches, the missing `WillSuspend`.
  Four of its items do apply:
  - unsteady frames with `multi_threaded`
  - MSAA on by default
  - CPU use in the background (our runner must stop on pause)
  - unsteady delta time
- **#24926** (skinned meshes broken on Adreno, open, 2026-07-09, no fix in 0.20.0). It applies to us:
  it is in the GPU path, not winit, and agent town is skinned.
  - Workaround: on Adreno only (`get_adreno_model`), set
    `WgpuSettings.constrained_limits.max_storage_buffers_per_shader_stage = 0`. That moves skinning and
    morphs to uniform buffers together.
  - It also turns off GPU preprocessing and storage-buffer clustering. About 75% sure it is enough;
    test on an Adreno phone.

## TLS for wss:

- **Pick `futures-rustls` 0.26 with `ring`, and `webpki-roots` 1.**
  - ring builds with the NDK's clang.
  - aws-lc-rs (rustls' default) needs a cmake cross build.
  - native-tls means OpenSSL on Android.
  - `webpki-roots` carries ISRG Root X1 and X2, which covers `tailscale serve`'s Let's Encrypt
    certificate.
  - `rustls-platform-verifier` needs a JNI init with the app's `Context`: not worth it for one host.
- **Keep async-tungstenite 0.33 on `handshake` only.** Its `async-tls` feature is rustls 0.21, and its
  rustls features need tokio. Wrap the `Async<TcpStream>` in `futures_rustls::TlsConnector` before
  `client_async_with_config` (`crates/bevy-ts/src/net/socket.rs:222`).
- **The rules.** `policy.rs:193` refuses `wss:` and `policy.rs:201` refuses headers off loopback. Drop
  the first, and let headers through over `wss:` too:
  `!headers.is_empty() && !(t.secure || is_loopback(&t.host))`. The allowlist check before it still
  limits headers to hosts `net.allow` names.
