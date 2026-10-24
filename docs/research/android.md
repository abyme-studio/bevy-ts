# bevy-ts on Android: the spike

Option B of the Android target (`~/.local/state/abyme/documents/bevy-ts/android-target.md`): bevy-ts'
engine as a `.so` in abyme's APK, drawing into its `SurfaceView`. This spike checks V8 and deno_core on
aarch64 Android, the sizes, Bevy without winit, and TLS.

Researched 2026-10-24 against bevy-ts `1cebc81e`: Bevy 0.20.0, wgpu 30, deno_core 0.412.0, and
`v8` 150.4.0 (the version in `Cargo.lock`; older notes say 152.2). NDK r28c (28.2.13676358), API 29,
built on the Mac (Apple silicon), run on a headless arm64 emulator (API 34, `-no-window`).

## V8 and deno_core

V8 RESULT PENDING: the builds are queued in a check slot.

### Why deno_core#738 crashes

Sure from the code; the run below confirms or refutes it.

- V8's GN turns on relative vtables for arm64 Android:
  `use_relative_vtables_abi = is_fuchsia || (is_android && current_cpu == "arm64" && use_custom_libcxx && !is_component_build)`
  (`build/config/compiler/compiler.gni:254`). rusty_v8 builds with `use_custom_libcxx` by default.
- With that flag, a C++ vtable holds 32-bit offsets, not pointers.
- rusty_v8's `OneByteConst` (`src/string.rs:270`) is a C++ `ExternalOneByteStringResource` laid out by
  hand in Rust, with a vtable of absolute function pointers.
- deno_core makes every static string with it (`FastStaticString`, through `v8::String::new_from_onebyte_const`)
  while it sets up `Deno.core` in `JsRuntime::new`. V8 reads the pointers as offsets and jumps to a
  misaligned address.
- That matches #738's tombstone: SIGBUS `BUS_ADRALN` with `pc` equal to the fault address, under
  `String::NewExternalOneByte` from `initialize_deno_core_namespace`. rusty_v8's own hello world works
  because it never uses `OneByteConst`.
- The fix to test: `GN_ARGS=use_relative_vtables_abi=false`. The other fix is a rusty_v8 patch that
  lays the vtable out as offsets on Android.

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

With these, `gn gen` makes 1117 targets.

## Sizes

Stripped `.so` for aarch64-linux-android, release (`lto = "thin"`, `codegen-units = 1`, `strip = true`).

| Library | Stripped | gzip -9 |
|---|---|---|
| Bevy, the shipped-game set without winit, audio or remote (`3d_bevy_render`, `scene`, `picking`, `bevy_ui_render`) | 66.9 MB | 22.3 MB |
| QuickJS-ng (`rquickjs` 0.14) | 1.46 MB | 0.68 MB |
| V8 + deno_core | pending | pending |

- In the Bevy probe, `.text` is 44.6 MB, `.eh_frame` 6.2 MB and `.rela.dyn` 6.2 MB.
- abyme's APK stores `.so` files uncompressed: `libabyme_phone.so` is "Stored" at 6.3 MB of the 8.5 MB APK.
  So the APK grows by the stripped size, not the gzipped one. With Bevy alone, 8.6 MB becomes about 75 MB.
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
