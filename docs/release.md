# Releasing bevy-ts to npm

Seven packages: `bevy-ts` (the TS API, types and launcher) and one engine package per platform,
`@bevy-ts/<platform>` (`darwin-arm64`, `darwin-x64`, `linux-arm64`, `linux-x64`, `win32-x64`),
installed as `bevy-ts`'s optional dependencies the way esbuild and Biome ship. Pre-releases go out
under the `next` dist-tag and, until a stable release exists, under `latest` too.

## Releasing, from CI

Push a tag: `git tag v0.1.0-alpha.1 && git push origin v0.1.0-alpha.1`. The `release` workflow
builds each platform's engine on its native runner, publishes the platform packages with
provenance through npm trusted publishing (OIDC, no token, no 2FA), then publishes `bevy-ts`
listing only the platforms now on npm at that version. A version with a `-` goes out under `next`,
one without under `latest`. Until a stable version exists, a pre-release also moves `latest`, so a
plain `npm i bevy-ts` gets the newest alpha. Re-running is safe: a package already on npm at the version is skipped.

Dry run: Actions > release > Run workflow, with a version and `publish` unticked.

macOS engines are built with `MACOSX_DEPLOYMENT_TARGET=14.0`, the oldest macOS Apple still patches.

## A package new to npm

npm sets a trusted publisher only on a package that exists, so a platform package's first version
is published once by hand on the operator's go; until then the workflow warns, leaves that platform
out of `bevy-ts`, and carries on. Then, with npm >= 11.15 and the operator's 2FA:

```sh
npm trust github @bevy-ts/<platform> --repo abyme-studio/bevy-ts --file release.yml --allow-publish --yes
```

or on npmjs.com: the package > Settings > Trusted publishing > GitHub Actions, repository
`abyme-studio/bevy-ts`, workflow `release.yml`. `bevy-ts`'s trusted publisher also has "Allow npm dist-tag", which moves
`latest`.
`npm trust list <package>` shows what is set.

`bevy-ts` and `@bevy-ts/darwin-arm64` were first published by hand from the operator's Mac at
`0.1.0-alpha.0`; npm made that `latest` as well as `next`.
