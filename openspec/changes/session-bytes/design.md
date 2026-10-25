## Context

`SystemMemory` (`crates/bevy-ts/src/memory.rs`) is how `Local` rows survive a reload. It is a
plain Bevy `Resource` in the `World`, keyed by system identity. A reload never touches it. A
restart or a Stop replaces it with the default, in `generation.rs` (`apply_set`, the restart arm).
The JS isolate is not a safe home: a reload whose heap passed `HEAP_RESTART_BYTES` starts a new
isolate, and module state goes either way.

Exported params (docs/native.md, "Calling Rust from TS") already carry `&str` arguments and
`&[u8]` / `&mut [u8]` slices, which are read and written in place (`host.rs`, `u8 => "Uint8Array"`).
A result can be a number, `bool`, string or `Option` of these, not a byte array.
`bevy-ts/diagnostics` (`crates/bevy-ts-cli/src/diagnostics.rs`) shows the shape of a built-in module:
a plugin, exported params, and `ts_module` with a small `.js` and `.d.ts`.

## Goals / Non-Goals

**Goals:** a store of string keys to bytes that outlives every reload; a small, plain API; one
host call and one copy for each operation; Rust and TS share it.

**Non-Goals:** saving to disk; the protocol; listing or iterating keys; typed values; one store
for each module.

## Decisions

1. The store is a Bevy `Resource`, `bevy_ts::SessionBytes`, beside `SystemMemory`. A resource is
   Bevy's own idiom for state that belongs to the world and not to an entity. Ports read it with
   `Res`. It lives in the core crate so that a crate depending on `bevy-ts` can name it. Inside, it
   is a `HashMap<String, Box<[u8]>>` plus a running byte total. Lookup is O(1) at 10k keys.
   Rejected: a TS resource row (fixed layout, 64 KB arena); a `Local` (one per system, fixed
   layout); a JS `Map` kept across isolates (it dies with the heap restart).

2. Lifetime follows `Local`s exactly. Reloads, failed reloads and heap-restart reloads keep it.
   `restart: true` and Stop clear it, in the same `generation.rs` arm that resets `SystemMemory`.
   Stop runs that same arm (`rebuild` → `apply_set(.., true)`), so one line covers both. "Play,
   Stop, play runs the same" holds. Rejected: keeping it across Stop. Then a second Play would
   start with the last Play's selection, which is the leak docs/native.md tells crates to reset in
   `OnExit(EngineMode::Play)`. A game that wants an editor-side memory can ask for it later as its
   own change.

3. The TS surface is an exported param, `SessionBytes`, in a new built-in module `bevy-ts/session`
   (`crates/bevy-ts-cli/src/session.rs`, `ScriptSessionPlugin`, as `ScriptDiagnosticsPlugin` is).
   The param holds `ResMut<SessionBytes>`, so writes are in place and land when the call returns.
   Methods: `put(&mut self, key: &str, value: &[u8]) -> Result<(), String>`,
   `size(&self, key) -> Option<u32>`, `read(&self, key, out: &mut [u8]) -> Option<u32>`,
   `has`, `delete(&mut self, key) -> bool`, `clear(&mut self)`, `count() -> u32`, `bytes() -> f64`.
   Read-only methods do not reach `DerefMut`, so they mark no change. Rust's
   `Res<SessionBytes>::is_changed()` sees only `put`, a `delete` that removed something, and
   `clear`.

4. `get` is a function of the module, `get(session, key)`. It calls `size`, allocates a
   `Uint8Array` and calls `read`: two host calls. An export cannot return a byte array today, and
   adding one (or a way to add JS methods to a port class) is a new pattern on the export API. The
   hot path, many small keys each frame, uses `read` into one reused buffer and allocates nothing.

5. Limits: a key is 1 to 1,024 bytes of UTF-8. A call's string crosses through 4 KB of slots, so
   1 KiB leaves room, and an id-shaped key is far shorter. The store is at most 64 MiB of keys plus
   values. Heimdall's estimate is about 1 MB (10k ids at about 100 B each), plus small UI records.
   64 MiB is 60 times that, and it is still a bound that stops a runaway loop from eating memory.
   There is no limit for each value apart from the store's. A `put` checks both limits before it
   changes anything, and the error names the key (cut to 64 characters) and the sizes. The limits
   are `pub const`s in `bevy_ts::session`.

6. One store for each app. Per module would key on the module path, which a rename breaks (a
   reload would lose the memory the store exists to keep), and two modules could not share a
   selection. The docs suggest a prefix.

## Risks / Trade-offs

- `ResMut` in the param makes every system that takes `SessionBytes` a writer of one resource, so
  two of them cannot run in parallel. TS systems already run one at a time, so this costs nothing
  today. A native system that reads it waits for them, as it would for any `ResMut`.
- Holding 64 MiB is possible. It is in RAM until a restart, a Stop or the process exits. The limit
  is the guard.
- A copy on every `put` and `read`: 64 KB is about 5 µs of memcpy. The bench checks the call cost
  around it.

## Measure plan

`bench/session-bytes/` (outside the workspace, docs/testing.md rule 8): a project whose `Update`
system runs the arms with `performance.now()` around each and logs one JSON line for each arm, under
`bevy-ts dev --headless` on a release engine. A `bench.mjs` runs it and appends to
`results/<label>.jsonl`. Its process in `.abyme/processes.ts` builds through gate-slot.
- `put64k`, `get64k`, `read64k`: one 64 KB value, 1,000 iterations each, median and p95 for each call.
- `put10k`, `read10k`, `get10k`: 10k keys `"id/<n>"` of 64 B, one frame each, total ms.
- `frame`: the `Update` frame time with 10k keys stored and nothing called, against an empty store.
Pass: `read10k` under 2 ms a frame and `put64k` under 50 µs on the M-series laptop. A miss goes
back to the lead before landing.

## Open Questions

None.
