# Releasing bevy-ts to npm

Seven packages: `bevy-ts` (the TS API, types and launcher) and one engine package per platform,
`@bevy-ts/<platform>` (`darwin-arm64`, `darwin-x64`, `linux-arm64`, `linux-x64`, `win32-x64`),
installed as `bevy-ts`'s optional dependencies the way esbuild and Biome ship. An eighth,
`@bevy-ts/android-arm64`, is the engine as a library for Android apps (below). Pre-releases go out
under the `next` dist-tag and, until a stable release exists, under `latest` too.

Only `@bevy-ts/darwin-arm64` ships for now. The other platforms keep their package folders, but
the release workflow neither builds nor publishes them, and `bevy-ts` lists only the engines it
was released with as optional dependencies. A platform is re-added by uncommenting its line in
`PLATFORMS`, in `.github/workflows/release.yml`'s first job:

```diff
-            # linux-x64 ubuntu-24.04
+            linux-x64 ubuntu-24.04
```

That one list drives the build and publish matrices and the platforms `bevy-ts` waits for and
lists. A platform never published needs its first version by hand first (below).

## Releasing, from CI

Push a tag: `git tag v0.1.0-alpha.1 && git push origin v0.1.0-alpha.1`. The `release` workflow
builds each released platform's engine on its native runner, publishes each platform package from a job of
its own, in parallel, with provenance through npm trusted publishing (OIDC, no token, no 2FA). Then
it checks that every released platform's engine built and, with `npm view`, is on npm at that version, and
only then publishes `bevy-ts` listing them all. A version with a `-` goes out under `next`, one
without under `latest`. Until a stable version exists, a pre-release also moves `latest`, so a
plain `npm i bevy-ts` gets the newest alpha.

npm fails now and then: a 503, 404s after it, a run so long its OIDC token expired. So each publish
(`scripts/npm-publish.sh`) retries a 5xx or 404 up to five times with backoff, each attempt a fresh
OIDC exchange, and takes "already published" as success. If an engine fails to build or a platform
still fails to publish, `bevy-ts` is held back rather than published without that engine:
"Re-run failed jobs" finishes the release, skipping whatever is already on npm.

Dry run: Actions > release > Run workflow, with a version and `publish` unticked.

macOS engines are built with `MACOSX_DEPLOYMENT_TARGET=14.0`, the oldest macOS Apple still patches.

## A package new to npm

npm sets a trusted publisher only on a package that exists, so a platform package's first version
is published once by hand on the operator's go; until then its publish fails and `bevy-ts` is held
back. Then, with npm >= 11.15 and the operator's 2FA:

```sh
npm trust github @bevy-ts/<platform> --repo abyme-studio/bevy-ts --file release.yml --allow-publish --yes
```

or on npmjs.com: the package > Settings > Trusted publishing > GitHub Actions, repository
`abyme-studio/bevy-ts`, workflow `release.yml`. `bevy-ts`'s trusted publisher also has "Allow npm dist-tag", which moves
`latest`.
`npm trust list <package>` shows what is set.

`bevy-ts` and `@bevy-ts/darwin-arm64` were first published by hand from the operator's Mac at
`0.1.0-alpha.0`; npm made that `latest` as well as `next`.

## The Android library

`@bevy-ts/android-arm64` holds the engine as a library for an Android app (docs/android.md):
`lib/libbevy_ts_android.so`, `lib/dev/bevyts/Engine.java` and `lib/bevy_ts_embed.h`. It is not one
of `bevy-ts`'s optional dependencies, and `bevy-ts` never waits for it: an app that embeds the
engine fetches the library at `bevy-ts`'s version itself.

The release workflow's `android` job builds it at every release on a Linux runner with
`scripts/android`. V8 has no Android prebuilt, so it builds from source once per `v8` version
(about an hour) and is cached under `~/.cache/bevy-ts/v8-android`. `android-publish` then publishes
it while `ANDROID_PUBLISH` in the first job's Platforms step is `true`; `false` makes its
publish a dry run. It is `false` until the package is on npm: its first version goes out by hand,
on the operator's go, from a dry run's `android-arm64` artifact, then its trusted publisher is set
as above, and only then is `ANDROID_PUBLISH` set to `true`:

```sh
gh run download <run> -n android-arm64 -D packages/platforms/android-arm64/lib
cd packages/platforms/android-arm64 && npm pkg set version=<version> && npm publish --access public --tag next
```

## The packed examples

`bevy-ts` carries the examples `bevy-ts new` and `bevy-ts examples` read, under `examples/`. They
are not committed there: `prepack` (`scripts/sync-examples.mjs`) copies them from the repository's
`examples/`, so `npm publish` in the release workflow picks them up with no step of its own.

An example ships when its directory holds an `example.json`: `id` (`[a-z0-9-]+`, unique),
`title`, `description` and `entry` (the entry file, relative to the example). The package takes
its `src/`, `assets/`, `bevy-ts.json`, `README.md`, `thumbnail.png` and `example.json`, nothing
else; walks and `tsconfig.json` stay in the repository. Only an example that runs on the prebuilt
engine may ship: packing fails on one with anything under `native` in `bevy-ts.json`. `cube` and
`arena` ship; `rapier` and `thrusters` wait for bevy-ts's crates on crates.io.

`pnpm test` in `packages/bevy-ts` syncs the examples, makes a project from each and type-checks it,
and checks `npm pack --dry-run` holds exactly them.

## Bumping Bevy

A Bevy bump re-vendors `vendor/bevy_render` at the new version and re-applies its patch, or
deletes the patch if Bevy shipped the fix ([PATCH.md](../vendor/bevy_render/PATCH.md)). A
`bevy-ts-cli` test fails while the vendored version differs from the lock's Bevy, since cargo would
otherwise drop a stale patch with only a warning.

A release tag `v<version>` is also where a project engine built from crates.io fetches the patched
`bevy_render`, so a published version's tag stays.
