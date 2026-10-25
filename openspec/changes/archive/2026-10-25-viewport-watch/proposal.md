## Why

Watch state was kept per (method, params), not per subscriber. A second subscriber with the same params as an open one (two studio windows) got no opening message and only some of the deltas. Every `+watch` method had its own copy of the bug: viewport mode, selection, tools, edits and assets, entities, get_components, log, reload, tsx_tree and level.

After a Stop the studio's "Spawned at runtime" listed the viewport's window and bevy_picking's pointer entities, because `bevy-ts/entities` lists them.

## What Changes

- One owner, `bevy_ts_remote::Subscribers`: per-subscriber cursors for every watching method. Bevy runs each open request once per update with only its params, from one list of every open watch: new requests are pushed on its end, closed ones `swap_remove`d. So the `n` subscribers served the update before are the first `n` calls of this update, and a call after them is a new subscriber. Each new subscriber gets its opening message, then every delta. It counts updates itself, so a Stop resetting `ScriptFrame` does not reset the watches.
- Every watching method uses it; the per-crate `Watchers` maps go.
- `bevy-ts/entities` and its watch leave out entities with `Window`, `Monitor`, `Gamepad` or `PointerId`, as they leave out observers. Not `ScriptHidden`: that would hide them from TS queries, and a game reads its window.

## Capabilities

### Modified Capabilities

- `remote-protocol`: every subscriber of a watch is served on its own; the entity list leaves out the engine's windows and pointers.

## Impact

- Code: `crates/bevy-ts-remote/src/{lib,entities,values}.rs`, `crates/bevy-ts-viewport/src/{methods,assets}.rs`, `crates/bevy-ts-tsx/src/protocol.rs`, `crates/bevy-ts-levels/src/protocol.rs`.
- Tests: `Subscribers` unit tests (different cursors, a close); two same-params tools subscribers (`bevy-ts-viewport/tests/edit.rs`); a second assets window opening as a file lands (`bevy-ts-viewport/tests/it/assets.rs`); engine entities after Stop and while the entity watch is open (`bevy-ts-cli/tests/it/play_stop.rs`). Tests that drained `level+watch` with many calls in one update now call it once per update, as Bevy does.
- Protocol: no schema change.
- `assets+watch`: a new subscriber's opening poll numbers its changes for the subscribers already open, instead of starting the listing over.
- Limit: after a close the survivors can swap slots, and one can take another's cursor. A slot is served the lowest cursor among it and the slots after it, so a subscriber can be sent a message again, never miss one. Streams that send everything since the cursor catch every subscriber up each update, so a swap changes nothing for them; `level+watch`, one event per update, is the exception and can repeat events after a close. One that opens in the update another with the same params closes gets no opening message.
