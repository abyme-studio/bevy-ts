## Context

The npm launcher `packages/bevy-ts/bin/bevy-ts.js` resolves the platform package's engine binary and passes every argument to it. The binary's commands are `dev`, `build`, `types`, `transform`, `skeleton` and `levels check` (`crates/bevy-ts-cli/src/cli.rs`); there is no `new`. `examples/` holds `cube`, `arena`, `rapier` and `thrusters`. Their `tsconfig.json` maps `bevy-ts` to `../../packages/bevy-ts`, and `thrusters/native/Cargo.toml` depends on `bevy-ts-native` by path. A project engine outside a checkout takes bevy-ts's crates from crates.io (`engine.rs`, `Source::Registry`), and nothing publishes them there yet.

abyme's start view (spec@bevy-start) calls `npx --yes bevy-ts@<pinned> examples --json` and `new <dir> --example <id> --json`, refuses a non-empty directory itself, then runs `npx --yes bevy-ts dev` in the project.

## Goals / Non-Goals

**Goals:**
- A user or abyme lists the examples and makes a playable project from one, with no checkout and no Rust.
- One source of truth per example: its directory under `examples/`.

**Non-Goals:**
- Shipping examples that need a project engine. They ship once bevy-ts's crates are on crates.io; `example.json` is all they will need.
- Making thumbnails. The format carries one; producing them is later work.
- A blank template distinct from `cube`.

## Decisions

1. **The launcher owns `new` and `examples`, in Node.** The files live in the npm package, which the launcher knows by `import.meta.url`; it knows its own version from `package.json`; and listing works with no platform package and no engine start. Embedding the examples in the binary would copy them into five platform packages and need a rebuild to change one. The binary refuses both commands so a direct run is not a silent "usage".

2. **`example.json` in the example's directory marks it shipped.** No central list to keep in step. The prepack script fails on an example that declares itself with anything under `native`, so a Rust example cannot ship by mistake.

3. **Copied at pack time, not committed.** `scripts/sync-examples.mjs` runs as `prepack` and copies the shipped files into `packages/bevy-ts/examples/` (gitignored), so the release workflow's `npm publish` picks them up with no change to `release.yml`. `runtime/` is committed with a `--check` because Node imports it in tests; examples have no such reader. Tests run the sync first.

4. **`new` writes `package.json`, `tsconfig.json` and `.gitignore` itself.** It does not copy the example's `tsconfig.json` (its `paths` point into the checkout); it writes the same compiler options without `paths`, including `.bevy-ts/types` as rapier's does. `bevy-ts` is pinned at exactly the running version, so the project gets the engine abyme chose.

5. **Install by default, with the package manager that ran it.** Agreed with spec@bevy-start: without `node_modules` the editor has no types and `npx bevy-ts dev` may fetch a version other than the pinned one. `npm_config_user_agent` names the runner (`npm/…`, `pnpm/…`, `yarn/…`, `bun/…`). Install output goes to stderr so `--json` stdout stays one object.

6. **Refuse a non-empty directory; create a missing one.** Nothing is ever overwritten. Validation happens before the first write, so a refusal writes nothing.

## Risks / Trade-offs

- [The pack-time copy drifts from a test of it] → The vitest suite runs the sync, then packs with `npm pack --dry-run --json` and checks the file list.
- [An example's entry or imports break against the published types] → The test makes a project from each shipped example with `--no-install`, links the local package and runs `tsc` on it.
- [`new` reaches the registry] → Only in the install step, which `--no-install` skips; a failure leaves the project and reports `installed: false`.
