## Why

abyme's start view offers "New from an example" (team bevy-start). It needs bevy-ts to list its examples and make a project from one. Today there is no `bevy-ts new`, the examples live only in the repository, and their `tsconfig.json` and `Cargo.toml` point into the checkout, so a copy does not build outside it.

## What Changes

- The `bevy-ts` npm package ships the examples that run on the prebuilt engine: `cube` and `arena`. An example ships when its directory holds an `example.json` (id, title, description, entry).
- `bevy-ts examples [--json]` lists them, with a thumbnail when the example has one.
- `bevy-ts new <dir> [--example <id>] [--no-install] [--json]` makes a project from one: copies it, writes a `package.json` pinning bevy-ts at the running version, writes a `tsconfig.json` without checkout paths, and runs the install with the package manager that ran it.
- Both run in the Node launcher; neither starts the engine.
- `rapier` and `thrusters` do not ship: they need a project engine built from bevy-ts's crates, which are not on crates.io yet.

The output format was agreed with spec@bevy-start on 2026-10-22.

## Capabilities

### New Capabilities
- `example-projects`: the examples the npm package ships, how they are listed and how a project is made from one.

### Modified Capabilities

None.

## Impact

- `packages/bevy-ts`: new `bin/examples.js`, the launcher dispatches `examples` and `new`; `files` gains `examples`; a `prepack` script copies the shipped examples in. The package grows by about 6 KB unpacked (cube and arena source, levels and metadata; 2 KB gzipped). A thumbnail would add its PNG; none exist yet.
- `examples/cube`, `examples/arena`: an `example.json` each.
- `crates/bevy-ts-cli/src/cli.rs`: the engine's usage names `new` and `examples` as launcher commands; the binary run directly refuses them with a pointer to `npx bevy-ts`.
- README: a "Start" section with `npx bevy-ts new`.
- `bevy-ts new` runs `npm install` (or pnpm, yarn, bun) in the new directory by default: it reaches the npm registry. `--no-install` skips it.
- No RPC protocol change.
