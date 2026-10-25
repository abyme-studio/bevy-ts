# session-bytes Specification

## Purpose
Session bytes: a Rust-held store of string keys to bytes that survives every reload, its `bevy-ts/session` param and `get`, its limits, its lifetime, and Rust access.

## Requirements

### Requirement: Session bytes are a Rust resource keyed by string
bevy-ts SHALL hold one `SessionBytes` resource in each app's `World`. It maps string keys to byte
values and is held in Rust, never in the JS heap. Rust SHALL read it as `Res<SessionBytes>` and
write it as `ResMut<SessionBytes>`. TS SHALL reach the same store, so a value Rust puts is the
value TS reads.

#### Scenario: Rust puts, TS reads
- **WHEN** a Rust system puts `[1, 2, 3]` under `"rust/seed"` and a later TS system calls `get(session, "rust/seed")`
- **THEN** it returns a `Uint8Array` of `[1, 2, 3]`

### Requirement: Scripts use the SessionBytes param from bevy-ts/session
Every engine SHALL ship the module `bevy-ts/session`. It SHALL export the system param
`SessionBytes`, a writable exported param over `ResMut<SessionBytes>`, with these methods:
- `put(key: string, value: Uint8Array)` stores a copy of `value`, replacing any value under `key`;
- `size(key: string): number | undefined` is the stored value's length, or `undefined` when there is none;
- `read(key: string, out: Uint8Array): number | undefined` copies the value into the start of `out`
  when it fits and returns its length. When `out` is shorter, it copies nothing and returns the
  length. It returns `undefined` when there is no value;
- `has(key: string): boolean`;
- `delete(key: string): boolean` removes the value and says whether one was there;
- `clear()` removes every value;
- `count(): number` is the number of keys, and `bytes(): number` the bytes the store holds.

The module SHALL also export `get(session: SessionBytes, key: string): Uint8Array | undefined`,
which returns a new array holding a copy of the value. A value argument that is not a `Uint8Array`
SHALL throw before the call crosses, as every exported slice argument does.

#### Scenario: Put then get in one run
- **WHEN** a system calls `session.put("a", new Uint8Array([7, 8]))` and then `get(session, "a")`
- **THEN** `get` returns `[7, 8]`, and changing the array it returns does not change the store

#### Scenario: Reading into a short buffer
- **WHEN** `"a"` holds 10 bytes and a system calls `session.read("a", new Uint8Array(4))`
- **THEN** it returns 10 and the 4-byte array is unchanged

#### Scenario: Delete and clear
- **WHEN** a system puts `"a"` and `"b"`, deletes `"a"`, and checks `has("a")` and `count()`, then calls `clear()`
- **THEN** `delete` returns `true`, `has("a")` is `false`, `count()` is 1, and after `clear()` it is 0

#### Scenario: Not a Uint8Array
- **WHEN** a system calls `session.put("a", new Float32Array(2))`
- **THEN** it throws naming `SessionBytes.put`, `Uint8Array` and `Float32Array`, and nothing is stored

### Requirement: Session bytes have a key limit and a store limit
A key SHALL be 1 to 1,024 bytes of UTF-8. The store SHALL hold at most 64 MiB (67,108,864 bytes),
counting each key's bytes and each value's bytes. A `put` that would break either limit SHALL throw,
naming the method, the key (cut to 64 characters) and the sizes, and SHALL leave the store as it
was, the old value under that key included. A replacing `put` is counted without the value it
replaces.

#### Scenario: A store that would overflow
- **WHEN** the store holds 60 MiB and a system puts a 5 MiB value under a new key
- **THEN** it throws naming the key, 5 MiB and the 64 MiB limit, and `count()` and `bytes()` are unchanged

#### Scenario: An empty key
- **WHEN** a system calls `session.put("", new Uint8Array(1))`
- **THEN** it throws saying a key is 1 to 1,024 bytes

### Requirement: Session bytes survive reloads and clear on restart and Stop
The store SHALL keep every key and value across every soft reload: a reload that replaces the
isolate because its heap passed its limit, and a failed reload, included. A `bevy-ts/reload` with
`restart: true` and a Stop (edit-mode) SHALL empty it, as they reset `Local` storage. The store
SHALL NOT be written to disk; it ends with the engine process.

#### Scenario: Put, reload, get
- **WHEN** a `Startup` system puts 64 KB under `"heimdall/memory"`, `game.ts` is saved with a changed `Update` system that reads it, and the reload applies
- **THEN** that `Update` system's `get(session, "heimdall/memory")` returns the same 64 KB

#### Scenario: A failed reload
- **WHEN** a value is stored and `game.ts` is saved with a syntax error
- **THEN** the value is still there, and still there after the next good save

#### Scenario: A restart
- **WHEN** a value is stored and a client calls `bevy-ts/reload` with `restart: true`
- **THEN** `count()` is 0 in the new generation

### Requirement: One store for each app
There SHALL be one store for each `World`, shared by every module and system. Keys SHALL NOT be
scoped to the module that wrote them: a key one module puts, another module reads.

#### Scenario: Two modules share a key
- **WHEN** `ui.ts` puts `"heimdall/selection"` and a system in `camera.ts` reads it later in the frame
- **THEN** it reads the bytes `ui.ts` put
