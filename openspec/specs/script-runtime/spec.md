# script-runtime Specification

## Purpose
The embedded JS runtime, the engine-neutral seam, the thread it runs on, how TS is compiled (including reading system signatures) and loaded, errors and debugging.

## Requirements

### Requirement: Rust owns the only world
bevy-ts SHALL keep all entities, components and resources in Bevy's single `World`. The JS runtime SHALL hold no copy of ECS state that outlives one system run; anything JS keeps between frames is module state, which a reload discards.

#### Scenario: JS sees state only through a run
- **WHEN** a TS system keeps a reference to a column view after its `run` returns
- **THEN** the view is detached and reading it yields length 0, never stale or freed memory

### Requirement: Engine-neutral seam
The core crate (`bevy-ts`) SHALL depend on no JS engine. It SHALL drive a backend only through one trait whose operations are: load a set of compiled modules as a new generation, with the environment `process.env` shows, and return its declarations and manifest sections, run one declared system over a frame of column buffers and get back a command buffer, run a startup function, call a named entry with a byte payload and get back bytes and a command buffer (see plugin-api), deliver one frame's socket events and the current real time and get back the network requests scripts made and the next timer deadline (see script-networking), and pump pending engine work (microtasks, inspector messages). Network requests and events SHALL cross the seam as plain data tagged with a connection id and a generation; the backend SHALL hold no socket. The V8 backend (`bevy-ts-v8`, `deno_core` 0.412) SHALL be the only backend in this change.

#### Scenario: The core builds without V8
- **WHEN** `bevy-ts` is built with no backend crate
- **THEN** it compiles, and its tests run against an in-process fake backend that implements the trait

#### Scenario: A backend sees buffers, not the world
- **WHEN** a backend runs a system
- **THEN** it receives only pointers, lengths, layouts and a command arena, and has no access to `World`

#### Scenario: A backend sees network data, not sockets
- **WHEN** a script sends on a `WebSocket`
- **THEN** the backend hands core a send request with the connection id, generation and bytes, and core's transport writes it

### Requirement: JS runs on the main thread inside exclusive stage systems
The isolate SHALL live on the thread that runs the main app's schedules and SHALL be stored as non-send data. TS systems SHALL run only inside bevy-ts's exclusive stage systems; no JS SHALL run concurrently with Rust systems of the main app. Remote protocol handlers that need the runtime SHALL run in `RemoteLast`, also on the main thread.

#### Scenario: Rust systems around a JS stage
- **WHEN** a schedule contains Rust systems ordered before and after the bevy-ts stage set
- **THEN** those Rust systems still run in parallel with each other, and none overlaps a JS system

### Requirement: Host-side TypeScript compilation
TS and TSX sources SHALL be compiled to JS by the host process with `oxc` (type stripping only, no type checking), producing a source map per module. `.tsx` SHALL compile with the automatic JSX runtime and `importSource: "bevy-ts"`; under `bevy-ts dev` it SHALL use the development transform, so each element carries its file, line and column in TS source coordinates. The engine SHALL load modules through its own module loader: relative imports and the built-in modules `bevy-ts`, `bevy-ts/bevy`, `bevy-ts/jsx-runtime` and `bevy-ts/jsx-dev-runtime` are resolved, and bare npm specifiers are resolved at compile time (see "npm packages resolve at compile time"). This change ships the two JSX runtime modules as the resolution targets only; what they do belongs to a later change. A relative import of a `.wgsl` file SHALL resolve to a synthetic module whose default export is an opaque shader-file value naming the file; the loader SHALL read the file host-side, not compile it, and watch it with the other loaded files. A missing `.wgsl` file SHALL fail the load with the importing file and line.

#### Scenario: A TSX element in dev
- **WHEN** `game.tsx` contains `<entity name="a" />` on line 7, column 3, and loads under `bevy-ts dev`
- **THEN** the compiled call imports from `bevy-ts/jsx-dev-runtime` and carries `game.tsx`, line 7, column 3

#### Scenario: A type error does not block a run
- **WHEN** `game.ts` contains a type error but valid syntax
- **THEN** it loads and runs; type checking is the editor's and `tsc`'s job

