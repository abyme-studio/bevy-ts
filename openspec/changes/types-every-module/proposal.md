## Why

`bevy-ts types` builds its own app, which listed its plugins by hand and missed
`ScriptReadyPlugin`: `bevy-ts/ready` is importable in a run but not declared, so a game had to
write its own `bevy-ts-ready.d.ts`. `bevy-ts build` checks imports against the same app, so it
refused `import "bevy-ts/ready"` too.

## What Changes

- `ScriptPlugins`, a Bevy `PluginGroup` in `crates/bevy-ts-cli/src/cli.rs`: every plugin that
  ships a built-in module. The run's app and `bevy-ts types` both add it, so the two lists cannot
  differ.
- A test builds the run's app in a child process and fails when `bevy-ts types` does not declare a
  module it registered.

## Capabilities

### Modified Capabilities

- `native-functions`: `bevy-ts types` declares every built-in module the run's engine registers.

## Impact

`crates/bevy-ts-cli/src/{cli.rs,types.rs}`, `crates/bevy-ts-cli/tests/it/types_modules.rs`,
`packages/bevy-ts/test/engine` (regenerated), `docs/native.md`.
