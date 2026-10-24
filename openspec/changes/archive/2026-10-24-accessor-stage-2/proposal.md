## Why

Two follow-ups to accessor-slot-names. A setup system's `Res`/`ResMut`/`Local` holder copies the accessor it was built from, but its nested vector views still point at that accessor, which is never bound: `h.t.x` reads nothing. And `On<E>` derefs to the event's fields beside its own `entity`, `propagate` and `toJSON`, so an event field with one of those names is shadowed silently.

## What Changes

- A setup holder's nested views follow the holder that gets bound.
- A TS event field named `entity` (on an `Event`), `propagate` or `toJSON` fails the load at the field: "field X shadows On<E>.X; rename it" (ruling, lead@bevy-ts). An `EntityEvent`'s `entity` stays its target. `On<E>` keeps Bevy's shape.
- Out of scope: a Rust event field by those names, which rust-events settles (noted in `docs/research/bevy-coverage.md`).

## Capabilities

### New Capabilities

None.

### Modified Capabilities

- `ts-systems`: three field names are reserved on events.

## Impact

`crates/bevy-ts-v8/js/core.js` (and its generated copy), `crates/bevy-ts-cli/src/reader.rs`, `packages/bevy-ts/index.d.ts`, tests in `crates/bevy-ts-cli/tests/it/surface.rs`.
