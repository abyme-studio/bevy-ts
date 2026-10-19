# script-runtime Specification

## Purpose
TBD - created by archiving change bridge. Update Purpose after archive.

## Requirements

### Requirement: Rust owns the only world
bevy-ts SHALL keep all entities, components and resources in Bevy's single `World`. The JS runtime SHALL hold no copy of ECS state that outlives one system run; anything JS keeps between frames is module state, which a reload discards.

#### Scenario: JS sees state only through a run
- **WHEN** a TS system keeps a reference to a column view after its `run` returns
- **THEN** the view is detached and reading it yields length 0, never stale or freed memory

### Requirement: Engine-neutral seam
The core crate (`bevy-ts`) SHALL depend on no JS engine. It SHALL drive a backend only through one trait whose operations are: load a set of compiled modules as a new generation and return its declarations and manifest sections, run one declared system over a frame of column buffers and get back a command buffer, run a startup function, call a named entry with a byte payload and get back bytes and a command buffer (see plugin-api), and pump pending engine work (timers, inspector messages). The V8 backend (`bevy-ts-v8`, `deno_core` 0.412) SHALL be the only backend in this change.

#### Scenario: The core builds without V8
- **WHEN** `bevy-ts` is built with no backend crate
- **THEN** it compiles, and its tests run against an in-process fake backend that implements the trait

#### Scenario: A backend sees buffers, not the world
- **WHEN** a backend runs a system
- **THEN** it receives only pointers, lengths, layouts and a command arena, and has no access to `World`

### Requirement: JS runs on the main thread inside exclusive stage systems
The isolate SHALL live on the thread that runs the main app's schedules and SHALL be stored as non-send data. TS systems SHALL run only inside bevy-ts's exclusive stage systems; no JS SHALL run concurrently with Rust systems of the main app. Remote protocol handlers that need the runtime SHALL run in `RemoteLast`, also on the main thread.

#### Scenario: Rust systems around a JS stage
- **WHEN** a schedule contains Rust systems ordered before and after the bevy-ts stage set
- **THEN** those Rust systems still run in parallel with each other, and none overlaps a JS system

### Requirement: Host-side TypeScript compilation
TS and TSX sources SHALL be compiled to JS by the host process with `oxc` (type stripping only, no type checking), producing a source map per module. `.tsx` SHALL compile with the automatic JSX runtime and `importSource: "bevy-ts"`; under `bevy-ts dev` it SHALL use the development transform, so each element carries its file, line and column in TS source coordinates. The engine SHALL load modules through its own module loader: relative imports and the built-in modules `bevy-ts`, `bevy-ts/bevy`, `bevy-ts/jsx-runtime` and `bevy-ts/jsx-dev-runtime` are resolved; bare npm specifiers SHALL fail with an error naming the specifier. This change ships the two JSX runtime modules as the resolution targets only; what they do belongs to a later change.

#### Scenario: A TSX element in dev
- **WHEN** `game.tsx` contains `<entity name="a" />` on line 7, column 3, and loads under `bevy-ts dev`
- **THEN** the compiled call imports from `bevy-ts/jsx-dev-runtime` and carries `game.tsx`, line 7, column 3

#### Scenario: A type error does not block a run
- **WHEN** `game.ts` contains a type error but valid syntax
- **THEN** it loads and runs; type checking is the editor's and `tsc`'s job

#### Scenario: An unsupported import
- **WHEN** a module imports `lodash`
- **THEN** loading fails with an error naming `lodash` and the importing file and line

### Requirement: JS errors are reported with TS locations
An exception thrown by a TS system, startup function or module evaluation SHALL be logged with a stack mapped to TS file, line and column, and SHALL be published on the remote protocol's log stream. A system that throws SHALL have the commands it queued in that run discarded; it SHALL run again next frame. Repeated identical errors SHALL be logged once per second with a count.

#### Scenario: A system throws every frame
- **WHEN** a system throws the same error every frame for 3 seconds
- **THEN** the log shows about 3 lines, each with the TS location and a repeat count, and other systems keep running

### Requirement: Debugging over the V8 inspector
In `bevy-ts dev`, the V8 backend SHALL serve the Chrome DevTools Protocol on a local port, so Chrome DevTools and VS Code can set breakpoints in TS sources through source maps. While paused at a breakpoint the game's main loop SHALL be blocked.

#### Scenario: A breakpoint in a system
- **WHEN** a debugger attached over CDP sets a breakpoint inside a system's `run`
- **THEN** the game freezes on that line in the TS source, and resumes when the debugger continues

### Requirement: Zero-copy depends on the V8 sandbox being off
The V8 backend SHALL build V8 with its sandbox disabled (the `v8` crate's default) and SHALL fail to compile if `v8_enable_sandbox` is enabled, because column views wrap memory outside any sandbox.

#### Scenario: Someone enables the sandbox
- **WHEN** `bevy-ts-v8` is built with the `v8_enable_sandbox` feature
- **THEN** the build fails with a message pointing at the bridge design
