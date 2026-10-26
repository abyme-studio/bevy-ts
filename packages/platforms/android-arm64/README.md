# @bevy-ts/android-arm64

The bevy-ts engine as a library for an Android app on arm64 (API 29+), at the same version as
`bevy-ts`. Not a dependency of `bevy-ts`: an app that embeds the engine fetches it itself.

- `lib/libbevy_ts_android.so`: the engine, V8 and Bevy in one library.
- `lib/dev/bevyts/Engine.java`: the Java class over it. `Engine.load(path)` loads the library by
  absolute path and refuses one built for another ABI.
- `lib/bevy_ts_embed.h`: the C ABI under the JNI layer.

How to embed it: docs/android.md in the bevy-ts repository.