#### Scenario: A package import
- **WHEN** `src/game.ts` imports `{ connect } from "@abyme/client/connect"` and the project's `node_modules/@abyme/client` is a pnpm link whose `exports` map `./connect` to `./src/connect.ts`
- **THEN** the load succeeds, the compiled `src/game.ts` imports `connect.ts` by a relative path to its real file, and `connect.ts` is compiled with its types stripped

#### Scenario: An unsupported import
- **WHEN** a module imports `lodash` and no `node_modules` above it holds `lodash`
- **THEN** loading fails with an error naming `lodash` and the importing file and line, saying the package is not installed

#### Scenario: A shader file import
- **WHEN** `src/game.ts` writes `import glow from "./glow.wgsl"` and `src/glow.wgsl` exists
- **THEN** `glow` is a shader-file value naming `src/glow.wgsl`, the file is watched, and its WGSL is never run through `oxc`

#### Scenario: A missing shader file
- **WHEN** `src/game.ts` imports `./glw.wgsl` on line 3 and no such file exists
- **THEN** loading fails with an error naming `./glw.wgsl`, `src/game.ts` and line 3

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

### Requirement: System signatures and data classes are read from syntax
The compile step SHALL read, from syntax alone and with no type checker (operator rulings 2026-10-20), the fields of every exported class extending `Component`, `ReflectComponent`, `Resource`, `Message`, `Event` or `EntityEvent` (each field's name, written type and initialiser, and a `static path`), and the parameter annotations of every exported function a module registers as a system, observer or plugin, and SHALL attach the declarations the engine needs (each param's kind, its query data and filters) to the compiled function. It SHALL follow, by name: type aliases including generic ones (by substituting the arguments as written), interfaces and object types used as param bundles, imports between project modules, and param kinds that installed ports declare. It SHALL fail the load with the file and line for a param type it cannot read this way, such as a computed type (`ReturnType<...>`, `Pick<...>`, a conditional type); the system then uses the setup-closure form (ts-systems). The pass SHALL change no function body, so source maps and breakpoints are unaffected. bevy-ts SHALL ship the same pass as plugins for vitest, Vite and esbuild, so systems can be tested outside the engine.

#### Scenario: An alias from another file
- **WHEN** `enemies.ts` exports `type Movers<T> = Query<[T, Mut<Transform>], With<Alive>>` and `game.ts` exports `function move(q: Movers<Enemy>)`
- **THEN** `move` is declared as a query of `Enemy` read and `Transform` written, filtered by `Alive`

#### Scenario: A computed param type
- **WHEN** a system's param is annotated `Pick<Bundle, "a">`
- **THEN** loading fails with an error naming `Pick`, the file and line

### Requirement: Declarations are appended, never woven in
The compile step SHALL attach a module's system declarations by appending statements after the module's code, so every line of the original keeps its number. A component, resource, message, event or state named in a param SHALL be bound through the module whose source text names it. A name imported with `import type` that a param needs at run time SHALL fail the compile with its file and line.

#### Scenario: A type-only import
- **WHEN** `game.ts` writes `import type { Enemy } from "./enemy"` and exports `function chase(q: Query<[Enemy]>)`
- **THEN** the compile fails at that import's line with "Enemy is imported as a type; system params need its value"

### Requirement: The transform command
The `bevy-ts` binary SHALL offer `bevy-ts transform <file>`, printing the compiled module with its declarations, and the npm package SHALL ship Vite, vitest and esbuild plugins that call it, so a module compiled by those tools declares its systems as under `bevy-ts dev`.

#### Scenario: A system in a vitest test
- **WHEN** a vitest test imports a module through the bevy-ts plugin and registers its system with the `bevy-ts/testing` app
- **THEN** the system's declarations are present and the test can call it with hand-built params

### Requirement: Data classes declare themselves when defined
The compile step SHALL give every class it reads as a component, scene component, resource, message, event, state machine or system set a static block, with no source mapping, that hands the runtime the class's name, fields and scene props, so the class carries its resolved name (`componentName`), `source`, `fieldNames` and, for a scene component, `sceneProps` from the moment it is defined. A field the compile step cannot type SHALL fail the load when the class is defined, with the field's file and line. The runtime SHALL keep the generation's scene components by name and by `module#export`.

#### Scenario: A scene reads a component at module load
- **WHEN** `game.tsx` declares `class Health extends Component { hp: f32 = 100 }` and, later in the same module, a scene uses `<Health hp={3} />`
- **THEN** the element is keyed by `game::Health` while the module evaluates, before the generation commits

