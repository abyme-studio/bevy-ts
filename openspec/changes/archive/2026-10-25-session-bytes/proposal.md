## Why

A reload keeps only ECS state (hot-reload, "ECS state survives a reload"); module-level JS state
starts again, and a reload whose heap passed its limit starts a new isolate anyway. Agent town's
Heimdall design keeps a session tier there today (selection, hover, camera pose, open panel, and a
small record for each of about 10k agent ids), so every save loses it. Components and `Local`s are
fixed layouts, not a store keyed by id, and a resource row is far too small for 10k records.

## What Changes

- A Rust resource `bevy_ts::SessionBytes`: string keys to byte values, held in the `World` beside
  `SystemMemory`, never in the JS heap. It survives every reload, failed ones too. A
  `restart: true` reload and a Stop clear it, as they clear `Local`s.
- A built-in module `bevy-ts/session`, which every engine ships. It exports the system param
  `SessionBytes`, a writable native param (docs/native.md, "Params that write") over
  `ResMut<SessionBytes>`:
  `put(key, value)`, `size(key)`, `read(key, out)`, `has(key)`, `delete(key)`, `clear()`,
  `count()`, `bytes()`. Values cross as `Uint8Array`, so they are copied in place in one host call.
  The module also exports `get(session, key)`, which returns a new `Uint8Array` or `undefined`.
- Limits: a key is 1 to 1,024 bytes of UTF-8. The whole store (keys and values) is at most 64 MiB.
  A `put` past either limit throws, naming the key and the sizes, and changes nothing.
- One store for each app, not one for each module. Keys are global; the docs suggest a prefix
  (`"heimdall/selection"`).
- Rust reads and writes the same store as `Res<SessionBytes>` / `ResMut<SessionBytes>`.
- Docs: a "Session bytes" section in `docs/scripts.md`. hot-reload's survival rule names the store.
- A bench in `bench/session-bytes/`: put and get of one 64 KB value, and of 10k small keys.

Out of scope: writing the store to disk, showing it on the protocol, listing keys, typed values
(scripts encode their own bytes).

## Capabilities

### New Capabilities
- `session-bytes`: the store, its TS param and module, its limits, its lifetime, and Rust access.

### Modified Capabilities
- `hot-reload`: "ECS state survives a reload" names session bytes as the one other state that
  survives. The restart rule clears them.

## Impact

- `crates/bevy-ts/src/session.rs` (new): the `SessionBytes` resource, its limits, and its errors.
  `lib.rs` inits it and re-exports it. `generation.rs` resets it on a restart, where it resets
  `SystemMemory`.
- `crates/bevy-ts-cli/src/session.rs` (new): `ScriptSessionPlugin`, the exported param, and the
  module. `crates/bevy-ts-cli/ts/session.{js,d.ts}` (new). Added in `cli.rs` and `types.rs`
  beside `ScriptDiagnosticsPlugin`.
- `crates/bevy-ts-cli/tests/it/session.rs` (new) and a `mod session;` line in `tests/it/main.rs`.
- `bench/session-bytes/` (new), outside the workspace. Its process goes in `.abyme/processes.ts`.
- `docs/scripts.md`.
- No protocol change and no new dependency.
- Operator go: none needed, on the checks below. The lead routes it if they read one another way.
  - Pattern: a built-in module over an exported param, as `bevy-ts/diagnostics` is. `get` is a
    plain module function, not a new way to add methods to a param.
  - Performance: nothing runs unless a script calls it. Each call is one host call and one copy.
    The bench measures it. Memory is at most 64 MiB for each app.
  - Data: in memory only, inside the engine process. Never on disk or on the wire.
