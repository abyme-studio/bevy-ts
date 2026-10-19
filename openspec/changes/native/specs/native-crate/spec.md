## ADDED Requirements

### Requirement: A project has at most one native crate
A bevy-ts project SHALL be able to declare one optional native crate in `bevy-ts.json` (`"native": "native"`, a path to a Cargo package with a `lib` target). A project with no `native` entry SHALL have no Rust in it and SHALL run on the prebuilt engine from npm. The native crate MAY be a Cargo workspace member and MAY depend on any crates, third-party Bevy plugins included.

#### Scenario: A project without Rust
- **WHEN** `bevy-ts.json` has no `native` entry and the user runs `bevy-ts dev src/game.ts`
- **THEN** the prebuilt engine from the platform package runs, and no `cargo` process is started

#### Scenario: A project with a native crate
- **WHEN** `bevy-ts.json` says `"native": "native"` and `native/Cargo.toml` is a library package
- **THEN** `bevy-ts dev` runs a project engine built from it (see project-engine)

### Requirement: One entry registers everything
The native crate SHALL register itself with exactly one `bevy_ts::native!(fn)` invocation, where `fn(&mut NativeApp)` receives a wrapper over Bevy's `App`. Through it the crate SHALL be able to add Bevy plugins and systems, register component types for TS (see native-components), expose system sets (see native-ordering) and register native functions (see native-functions). A crate with zero or two `native!` invocations SHALL fail the project-engine build with an error naming the crate.

#### Scenario: Adding a third-party plugin
- **WHEN** the entry calls `app.add_plugins(avian3d::PhysicsPlugins::default())`
- **THEN** the project engine runs avian's systems in the same world as the TS systems

#### Scenario: Two entries
- **WHEN** the native crate contains `native!` twice
- **THEN** the build fails naming the crate and both locations

### Requirement: The native crate pins bevy-ts and Bevy exactly
The native crate SHALL depend on the `bevy-ts-native` crate at the exact version of the installed `bevy-ts` npm package (`=x.y.z`), and on `bevy` only through `bevy_ts_native::bevy` or at the exact Bevy version that release was built with. Before building, bevy-ts SHALL compare the resolved versions in `Cargo.lock` with its own and SHALL refuse to build on a mismatch, naming the crate, the expected and the found version.

#### Scenario: npm upgraded, Cargo not
- **WHEN** the npm package is 0.4.0 and `Cargo.lock` resolves `bevy-ts-native` 0.3.2
- **THEN** the build stops with an error naming `bevy-ts-native`, expected `=0.4.0`, found 0.3.2, before `cargo` compiles anything

### Requirement: Native code stays out of gameplay reloads
A TS or TSX edit SHALL never rebuild, relink or restart the engine, with or without a native crate. Everything the native crate registers SHALL be fixed for the life of the engine process; a TS reload SHALL see the same Rust components, sets and functions as the previous generation.

#### Scenario: Editing gameplay in a project with physics
- **WHEN** a project whose native crate adds avian saves a change to `src/game.ts`
- **THEN** the generation reloads in the running engine as in a project without Rust, and no `cargo` process runs
