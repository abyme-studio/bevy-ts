## ADDED Requirements

### Requirement: A project engine is the default engine plus the native crate, statically linked
For a project with a native crate, adopted crates or ports, bevy-ts SHALL generate a binary crate under `.bevy-ts/engine/` whose `main` calls the same engine entry the prebuilt binary uses (`bevy_ts_cli::run`) with their Bevy plugins passed in, and SHALL build it with `cargo build` in the profile `bevy-ts dev` (dev, with dependencies optimised) or `bevy-ts build` (release) asks for. The native crate SHALL be linked statically into one executable; bevy-ts SHALL NOT load Rust code from a dynamic library at runtime.

#### Scenario: Building a project engine
- **WHEN** `bevy-ts dev` runs in a project with a native crate and no cached engine
- **THEN** it generates `.bevy-ts/engine/`, runs `cargo build`, shows cargo's progress, and starts the resulting binary with the same flags the prebuilt engine takes

#### Scenario: No toolchain
- **WHEN** a project engine must be built and `cargo` is not on `PATH`, and no shared cache has it
- **THEN** `bevy-ts dev` stops with an error saying a Rust toolchain or a shared cache entry is needed, and names the engine hash

### Requirement: Engines are cached by a content hash of the Rust inputs
bevy-ts SHALL compute an engine hash from: the bevy-ts version, the target triple, the build profile, the enabled engine features, `Cargo.lock`, every `Cargo.toml` in the native crate's workspace, and the contents of every file under the native crate's package directories that Cargo would package (`.rs` and `build.rs` inputs included). TS sources, assets and `bevy-ts.json` fields other than `native` and `features` SHALL NOT enter the hash. A binary for a hash that is in the local cache (`.bevy-ts/engines/<hash>/`) SHALL be reused without running `cargo`.

#### Scenario: A TS-only change
- **WHEN** only `src/*.ts` and assets changed since the last run
- **THEN** the engine hash is unchanged and the cached binary starts with no `cargo` invocation

#### Scenario: A Rust change
- **WHEN** one line in `native/src/lib.rs` changed
- **THEN** the engine hash changes and a new project engine is built

### Requirement: A shared engine cache
`bevy-ts.json` MAY name a shared engine cache, a directory laid out as `<hash>/<target>/` (a network share, or a CI artefact a team unpacks). On a local miss bevy-ts SHALL try the shared cache before building, SHALL verify a binary against the SHA-256 recorded next to it, and SHALL NOT execute a binary that fails verification. The checksum proves integrity, not authenticity; the docs SHALL say so. An HTTP cache and a publish command are out of scope for this change. Before any build, `bevy-ts dev` SHALL print the engine hash, why the caches missed and what will compile.

#### Scenario: A gameplay author without Rust
- **WHEN** CI has placed the engine for the current `Cargo.lock` and native sources in the shared directory, and an author with no Rust toolchain runs `bevy-ts dev`
- **THEN** the engine is fetched from the shared cache, verified and started, and no build is attempted

#### Scenario: A tampered download
- **WHEN** the fetched binary's SHA-256 does not match its recorded checksum
- **THEN** bevy-ts deletes it, reports the mismatch and does not run it

### Requirement: Rust edits under `bevy-ts dev` rebuild and restart
When a native crate is present, `bevy-ts dev` SHALL watch the files that enter the engine hash. On a change it SHALL rebuild the engine in the background while the old engine keeps running; on success it SHALL stop the old engine and start the new one with the same entry, flags and protocol port, then load the current TS generation as a restart (bridge hot-reload rule 4). On failure it SHALL report cargo's errors and keep the old engine running.

#### Scenario: A compile error in Rust
- **WHEN** a save in `native/src/lib.rs` fails to compile
- **THEN** the running game continues on the old engine and the terminal shows cargo's error

#### Scenario: A successful Rust edit
- **WHEN** a save in `native/src/lib.rs` compiles
- **THEN** the game restarts on the new engine within the build time plus engine startup, and abyme's studio reconnects on the same port after `bevy-ts/hello` reports the new engine hash

### Requirement: The engine reports what it is
`bevy-ts/hello` SHALL report the engine hash, whether the engine is prebuilt or a project engine, and the native crate's name and version if there is one.

#### Scenario: Handshake on a project engine
- **WHEN** a client calls `bevy-ts/hello` on a project engine
- **THEN** the result carries `engine: { kind: "project", hash, native: { name, version } }`