### Requirement: npm packages resolve at compile time
The compile step SHALL resolve every import that is not a built-in or registered `bevy-ts` module with one Node-style resolver: relative paths, and bare specifiers through `node_modules` folders walked up from the importing file, a package's `exports` and `imports` maps, and `main` when it has no `exports`. It SHALL follow symlinks to real paths, so a pnpm or workspace link loads the linked package's files. It SHALL rewrite each import's specifier, in the compiled code, to a relative path to the resolved file, so the module loader and a shipped game resolve only relative imports. The rewrite SHALL leave every line and column of the source map unchanged. Export conditions SHALL be `bevy-ts`, `import`, `module` and `default`, plus `development` under `bevy-ts dev` and `production` under `bevy-ts build`; `node`, `browser`, `require` and `types` SHALL NOT be used, because scripts are not Node, have no DOM, load ES modules only and never load declaration files. A package's `.ts` and `.tsx` files SHALL compile as project files do. A relative import in a package MAY leave the package. tsconfig `paths` SHALL NOT be read; built-in and registered modules SHALL be matched before the resolver runs.

#### Scenario: A condition for bevy-ts
- **WHEN** a package's `exports` map `"."` to `{ "node": "./node.js", "bevy-ts": "./bevy.js", "default": "./index.js" }`
- **THEN** the game loads `bevy.js`

#### Scenario: Development and production builds
- **WHEN** a package's `exports` map `"."` to `{ "development": "./dev.js", "production": "./prod.js" }`
- **THEN** `bevy-ts dev` loads `dev.js` and `bevy-ts build` ships `prod.js`

#### Scenario: A package reaches outside itself
- **WHEN** `@abyme/client`'s `src/connect.ts` imports `../../kernel/src/protocol.ts`, outside the package folder
- **THEN** the import resolves to that file's real path and it is compiled into the graph

#### Scenario: A subpath the package does not export
- **WHEN** `src/game.ts` imports `@abyme/client/src/internal.ts` and the package's `exports` do not offer that subpath
- **THEN** loading fails with an error naming the specifier, the package and the importing file and line

### Requirement: Scripts load ES modules only
The compile step SHALL refuse, each with an error naming the importing file and line: a CommonJS module (a `.cjs` file, or a `.js` file whose nearest `package.json` does not say `"type": "module"` and that has no `import` or `export`), naming the package and file and saying bevy-ts loads ES modules only; a Node built-in, written `node:<name>` or bare (`fs`, `path`), naming the built-in and saying scripts have no Node APIs; and an import of a `.json` file. A dynamic `import()` SHALL NOT be resolved at compile time: at run time the loader SHALL find its target only in the compiled graph and SHALL reject, with an error the script can catch, when the target is not there, a bare specifier or a JSON import attribute included. A module's `import.meta.url` SHALL be its file's URL: the real path under `bevy-ts dev`, and the shipped file under `game/scripts/` in a built game.

#### Scenario: A CommonJS package
- **WHEN** `src/game.ts` imports `left-pad`, whose `package.json` has no `type` and whose `index.js` assigns `module.exports`
- **THEN** loading fails with an error naming `left-pad`, its `index.js`, and the importing line, saying bevy-ts loads ES modules only

#### Scenario: A Node built-in
- **WHEN** `src/game.ts` imports `node:fs` on line 1
- **THEN** loading fails with an error naming `node:fs`, `src/game.ts` and line 1, saying scripts have no Node APIs

#### Scenario: A dynamic import outside the graph
- **WHEN** a module runs `await import(new URL("../plugins/x/package.json", import.meta.url), { with: { type: "json" } })` inside `try`
- **THEN** the promise rejects, the `catch` runs, and the game keeps running

### Requirement: Package code is library code
A module SHALL be game code when its real path is under the project root and not under a `node_modules` folder, and package code otherwise. Package code SHALL get no declaration pass. A package module that imports `bevy-ts` and declares a data class (a component, resource, message or event), a `States` or `SystemSet` class, or a `system(...)`, SHALL fail the load with its file and line, naming the package and saying packages cannot declare ECS types yet. A package MAY import `bevy-ts` for anything else.

#### Scenario: A package declares a component
- **WHEN** a package module exports `class Health extends Component { hp: f32 = 1 }` imported from `bevy-ts`
- **THEN** loading fails with that module's file and line, naming the package
