## Why

A generated accessor keeps its own state on the accessor object, under plain names: `t` (the row's tick column), `r` (the row), `k` (the run's tick), the typed views `f32`, `u8`, `u32` and the rest, and `_<field>` for a vector's or array's nested view. A field with one of those names collides: building the accessor runs the field's setter, and the field and the slot overwrite each other. A component field `t: Vec3`, or `Local<{ t: Timer }>`, breaks the system that uses it. Query rows (`present`) and observers' `On<E>` (`stopped`, `bind`) have the same kind of slot.

## What Changes

- **Every internal slot is keyed by a symbol** the runtime owns, so no field can name it: the row, the tick, the tick column, the typed views, the nested views, a query row's `present` and `On`'s `stopped` and `bind`. User field names are never refused or renamed.
- **Symbols, not private class fields:** code outside the generated class (row binding, `q.each`, a setup system's holders, which take an accessor's state by `Object.assign`) reaches the slots, and private fields would need a call per row.
- `On<E>` keeps `entity` and `propagate`, its documented API.

## Capabilities

### New Capabilities

None.

### Modified Capabilities

- `ts-components`: a generated accessor keeps its own state under keys no field can name, so a field may take any name.

## Impact

`crates/bevy-ts-v8/js/core.js` (and its generated copy `packages/bevy-ts/runtime/core.js`), a test in `crates/bevy-ts-cli/tests/systems.rs`; the bench used throwaway processes, not kept.
