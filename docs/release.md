# Releasing bevy-ts to npm

Seven packages: `bevy-ts` (the TS API, types and launcher) and one engine package per platform,
`@bevy-ts/<platform>` (`darwin-arm64`, `darwin-x64`, `linux-arm64`, `linux-x64`, `win32-x64`),
installed as `bevy-ts`'s optional dependencies the way esbuild and Biome ship. Pre-releases go out
under the `next` dist-tag, so `npm i bevy-ts` installs nothing until a stable release; use
`npm i bevy-ts@next`.

## First publish, by hand, from the operator's Mac

The packages do not exist on npm yet, so trusted publishing cannot be configured; the first version
is published by hand, on the operator's go.

1. The operator creates the npm org `bevy-ts` and runs `npm login`.
2. `node scripts/set-npm-version.mjs 0.1.0-alpha.0 darwin-arm64` sets every version and lists only
   `darwin-arm64` in `bevy-ts`'s optional dependencies, the one platform this Mac builds.
3. `scripts/pack-npm.sh` builds the release engine and packs `@bevy-ts/darwin-arm64` and `bevy-ts`
   into `dist/npm`. Inspect them (`tar tzf`).
4. `npm publish dist/npm/bevy-ts-darwin-arm64-0.1.0-alpha.0.tgz --access public --tag next`, then
   `npm publish dist/npm/bevy-ts-0.1.0-alpha.0.tgz --access public --tag next`.
5. On npmjs.com, for each published package: Settings > Trusted publishing > GitHub Actions,
   repository `abyme-studio/bevy-ts`, workflow `release.yml`.
6. Each new platform package must also be published once by hand, or with an `NPM_TOKEN` secret in
   the repository, before trusted publishing can take it over.

## Later releases, from CI

Actions > release > Run workflow, with the version (for example `0.1.0-alpha.1`), the dist-tag and
`publish` ticked; unticked is a dry run. Each platform builds on its own native runner. The ones that
build are published with provenance, then `bevy-ts` is published listing only those.
