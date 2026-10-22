## MODIFIED Requirements

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

## ADDED Requirements

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
