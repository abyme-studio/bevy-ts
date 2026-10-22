## Why

abyme's stream sends each touch with `t`, ms on the viewer device's own clock (abyme `crates/app/SURFACE.md`, input table, touch row). `TouchEvent` refuses unknown fields and has no `t`, so every phone touch is refused as malformed.

The shape was agreed with research@game-view-anywhere (abyme), 2026-10-22.

## What Changes

- `TouchEvent` gains optional `t: number`: ms on the viewer's clock, fractional, only compared within one stream. Near-rising: a move sent late keeps its own time. The engine accepts it and ignores it; pacing touches by `t` is out of scope.
- Protocol 1.20.0, additions only.

## Capabilities

### New Capabilities
None.

### Modified Capabilities
- `viewport-input`: `touch` carries an optional `t`.

## Impact

- `protocol/bevy-ts.openrpc.json` and its generated Rust and TS types (`scripts/gen-protocol.sh`), version bump.
- `crates/bevy-ts-viewport/src/input.rs`: a test that a touch with `t` and one without are accepted.
- Operator go: none needed. A protocol minor, additions only.
